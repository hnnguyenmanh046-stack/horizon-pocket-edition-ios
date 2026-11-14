//! The last direct way between a host and a guest, tried only when their TCP race over the
//! relay or the public tunnel found nothing: UDP.
//!
//! Each end binds one UDP socket and learns what its NAT shows of it (STUN, from that socket).
//! The two swap those addresses over the protected stream and send each other probes from it,
//! so that each NAT sees its own side "call out" and lets the other's packets in (hole
//! punching). A probe answered names an address that works both ways. QUIC then carries the
//! session over that pair of sockets (TLS 1.3, the host's certificate pinned by a hash the
//! protected stream carried), and its stream is bridged to a loopback TCP stream: everything
//! after (the session's own encryption, the game) is as over any direct stream.
use crate::crypto::{Key, Role};
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
};
use sha2::{Digest, Sha256};
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, channel},
    },
    time::{Duration, Instant},
};

/// How long the ends send probes before they give UDP up.
pub const PROBE: Duration = Duration::from_millis(3000);
const PROBE_EVERY: Duration = Duration::from_millis(100);
/// First, probes that live only a few hops: they open this end's own NAT (its mapping toward
/// the other end) but die on the way, before the other end's NAT. A NAT that a probe reaches
/// before its own side has sent anything takes it for a stranger, and on Linux (most home
/// routers) that makes it map its own side's probes to another port, so the two never meet.
/// The other end starts its probes up to one trip through the bridged stream later.
const OPENERS: Duration = Duration::from_millis(700);
/// A home router is the first hop, a carrier's NAT a few more; the other end's NAT is many
/// hops further on the internet.
const OPENER_TTLS: [u32; 4] = [2, 3, 4, 5];
/// QUIC's handshake and first stream, once a probe went both ways.
pub const CONNECT: Duration = Duration::from_secs(5);
const STUN_WAIT: Duration = Duration::from_millis(1200);
/// Keeps the NATs' mappings open while the game is quiet (most keep a UDP one 30 s or more).
const KEEP_ALIVE: Duration = Duration::from_secs(5);
/// The other end is gone after this long without a packet.
const IDLE: Duration = Duration::from_secs(30);
const MAGIC: &[u8; 4] = b"HZU1";
const PROBE_KIND: u8 = 0;
const ACK_KIND: u8 = 1;
const PACKET: usize = 4 + 1 + 1 + 8 + 16;
/// What everything else, QUIC included, is sent with.
const NORMAL_TTL: u32 = 64;
const ALPN: &[u8] = b"horizon-friends/1";
const SERVER_NAME: &str = "horizon-friends";
/// The first bytes of the guest's QUIC stream: the host keeps only a stream that starts so.
const STREAM_MARK: &[u8; 4] = b"HZQ1";

/// What UDP needs from the end's settings.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// STUN servers, asked from the UDP socket itself.
    pub stun: Vec<String>,
    /// Loopback addresses are candidates too (tests on one machine).
    pub loopback: bool,
}

/// An end's UDP socket for one session, and where the other end may reach it.
pub struct Socket {
    socket: UdpSocket,
    /// IPv6 with IPv4 mapped into it: IPv4 addresses go out mapped.
    v6: bool,
    pub candidates: Vec<SocketAddr>,
}

/// What [`Socket::punch`] found.
#[derive(Debug, Default)]
pub struct Punched {
    /// The first of the other end's addresses whose answer came back.
    pub confirmed: Option<SocketAddr>,
    /// Every address the other end's probes came from.
    pub heard: Vec<SocketAddr>,
}

/// The form a socket of this kind sends to.
fn wire(addr: SocketAddr, v6: bool) -> SocketAddr {
    match addr {
        SocketAddr::V4(a) if v6 => SocketAddr::new(IpAddr::V6(a.ip().to_ipv6_mapped()), a.port()),
        other => other,
    }
}

/// An address as it is compared and shown: IPv4 mapped into IPv6 is IPv4.
fn canonical(addr: SocketAddr) -> SocketAddr {
    SocketAddr::new(addr.ip().to_canonical(), addr.port())
}

fn dual_stack() -> io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket as Raw, Type};
    let s = Raw::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_only_v6(false)?;
    s.bind(&SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0).into())?;
    Ok(s.into())
}

