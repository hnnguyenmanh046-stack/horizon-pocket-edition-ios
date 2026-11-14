//! One end of a shared world's connection for the NAT lab (tools/ci/natlab.sh), which runs a
//! host and a guest in network namespaces behind simulated home routers, with a real bore
//! server and a STUN server on the simulated internet between them.
//!
//!     natlab host  --bore IP --stun IP:PORT --out FILE
//!     natlab guest --code FILE --stun IP:PORT
//!
//! The host shares an echo server as the game (Play with Friends, the public tunnel through
//! bore as a shared world's host has it) and writes its code and tunnel address to FILE. The
//! guest joins the way a shared world's guest does (the tunnel marked), sends 1 MiB through,
//! checks the echo, and prints how its connection went as JSON.
use horizon_friends::{
    Host, HostConfig, HostEvent, PathMode,
    guest::{self, GuestConfig},
    reach,
};
use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

const PROTOCOL: i32 = 777;

fn arg(args: &[String], name: &str) -> String {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| panic!("{name} is missing"))
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

fn host(args: &[String]) {
    let bore: IpAddr = arg(args, "--bore").parse().unwrap();
    let stun = arg(args, "--stun");
    let out = arg(args, "--out");
    let open: horizon_friends::host::Tunnel = Arc::new(move |port: u16| {
        let server = SocketAddr::new(bore, horizon_shared_worlds::bore::CONTROL_PORT);
        let local = SocketAddr::from(([127, 0, 0, 1], port));
        match horizon_shared_worlds::bore::Tunnel::open_at(server, local) {
            Ok(t) => Some((t.address(), Box::new(t) as Box<dyn std::any::Any + Send>)),
            Err(e) => {
                eprintln!("host: no tunnel: {e}");
                None
            }
        }
    });
    let host = Host::start(HostConfig {
        lan: echo_server(),
        protocol: PROTOCOL,
        game: "26.3".into(),
        relay: None,
        path: PathMode::Auto,
        port: horizon_friends::host::DEFAULT_PORT,
        reach: reach::Options {
            map_port: false,
            stun: vec![stun],
            loopback: false,
        },
        tunnel: Some(open),
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let (code, tunnel) = loop {
        let s = host.state();
        if let (Some(code), Some(tunnel)) = (s.invite.clone(), s.tunnel) {
            break (code, tunnel);
        }
        assert!(Instant::now() < deadline, "host: no code and tunnel");
        std::thread::sleep(Duration::from_millis(50));
    };
    std::fs::write(&out, format!("{code}\n{tunnel}\n")).unwrap();
    eprintln!("host: code ready, tunnel {tunnel}, {}", host.state().summary);
    // Until the lab stops it: how each session is carried.
    let end = Instant::now() + Duration::from_secs(600);
    while Instant::now() < end {
        for e in host.poll() {
            if let HostEvent::Connected {
                direct,
                tunnel,
                udp,
                ..
            } = e
            {
                println!(r#"{{"host":{{"direct":{direct},"tunnel":{tunnel},"udp":{udp}}}}}"#);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn guest(args: &[String]) {
    let file = std::fs::read_to_string(arg(args, "--code")).unwrap();
    let mut lines = file.lines();
    let code = lines.next().unwrap().to_owned();
    let tunnel: SocketAddr = lines.next().unwrap().parse().unwrap();
    let started = Instant::now();
    let joined = guest::join(&GuestConfig {
        code,
        relay: None,
        protocol: PROTOCOL,
        game: "26.3".into(),
        path: PathMode::Auto,
        tunnels: vec![tunnel],
        stun: vec![arg(args, "--stun")],
    })
    .unwrap_or_else(|e| panic!("guest: couldn't join: {e}"));
    let took = started.elapsed().as_millis();
    let (direct, tunneled, udp) = (joined.direct, joined.tunneled, joined.udp);
    let (port, done) = joined.serve_loopback().unwrap();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let payload: Vec<u8> = (0..1usize << 20).map(|i| (i * 7 + i / 251) as u8).collect();
    let mut w = s.try_clone().unwrap();
    let sent = payload.clone();
    let writer = std::thread::spawn(move || w.write_all(&sent).unwrap());
    let mut back = vec![0u8; payload.len()];
    let echo_started = Instant::now();
    s.read_exact(&mut back).unwrap();
    writer.join().unwrap();
    assert!(back == payload, "guest: the echo differs");
    let echo = echo_started.elapsed().as_millis();
    drop(s);
    let _ = done.join();
    println!(
        r#"{{"direct":{direct},"tunneled":{tunneled},"udp":{udp},"join_ms":{took},"echo_ms":{echo}}}"#
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("host") => host(&args),
        Some("guest") => guest(&args),
        _ => panic!("natlab host|guest ..."),
    }
}
