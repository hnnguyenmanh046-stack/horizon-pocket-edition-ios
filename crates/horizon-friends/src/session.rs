//! What both ends do once the relay (or the host's public tunnel) has bridged them and the key
//! is agreed: protect the bridged stream, swap candidates over it, race direct TCP streams for
//! [`direct::RACE`]; only when that finds nothing, and both ends can, try UDP
//! ([`crate::udp`]); and settle on one. The guest decides and says so; the host follows.
use crate::{
    PathMode,
    crypto::{self, Frame, Key, Role, SecureReader, SecureWriter},
    direct, udp,
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    net::{SocketAddr, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(crate) type Secure = (SecureReader<TcpStream>, SecureWriter<TcpStream>);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum Control {
    /// Where this end can be reached; `direct` false when it won't try. `udp`: it can try
    /// UDP after the TCP race (an older end leaves it out).
    Hello {
        direct: bool,
        candidates: Vec<String>,
        #[serde(default)]
        udp: bool,
    },
    /// Sent by the guest as the first frame of the direct stream it keeps.
    Use,
    /// The guest's decision, on the bridged stream; `udp` when the stream it keeps is the UDP
    /// one.
    Path {
        direct: bool,
        #[serde(default)]
        udp: bool,
    },
    /// After a TCP race that found nothing: this end's UDP candidates (none: it can't), and
    /// from the host the pin of its QUIC certificate.
    Udp {
        candidates: Vec<String>,
        #[serde(default)]
        pin: String,
    },
    /// The guest heard the host's UDP answer: the host starts QUIC.
    UdpGo,
    /// The host's answer to [`Control::UdpGo`]: its QUIC endpoint listens (`ok`) or couldn't.
    UdpReady { ok: bool },
}

fn send(w: &mut SecureWriter<TcpStream>, c: &Control) -> io::Result<()> {
    w.write_control(&serde_json::to_vec(c).map_err(io::Error::other)?)
}
fn recv(r: &mut SecureReader<TcpStream>) -> io::Result<Control> {
    match r.read_frame()? {
        Some(Frame::Control(c)) => {
            serde_json::from_slice(&c).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        }
        Some(Frame::Data(_)) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "data before the path",
        )),
        None => Err(io::ErrorKind::UnexpectedEof.into()),
    }
}
fn unexpected(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("expected {what}"))
}

pub(crate) fn secure(stream: TcpStream, key: &Key, role: Role) -> io::Result<Secure> {
    crypto::secure(stream.try_clone()?, stream, key, role)
}

pub(crate) struct Established {
    pub secure: Secure,
    pub direct: bool,
    /// The direct stream is the UDP one.
    pub udp: bool,
}

/// Start accepting this session's inbound direct streams (already
/// introduced) into the sender until the deadline or the stop flag.
pub(crate) type Listen<'a> = Box<dyn FnOnce(Sender<TcpStream>, Instant, Arc<AtomicBool>) + 'a>;

/// Whether the guest races for a direct TCP stream (`HORIZON_FRIENDS_TCP_RACE=0` skips it,
/// so tests on one machine, where TCP always connects, reach the UDP step).
fn tcp_race() -> bool {
    std::env::var("HORIZON_FRIENDS_TCP_RACE").as_deref() != Ok("0")
}

/// A direct stream protected as the session's own: the host keeps it only once the guest
/// marked it.
fn authenticate(raw: TcpStream, key: &Key, role: Role) -> Option<Secure> {
    raw.set_read_timeout(Some(Duration::from_secs(6))).ok()?;
    let mut s = secure(raw, key, role).ok()?;
    if role == Role::Host && !matches!(recv(&mut s.0), Ok(Control::Use)) {
        return None;
    }
    Some(s)
}

fn parse(list: &[String]) -> Vec<SocketAddr> {
    list.iter()
        .filter_map(|a| a.parse().ok())
        .take(16)
        .collect()
}