/// The probe and its answer for one end of one session, keyed by the session key, so that
/// nobody without it can pass for the other end.
fn packet(key: &Key, kind: u8, from: Role, session: u64) -> [u8; PACKET] {
    let role = match from {
        Role::Host => 0u8,
        Role::Guest => 1u8,
    };
    let mut p = [0u8; PACKET];
    p[..4].copy_from_slice(MAGIC);
    p[4] = kind;
    p[5] = role;
    p[6..14].copy_from_slice(&session.to_be_bytes());
    let mut label = b"horizon-friends v1 udp probe".to_vec();
    label.extend_from_slice(&p[..14]);
    let tag = key.derive(&label);
    p[14..].copy_from_slice(&tag[..16]);
    p
}

fn other(role: Role) -> Role {
    match role {
        Role::Host => Role::Guest,
        Role::Guest => Role::Host,
    }
}

impl Socket {
    /// A socket of this end's: IPv6 and IPv4 where the system has both, else IPv4. Its
    /// candidates are what STUN saw of it (the address the other end aims at from outside),
    /// then the interface addresses on its port.
    pub fn bind(options: &Options) -> io::Result<Self> {
        let addresses = crate::reach::interface_addresses(options.loopback);
        let v4 = || UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0));
        // Both families on one socket only where this device has an IPv6 address: elsewhere an
        // IPv6 socket may send nothing at all, IPv4 mapped into it included.
        let dual = if addresses.iter().any(IpAddr::is_ipv6) {
            dual_stack().ok()
        } else {
            None
        };
        let (mut socket, mut v6) = match dual {
            Some(s) => (s, true),
            None => (v4()?, false),
        };
        let mut mapped = crate::stun::mapped_on(&socket, v6, &options.stun, STUN_WAIT);
        if mapped.is_none() && v6 && !options.stun.is_empty() {
            // The IPv6 socket reached no STUN server: an IPv4 one may.
            let plain = v4()?;
            if let Some(m) = crate::stun::mapped_on(&plain, false, &options.stun, STUN_WAIT) {
                (socket, v6, mapped) = (plain, false, Some(m));
            }
        }
        let port = socket.local_addr()?.port();
        let mut candidates = Vec::new();
        if let Some(mapped) = mapped {
            candidates.push(canonical(mapped));
        }
        crate::trace!(
            "udp: socket {} ({}), mapped {mapped:?}",
            socket.local_addr()?,
            if v6 { "IPv6 and IPv4" } else { "IPv4" }
        );
        for ip in addresses {
            let a = SocketAddr::new(ip, port);
            if (v6 || ip.is_ipv4()) && !candidates.contains(&a) {
                candidates.push(a);
            }
        }
        candidates.truncate(crate::invite::MAX_ENDPOINTS);
        Ok(Self {
            socket,
            v6,
            candidates,
        })
    }

    /// Sets how many hops what this socket sends next lives, for IPv4 and for IPv6.
    fn set_ttl(&self, ttl: u32) {
        let _ = self.socket.set_ttl(ttl);
        if self.v6 {
            let _ = socket2::SockRef::from(&self.socket).set_unicast_hops_v6(ttl);
        }
    }

    /// Sends `data` to `to` with `ttl` hops to live; a target that can't be sent to (no route)
    /// is left, the others go on.
    fn send(&self, data: &[u8], to: SocketAddr, ttl: u32) {
        self.set_ttl(ttl);
        let _ = self.socket.send_to(data, wire(to, self.v6));
    }

    /// Probe the other end at `targets` from this socket, answering every probe of its,
    /// until `until` passes or `stop` is set; the guest (`keep` false) stops at the first
    /// answer, the host keeps answering until it is told to stop.
    pub fn punch(
        &self,
        targets: &[SocketAddr],
        key: &Key,
        role: Role,
        session: u64,
        until: Instant,
        stop: &AtomicBool,
        keep: bool,
    ) -> io::Result<Punched> {
        let mine = packet(key, PROBE_KIND, role, session);
        let my_answer = packet(key, ACK_KIND, role, session);
        let theirs = packet(key, PROBE_KIND, other(role), session);
        let their_answer = packet(key, ACK_KIND, other(role), session);
        let targets: Vec<SocketAddr> = targets.iter().map(|t| canonical(*t)).collect();
        let mut found = Punched::default();
        let started = Instant::now();
        let mut next = started;
        let mut buf = [0u8; 1500];
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            if now >= until {
                break;
            }
            if now >= next {
                let openers = now.duration_since(started) < OPENERS;
                for t in &targets {
                    // One that can't be sent to (no route) is skipped; the others go on.
                    if openers {
                        for ttl in OPENER_TTLS {
                            self.send(&mine, *t, ttl);
                        }
                    } else {
                        self.send(&mine, *t, NORMAL_TTL);
                    }
                }
                next = now + PROBE_EVERY;
            }
            let wait = next
                .min(until)
                .saturating_duration_since(Instant::now())
                .clamp(Duration::from_millis(1), Duration::from_millis(20));
            self.socket.set_read_timeout(Some(wait))?;
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    let from = canonical(from);
                    let got = &buf[..n];
                    if got == theirs {
                        // An answer goes all the way: the other end's NAT let its probe out.
                        self.send(&my_answer, from, NORMAL_TTL);
                        if !found.heard.contains(&from) {
                            found.heard.push(from);
                        }
                    } else if got == their_answer && found.confirmed.is_none() {
                        found.confirmed = Some(from);
                        if !keep {
                            break;
                        }
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                // An ICMP error reported on the socket (a candidate nobody listens on).
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        self.socket.set_read_timeout(None)?;
        self.set_ttl(NORMAL_TTL);
        crate::trace!(
            "udp: {role:?} probed {targets:?}: answered by {:?}, heard from {:?}",
            found.confirmed,
            found.heard
        );
        Ok(found)
    }
}

