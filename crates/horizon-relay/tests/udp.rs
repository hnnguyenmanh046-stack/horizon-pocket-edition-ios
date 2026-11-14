//! The UDP step (horizon_friends::udp), on loopback in one process: a host behind a stand-in
//! for its public tunnel, or a self-hosted relay, and guests that skip the TCP race
//! (`HORIZON_FRIENDS_TCP_RACE=0`, for this test binary only: on one machine TCP always
//! connects, so UDP would never be reached). STUN is off; the candidates are loopback ones.
use horizon_friends::{
    Host, HostConfig, HostEvent, PathMode,
    guest::{self, GuestConfig},
    invite::{self, Invite, Parsed},
    reach,
};
use horizon_relay::{Config, Relay, limits::Rates};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Once,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const PROTOCOL: i32 = 777;

fn no_tcp_race() {
    static ONCE: Once = Once::new();
    // Before any thread of this binary reads it.
    ONCE.call_once(|| unsafe { std::env::set_var("HORIZON_FRIENDS_TCP_RACE", "0") });
}

fn echo_server() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            std::thread::spawn(move || {
                let mut out = s.try_clone().unwrap();
                let _ = std::io::copy(&mut s, &mut out);
            });
        }
    });
    a
}

fn options() -> reach::Options {
    reach::Options {
        map_port: false,
        stun: Vec::new(),
        loopback: true,
    }
}

/// A host whose public tunnel is a loopback port carrying every connection to its direct port.
fn tunneled_host(path: PathMode) -> (Host, Arc<AtomicUsize>) {
    let carried = Arc::new(AtomicUsize::new(0));
    let count = carried.clone();
    let open: horizon_friends::host::Tunnel = Arc::new(move |port: u16| {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let address = listener.local_addr().ok()?;
        let count = count.clone();
        std::thread::spawn(move || {
            for visitor in listener.incoming() {
                let Ok(visitor) = visitor else { continue };
                count.fetch_add(1, Ordering::SeqCst);
                let Ok(host) = TcpStream::connect(("127.0.0.1", port)) else {
                    continue;
                };
                for (mut from, mut to) in [
                    (visitor.try_clone().unwrap(), host.try_clone().unwrap()),
                    (host, visitor),
                ] {
                    std::thread::spawn(move || {
                        let _ = std::io::copy(&mut from, &mut to);
                        let _ = to.shutdown(std::net::Shutdown::Both);
                        let _ = from.shutdown(std::net::Shutdown::Both);
                    });
                }
            }
        });
        Some((address, Box::new(()) as Box<dyn std::any::Any + Send>))
    });
    let host = Host::start(HostConfig {
        lan: echo_server(),
        protocol: PROTOCOL,
        game: "26.3".into(),
        relay: None,
        path,
        port: 0,
        reach: options(),
        tunnel: Some(open),
    })
    .unwrap();
    (host, carried)
}

fn wait<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(t) = f() {
            return t;
        }
        assert!(Instant::now() < deadline, "no {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The code, and an invite that reaches the host only through its tunnel.
fn through_the_tunnel(host: &Host) -> (SocketAddr, String) {
    let code = wait("code", || host.state().invite);
    let tunnel = wait("tunnel", || host.state().tunnel);
    let mut invite: Invite = match invite::parse(&code) {
        Some(Parsed::Long(i)) => i,
        other => panic!("{other:?}"),
    };
    invite.endpoints = vec![tunnel];
    (tunnel, invite.encode())
}

fn join(
    code: &str,
    relay: Option<&str>,
    tunnels: Vec<SocketAddr>,
    path: PathMode,
) -> guest::Tunnel {
    guest::join(&GuestConfig {
        code: code.to_owned(),
        relay: relay.map(str::to_owned),
        protocol: PROTOCOL,
        game: "26.3".into(),
        path,
        tunnels,
        stun: Vec::new(),
    })
    .unwrap()
}

/// (direct, tunnel, udp) of the host's next session.
fn carried_as(host: &Host) -> (bool, bool, bool) {
    wait("session", || {
        host.poll().into_iter().find_map(|e| match e {
            HostEvent::Connected {
                direct,
                tunnel,
                udp,
                ..
            } => Some((direct, tunnel, udp)),
            _ => None,
        })
    })
}

fn round_trip(tunnel: guest::Tunnel, payload: &[u8]) {
    let (port, done) = tunnel.serve_loopback().unwrap();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    let mut w = s.try_clone().unwrap();
    let send = payload.to_vec();
    let writer = std::thread::spawn(move || w.write_all(&send).unwrap());
    let mut got = vec![0u8; payload.len()];
    s.read_exact(&mut got).unwrap();
    writer.join().unwrap();
    assert!(got == payload, "echo differs");
    drop(s);
    done.join().unwrap();
}

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 7 + i / 251) as u8).collect()
}

#[test]
fn when_tcp_finds_nothing_udp_carries_the_game_and_the_tunnel_is_left() {
    no_tcp_race();
    let (host, carried) = tunneled_host(PathMode::Auto);
    let (tunnel, code) = through_the_tunnel(&host);
    let started = Instant::now();
    let joined = join(&code, None, vec![tunnel], PathMode::Auto);
    assert!(joined.direct && joined.udp && !joined.tunneled);
    assert_eq!(carried_as(&host), (true, false, true));
    assert_eq!(carried.load(Ordering::SeqCst), 1);
    assert!(
        started.elapsed() < Duration::from_secs(12),
        "{:?}",
        started.elapsed()
    );
    round_trip(joined, &payload(2 << 20));
    // A second guest at the same time, as well.
    let a = join(&code, None, vec![tunnel], PathMode::Auto);
    let b = join(&code, None, vec![tunnel], PathMode::Auto);
    assert!(a.udp && b.udp);
    let ta = std::thread::spawn(move || round_trip(a, &payload(300_000)));
    round_trip(b, &payload(200_000));
    ta.join().unwrap();
}

#[test]
fn a_guest_that_never_goes_direct_never_tries_udp() {
    no_tcp_race();
    let (host, _) = tunneled_host(PathMode::Auto);
    let (tunnel, code) = through_the_tunnel(&host);
    let joined = join(&code, None, vec![tunnel], PathMode::RelayOnly);
    assert!(!joined.direct && !joined.udp && joined.tunneled);
    assert_eq!(carried_as(&host), (false, true, false));
    round_trip(joined, &payload(100_000));
}

#[test]
fn a_relayed_session_takes_udp_too_when_tcp_finds_nothing() {
    no_tcp_race();
    let (_relay, address, _) = Relay::spawn_local(Config {
        rates: Rates {
            join_per_min: 600.0,
            join_burst: 100.0,
            ..Rates::default()
        },
        ..Config::default()
    })
    .unwrap();
    let address = address.to_string();
    let host = Host::start(HostConfig {
        lan: echo_server(),
        protocol: PROTOCOL,
        game: "26.3".into(),
        relay: Some(address.clone()),
        path: PathMode::Auto,
        port: 0,
        reach: options(),
        tunnel: None,
    })
    .unwrap();
    let short = wait("short code", || host.state().short).to_string();
    let joined = join(&short, Some(&address), Vec::new(), PathMode::Auto);
    assert!(joined.direct && joined.udp);
    assert_eq!(carried_as(&host), (true, false, true));
    round_trip(joined, &payload(500_000));
}