/// Settle a session on the bridged stream `relay` (already protected) or a direct one. `udp`:
/// the settings for the UDP step, None to never take it.
pub(crate) fn settle(
    relay: Secure,
    key: &Key,
    role: Role,
    mode: PathMode,
    session: u64,
    direct_port: Option<u16>,
    candidates: Vec<SocketAddr>,
    listen: Listen<'_>,
    udp_options: Option<udp::Options>,
) -> io::Result<Established> {
    let (mut rr, mut rw) = relay;
    // The UDP step can take a few seconds past the TCP race.
    rr.get_ref()
        .set_read_timeout(Some(Duration::from_secs(20)))?;
    let wants = mode != PathMode::RelayOnly && direct_port.is_some();
    let wants_udp = mode != PathMode::RelayOnly && udp_options.is_some();
    send(
        &mut rw,
        &Control::Hello {
            direct: wants,
            candidates: if wants {
                candidates.iter().map(SocketAddr::to_string).collect()
            } else {
                Vec::new()
            },
            udp: wants_udp,
        },
    )?;
    let Control::Hello {
        direct: they_want,
        candidates: theirs,
        udp: they_udp,
    } = recv(&mut rr)?
    else {
        return Err(unexpected("hello"));
    };
    let theirs = parse(&theirs);
    let both_udp = wants_udp && they_udp;
    crate::trace!(
        "{role:?}: direct {wants}/{they_want}, udp {wants_udp}/{they_udp}, theirs {theirs:?}"
    );
    let stop = Arc::new(AtomicBool::new(false));
    let (atx, arx) = channel::<Secure>();
    let race = wants && they_want && (role == Role::Host || tcp_race());
    if race {
        let deadline = Instant::now() + direct::RACE;
        let (raw_tx, raw_rx) = channel::<TcpStream>();
        listen(raw_tx.clone(), deadline, stop.clone());
        direct::dial(direct_port, theirs, session, deadline, stop.clone(), raw_tx);
        let key = key.clone();
        let _ = std::thread::Builder::new()
            .name("horizon-friends-auth".into())
            .spawn(move || {
                for raw in raw_rx {
                    let (key, atx) = (key.clone(), atx.clone());
                    let _ = std::thread::Builder::new()
                        .name("horizon-friends-auth".into())
                        .spawn(move || {
                            if let Some(s) = authenticate(raw, &key, role) {
                                let _ = atx.send(s);
                            }
                        });
                }
            });
    } else {
        drop(atx);
    }
    let finish = |(r, w): Secure, direct: bool, udp: bool| -> io::Result<Established> {
        r.get_ref().set_read_timeout(None)?;
        Ok(Established {
            secure: (r, w),
            direct,
            udp,
        })
    };
    let fallback = |rr: SecureReader<TcpStream>, rw: SecureWriter<TcpStream>| {
        if mode == PathMode::DirectOnly {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "no direct connection",
            ))
        } else {
            finish((rr, rw), false, false)
        }
    };
    match role {
        Role::Guest => {
            let chosen = if race {
                let left = direct::RACE + Duration::from_millis(500);
                arx.recv_timeout(left).ok()
            } else {
                None
            };
            stop.store(true, Ordering::Relaxed);
            if let Some((r, mut w)) = chosen
                && send(&mut w, &Control::Use).is_ok()
            {
                send(
                    &mut rw,
                    &Control::Path {
                        direct: true,
                        udp: false,
                    },
                )?;
                let _ = rr.get_ref().shutdown(std::net::Shutdown::Both);
                crate::trace!("guest: a direct TCP stream");
                return finish((r, w), true, false);
            }
            // TCP found nothing: UDP, when both ends can.
            if let (true, Some(options)) = (both_udp, &udp_options)
                && let Some(raw) = guest_udp(&mut rr, &mut rw, key, session, options)
                    .ok()
                    .flatten()
                && let Some((r, mut w)) = authenticate(raw, key, Role::Guest)
                && send(&mut w, &Control::Use).is_ok()
            {
                send(
                    &mut rw,
                    &Control::Path {
                        direct: true,
                        udp: true,
                    },
                )?;
                let _ = rr.get_ref().shutdown(std::net::Shutdown::Both);
                return finish((r, w), true, true);
            }
            send(
                &mut rw,
                &Control::Path {
                    direct: false,
                    udp: false,
                },
            )?;
            fallback(rr, rw)
        }
        Role::Host => {
            let mut udp_step = HostUdp::default();
            let result = loop {
                match recv(&mut rr) {
                    Err(e) => break Err(e),
                    Ok(Control::Path { direct: true, udp }) => {
                        let chosen = if udp {
                            udp_step
                                .secured
                                .take()
                                .and_then(|rx| rx.recv_timeout(Duration::from_secs(5)).ok())
                        } else {
                            arx.recv_timeout(Duration::from_secs(5)).ok()
                        };
                        stop.store(true, Ordering::Relaxed);
                        let _ = rr.get_ref().shutdown(std::net::Shutdown::Both);
                        let Some(chosen) = chosen else {
                            break Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "the direct stream vanished",
                            ));
                        };
                        break finish(chosen, true, udp);
                    }
                    Ok(Control::Path { direct: false, .. }) => {
                        stop.store(true, Ordering::Relaxed);
                        udp_step.stop();
                        break fallback(rr, rw);
                    }
                    Ok(Control::Udp {
                        candidates: theirs, ..
                    }) => {
                        let answer = match (&udp_options, both_udp) {
                            (Some(options), true) => udp_step.start(options, &theirs, key, session),
                            _ => None,
                        };
                        let (candidates, pin) = answer.unwrap_or_default();
                        if let Err(e) = send(&mut rw, &Control::Udp { candidates, pin }) {
                            break Err(e);
                        }
                    }
                    Ok(Control::UdpGo) => {
                        let ok = udp_step.serve(key);
                        if let Err(e) = send(&mut rw, &Control::UdpReady { ok }) {
                            break Err(e);
                        }
                    }
                    Ok(_) => break Err(unexpected("the path")),
                }
            };
            udp_step.stop();
            result
        }
    }
}