/// The host's QUIC certificate for one session, made on the spot.
pub struct Certificate {
    der: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}
impl Certificate {
    pub fn new() -> io::Result<Self> {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec![SERVER_NAME.to_owned()])
                .map_err(io::Error::other)?;
        Ok(Self {
            der: cert.der().clone(),
            key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der())),
        })
    }
    /// What the guest pins: the certificate's SHA-256, in hex.
    pub fn pin(&self) -> String {
        hex(&Sha256::digest(self.der.as_ref()))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn transport() -> Arc<quinn::TransportConfig> {
    let mut t = quinn::TransportConfig::default();
    t.keep_alive_interval(Some(KEEP_ALIVE));
    t.max_idle_timeout(IDLE.try_into().ok());
    Arc::new(t)
}

/// Accepts only the certificate the protected stream named.
#[derive(Debug)]
struct Pinned {
    pin: String,
    provider: Arc<rustls::crypto::CryptoProvider>,
}
impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if hex(&Sha256::digest(end_entity.as_ref())) == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("not the host's certificate".into()))
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn server_config(cert: &Certificate) -> io::Result<quinn::ServerConfig> {
    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?
        .with_no_client_auth()
        .with_single_cert(vec![cert.der.clone()], cert.key.clone_key())
        .map_err(io::Error::other)?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto =
        quinn::crypto::rustls::QuicServerConfig::try_from(tls).map_err(io::Error::other)?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    config.transport_config(transport());
    Ok(config)
}

fn client_config(pin: &str) -> io::Result<quinn::ClientConfig> {
    let provider = provider();
    let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned {
            pin: pin.to_owned(),
            provider,
        }))
        .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto =
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(io::Error::other)?;
    let mut config = quinn::ClientConfig::new(Arc::new(crypto));
    config.transport_config(transport());
    Ok(config)
}

/// Two ends of a loopback TCP connection, made here: (the one handed on, the one bridged).
fn loopback_pair() -> io::Result<(TcpStream, TcpStream)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let outer = TcpStream::connect(listener.local_addr()?)?;
    let mine = outer.local_addr()?;
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match listener.accept() {
            // Only our own connection, whoever else finds the port meanwhile.
            Ok((inner, from)) if from == mine => {
                inner.set_nonblocking(false)?;
                let _ = outer.set_nodelay(true);
                return Ok((outer, inner));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "no loopback"));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Carry bytes both ways between the loopback stream and the QUIC stream until either ends,
/// then give the other a moment to finish.
async fn bridge(inner: TcpStream, mut send: quinn::SendStream, mut recv: quinn::RecvStream) {
    let Ok(()) = inner.set_nonblocking(true) else {
        return;
    };
    let Ok(tcp) = tokio::net::TcpStream::from_std(inner) else {
        return;
    };
    let _ = tcp.set_nodelay(true);
    let (mut r, mut w) = tcp.into_split();
    let up = async {
        let _ = tokio::io::copy(&mut r, &mut send).await;
        let _ = send.finish();
    };
    let down = async {
        let _ = tokio::io::copy(&mut recv, &mut w).await;
        let _ = tokio::io::AsyncWriteExt::shutdown(&mut w).await;
    };
    tokio::pin!(up, down);
    tokio::select! {
        () = &mut up => { let _ = tokio::time::timeout(Duration::from_secs(10), down).await; }
        () = &mut down => { let _ = tokio::time::timeout(Duration::from_secs(10), up).await; }
    }
}

fn runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// The host's side: QUIC on `socket` for the first connection from one of `from` (the
/// addresses the guest's probes came from) whose first stream starts with the mark. The
/// stream comes back as a loopback TCP stream; a thread carries it until it ends.
pub fn serve(
    socket: Socket,
    cert: &Certificate,
    from: Vec<SocketAddr>,
) -> io::Result<Receiver<io::Result<TcpStream>>> {
    let config = server_config(cert)?;
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("horizon-friends-quic".into())
        .spawn(move || {
            let rt = match runtime() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };
            rt.block_on(async move {
                let endpoint = match quinn::Endpoint::new(
                    quinn::EndpointConfig::default(),
                    Some(config),
                    socket.socket,
                    Arc::new(quinn::TokioRuntime),
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                let accepted = tokio::time::timeout(CONNECT, async {
                    loop {
                        let incoming = endpoint.accept().await?;
                        if !from.contains(&canonical(incoming.remote_address())) {
                            incoming.refuse();
                            continue;
                        }
                        let Ok(connection) = incoming.await else {
                            continue;
                        };
                        let Ok((send, mut recv)) = connection.accept_bi().await else {
                            continue;
                        };
                        let mut mark = [0u8; 4];
                        if recv.read_exact(&mut mark).await.is_ok() && &mark == STREAM_MARK {
                            return Some((connection, send, recv));
                        }
                        connection.close(1u32.into(), b"not this session");
                    }
                })
                .await;
                let Ok(Some((connection, send, recv))) = accepted else {
                    let _ = tx.send(Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "no QUIC connection",
                    )));
                    endpoint.close(0u32.into(), b"");
                    return;
                };
                match loopback_pair() {
                    Ok((outer, inner)) => {
                        let _ = tx.send(Ok(outer));
                        bridge(inner, send, recv).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
                connection.close(0u32.into(), b"done");
                let _ = tokio::time::timeout(Duration::from_secs(1), endpoint.wait_idle()).await;
            });
        })?;
    Ok(rx)
}