/// The guest's UDP step: its candidates to the host, the host's back, probes until one is
/// answered; then the host starts QUIC and the guest connects. None (and the host is told so
/// by the path that follows) when any of it doesn't work.
fn guest_udp(
    rr: &mut SecureReader<TcpStream>,
    rw: &mut SecureWriter<TcpStream>,
    key: &Key,
    session: u64,
    options: &udp::Options,
) -> io::Result<Option<TcpStream>> {
    let Ok(socket) = udp::Socket::bind(options) else {
        return Ok(None);
    };
    send(
        rw,
        &Control::Udp {
            candidates: socket
                .candidates
                .iter()
                .map(SocketAddr::to_string)
                .collect(),
            pin: String::new(),
        },
    )?;
    let Control::Udp {
        candidates: theirs,
        pin,
    } = recv(rr)?
    else {
        return Err(unexpected("the host's UDP candidates"));
    };
    let theirs = parse(&theirs);
    if theirs.is_empty() || pin.is_empty() {
        return Ok(None);
    }
    let never = AtomicBool::new(false);
    crate::trace!("guest: udp, the host's {theirs:?}");
    let punched = socket.punch(
        &theirs,
        key,
        Role::Guest,
        session,
        Instant::now() + udp::PROBE,
        &never,
        false,
    )?;
    let Some(at) = punched.confirmed else {
        return Ok(None);
    };
    send(rw, &Control::UdpGo)?;
    match recv(rr)? {
        Control::UdpReady { ok: true } => {}
        Control::UdpReady { ok: false } => return Ok(None),
        _ => return Err(unexpected("the host's QUIC")),
    }
    let connected = udp::connect(socket, at, &pin);
    crate::trace!("guest: QUIC to {at}: {:?}", connected.as_ref().map(|_| ()));
    Ok(connected.ok())
}

/// The host's side of the UDP step, as the guest's messages come.
#[derive(Default)]
struct HostUdp {
    /// Probing and answering the guest's probes until told to stop.
    punching: Option<(
        JoinHandle<Option<(udp::Socket, udp::Punched)>>,
        Arc<AtomicBool>,
    )>,
    cert: Option<udp::Certificate>,
    /// The QUIC stream, protected, once the guest connected.
    secured: Option<Receiver<Secure>>,
}