/// The guest's side: QUIC from `socket` to the host at `to`, its certificate pinned; the
/// stream as a loopback TCP stream (a thread carries it until it ends). Blocks for the
/// handshake, [`CONNECT`] at most.
pub fn connect(socket: Socket, to: SocketAddr, pin: &str) -> io::Result<TcpStream> {
    let config = client_config(pin)?;
    let to = wire(to, socket.v6);
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("horizon-friends-quic".into())
        .spawn(move || {
            let rt = match runtime() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };
            rt.block_on(async move {
                let endpoint = match quinn::Endpoint::new(
                    quinn::EndpointConfig::default(),
                    None,
                    socket.socket,
                    Arc::new(quinn::TokioRuntime),
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                let opened = tokio::time::timeout(CONNECT, async {
                    let connection = endpoint
                        .connect_with(config, to, SERVER_NAME)
                        .map_err(io::Error::other)?
                        .await
                        .map_err(io::Error::other)?;
                    let (mut send, recv) = connection.open_bi().await.map_err(io::Error::other)?;
                    send.write_all(STREAM_MARK)
                        .await
                        .map_err(io::Error::other)?;
                    io::Result::Ok((connection, send, recv))
                })
                .await;
                let (connection, send, recv) = match opened {
                    Ok(Ok(found)) => found,
                    Ok(Err(e)) => {
                        let _ = tx.send(Err(e));
                        endpoint.close(0u32.into(), b"");
                        return;
                    }
                    Err(_) => {
                        let _ = tx.send(Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "the QUIC handshake didn't finish",
                        )));
                        endpoint.close(0u32.into(), b"");
                        return;
                    }
                };
                match loopback_pair() {
                    Ok((outer, inner)) => {
                        let _ = tx.send(Ok(outer));
                        bridge(inner, send, recv).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
                connection.close(0u32.into(), b"done");
                let _ = tokio::time::timeout(Duration::from_secs(1), endpoint.wait_idle()).await;
            });
        })?;
    rx.recv_timeout(CONNECT + Duration::from_secs(1))
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "no QUIC connection"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Key {
        Key::for_tests([7u8; 32])
    }

    #[test]
    fn probes_differ_by_kind_end_session_and_key() {
        let k = key();
        let p = packet(&k, PROBE_KIND, Role::Host, 1);
        assert_ne!(p, packet(&k, ACK_KIND, Role::Host, 1));
        assert_ne!(p, packet(&k, PROBE_KIND, Role::Guest, 1));
        assert_ne!(p, packet(&k, PROBE_KIND, Role::Host, 2));
        assert_ne!(
            p,
            packet(&Key::for_tests([8u8; 32]), PROBE_KIND, Role::Host, 1)
        );
        assert_eq!(p, packet(&k, PROBE_KIND, Role::Host, 1));
    }

    #[test]
    fn mapped_and_plain_addresses_compare_alike() {
        let v4: SocketAddr = "203.0.113.5:4000".parse().unwrap();
        let mapped = wire(v4, true);
        assert!(mapped.is_ipv6());
        assert_eq!(canonical(mapped), v4);
        assert_eq!(wire(v4, false), v4);
    }

    fn pair() -> (Socket, Socket) {
        let options = Options {
            stun: Vec::new(),
            loopback: true,
        };
        (
            Socket::bind(&options).unwrap(),
            Socket::bind(&options).unwrap(),
        )
    }

    #[test]
    fn two_sockets_find_each_other_and_quic_carries_bytes_both_ways() {
        use std::io::{Read, Write};
        let (host, guest) = pair();
        assert!(host.candidates.iter().any(|c| c.ip().is_loopback()));
        let (k, session) = (key(), 42);
        let (hc, gc) = (host.candidates.clone(), guest.candidates.clone());
        let stop = Arc::new(AtomicBool::new(false));
        let (hk, hstop) = (k.clone(), stop.clone());
        let hosting = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            let found = host
                .punch(&gc, &hk, Role::Host, session, until, &hstop, true)
                .unwrap();
            (host, found)
        });
        let until = Instant::now() + PROBE;
        let never = AtomicBool::new(false);
        let found = guest
            .punch(&hc, &k, Role::Guest, session, until, &never, false)
            .unwrap();
        let at = found.confirmed.expect("an answer from the host");
        stop.store(true, Ordering::Relaxed);
        let (host, heard) = hosting.join().unwrap();
        assert!(!heard.heard.is_empty());
        let cert = Certificate::new().unwrap();
        let pin = cert.pin();
        let served = serve(host, &cert, heard.heard).unwrap();
        let mut g = connect(guest, at, &pin).unwrap();
        let mut h = served
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        g.write_all(b"hello host").unwrap();
        let mut got = [0u8; 10];
        h.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"hello host");
        let big: Vec<u8> = (0..1_000_000u32).map(|i| (i * 31 % 251) as u8).collect();
        let mut hw = h.try_clone().unwrap();
        let sent = big.clone();
        let writer = std::thread::spawn(move || hw.write_all(&sent).unwrap());
        let mut back = vec![0u8; big.len()];
        g.read_exact(&mut back).unwrap();
        writer.join().unwrap();
        assert!(back == big);
        // Closing one end ends the other.
        drop(g);
        h.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
        let mut rest = [0u8; 1];
        assert_eq!(h.read(&mut rest).unwrap(), 0);
    }

    #[test]
    fn a_wrong_pin_is_refused() {
        let (host, guest) = pair();
        let (k, session) = (key(), 7);
        let (hc, gc) = (host.candidates.clone(), guest.candidates.clone());
        let stop = Arc::new(AtomicBool::new(false));
        let (hk, hstop) = (k.clone(), stop.clone());
        let hosting = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            let found = host
                .punch(&gc, &hk, Role::Host, session, until, &hstop, true)
                .unwrap();
            (host, found)
        });
        let never = AtomicBool::new(false);
        let at = guest
            .punch(
                &hc,
                &k,
                Role::Guest,
                session,
                Instant::now() + PROBE,
                &never,
                false,
            )
            .unwrap()
            .confirmed
            .unwrap();
        stop.store(true, Ordering::Relaxed);
        let (host, heard) = hosting.join().unwrap();
        let cert = Certificate::new().unwrap();
        let other = Certificate::new().unwrap();
        let _served = serve(host, &cert, heard.heard).unwrap();
        assert!(connect(guest, at, &other.pin()).is_err());
    }

    #[test]
    fn another_sessions_probes_are_not_answered() {
        let (host, guest) = pair();
        let k = key();
        let hc = host.candidates.clone();
        let gc = guest.candidates.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let (hk, hstop) = (k.clone(), stop.clone());
        let hosting = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(3);
            host.punch(&gc, &hk, Role::Host, 1, until, &hstop, true)
                .unwrap()
        });
        let never = AtomicBool::new(false);
        let found = guest
            .punch(
                &hc,
                &k,
                Role::Guest,
                2,
                Instant::now() + Duration::from_millis(800),
                &never,
                false,
            )
            .unwrap();
        stop.store(true, Ordering::Relaxed);
        assert_eq!(found.confirmed, None);
        assert!(hosting.join().unwrap().heard.is_empty());
    }
}