impl HostUdp {
    /// Bind, make the certificate and start probing the guest's `theirs`: what to answer with
    /// (this end's candidates, the certificate's pin). None: UDP can't be tried.
    fn start(
        &mut self,
        options: &udp::Options,
        theirs: &[String],
        key: &Key,
        session: u64,
    ) -> Option<(Vec<String>, String)> {
        if self.punching.is_some() || self.secured.is_some() {
            return None;
        }
        let theirs = parse(theirs);
        if theirs.is_empty() {
            return None;
        }
        let socket = udp::Socket::bind(options).ok()?;
        let cert = udp::Certificate::new().ok()?;
        let answer = (
            socket
                .candidates
                .iter()
                .map(SocketAddr::to_string)
                .collect(),
            cert.pin(),
        );
        let stop = Arc::new(AtomicBool::new(false));
        let (key, flag) = (key.clone(), stop.clone());
        // The guest decides within its probe time; this end answers a little longer.
        let until = Instant::now() + udp::PROBE + Duration::from_secs(3);
        let thread = std::thread::Builder::new()
            .name("horizon-friends-punch".into())
            .spawn(move || {
                let punched = socket
                    .punch(&theirs, &key, Role::Host, session, until, &flag, true)
                    .ok()?;
                Some((socket, punched))
            })
            .ok()?;
        self.punching = Some((thread, stop));
        self.cert = Some(cert);
        Some(answer)
    }

    /// The guest heard this end: stop probing and serve QUIC on the socket, for the addresses
    /// the guest's probes came from. False when that can't be done.
    fn serve(&mut self, key: &Key) -> bool {
        let Some((thread, stop)) = self.punching.take() else {
            return false;
        };
        stop.store(true, Ordering::Relaxed);
        let (Ok(Some((socket, punched))), Some(cert)) = (thread.join(), self.cert.take()) else {
            return false;
        };
        if punched.heard.is_empty() {
            return false;
        }
        let Ok(streams) = udp::serve(socket, &cert, punched.heard) else {
            return false;
        };
        let (tx, rx) = channel();
        let key = key.clone();
        let spawned = std::thread::Builder::new()
            .name("horizon-friends-auth".into())
            .spawn(move || {
                if let Ok(Ok(raw)) = streams.recv_timeout(udp::CONNECT + Duration::from_secs(2))
                    && let Some(s) = authenticate(raw, &key, Role::Host)
                {
                    let _ = tx.send(s);
                }
            });
        if spawned.is_err() {
            return false;
        }
        self.secured = Some(rx);
        true
    }

    fn stop(&mut self) {
        if let Some((thread, stop)) = self.punching.take() {
            stop.store(true, Ordering::Relaxed);
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_ends_messages_read_as_no_udp() {
        // As horizon-friends before the UDP step sends them.
        let hello: Control = serde_json::from_str(
            r#"{"t":"hello","direct":true,"candidates":["203.0.113.5:25580"]}"#,
        )
        .unwrap();
        assert!(matches!(
            hello,
            Control::Hello {
                direct: true,
                udp: false,
                ..
            }
        ));
        let path: Control = serde_json::from_str(r#"{"t":"path","direct":true}"#).unwrap();
        assert!(matches!(
            path,
            Control::Path {
                direct: true,
                udp: false
            }
        ));
        // And what this one sends, an older one reads (it ignores the fields it doesn't know).
        let text = serde_json::to_string(&Control::Path {
            direct: false,
            udp: false,
        })
        .unwrap();
        assert!(
            text.contains(r#""t":"path""#) && text.contains(r#""direct":false"#),
            "{text}"
        );
    }

    #[test]
    fn the_udp_messages_round_trip() {
        for c in [
            Control::Udp {
                candidates: vec!["198.51.100.1:40000".into()],
                pin: "ab".into(),
            },
            Control::UdpGo,
            Control::UdpReady { ok: true },
        ] {
            let text = serde_json::to_string(&c).unwrap();
            let back: Control = serde_json::from_str(&text).unwrap();
            assert_eq!(format!("{back:?}"), format!("{c:?}"));
        }
    }
}
