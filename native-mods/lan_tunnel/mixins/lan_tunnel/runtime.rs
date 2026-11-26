// LAN Tunnel for Horizon: a world open to LAN gets a public address friends can
// join from anywhere, as the e4mc mod gives one, through the public bore
// tunnel server (https://github.com/ekzhang/bore, `bore.pub`).
//
// The host connects *out* to the server, which opens a port on its own address
// and hands every visitor back down a connection the host opens. The protocol
// (bore 0.5) is JSON messages ended by a zero byte on a control connection to
// port 7835: `{"Hello":0}` asks for any port and `{"Hello":<port>}` answers;
// the server then sends `{"Connection":"<id>"}` for each visitor, and the host
// opens a new connection, says `{"Accept":"<id>"}` and from then on that
// connection is the visitor's bytes, passed on to the LAN port.
//
// The server sees every byte (the game's stream is not encrypted), its speed
// and whether it is up are its own, and anyone who has the address can join
// the world: it is as public as a server on the internet.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const ID: &str = "lan_tunnel";
pub const NAME: &str = "LAN Tunnel";
pub const VERSION: &str = "0.1.0";
pub const DESCRIPTION: &str = "Gives a world open to LAN a public address friends can join.";

const SERVER: &str = "bore.pub";
const CONTROL_PORT: u16 = 7835;
const CONNECT: Duration = Duration::from_secs(10);
/// How often the control connection looks at whether the tunnel was closed.
const WAKE: Duration = Duration::from_secs(2);

/// What the tick should tell the player.
pub enum Note {
    Address(String),
    Failed(String),
}

type Outcome = Arc<Mutex<Option<Result<u16, String>>>>;

struct Tunnel {
    local: u16,
    stop: Arc<AtomicBool>,
    outcome: Outcome,
    told: bool,
}

static STATE: Mutex<Option<Tunnel>> = Mutex::new(None);

/// Once a tick with the port the world is open to LAN on (None if it is not):
/// opens, replaces or closes the tunnel, and returns what to say, once.
pub fn poll(lan: Option<u16>) -> Option<Note> {
    let mut state = STATE.lock().ok()?;
    match (lan, state.as_mut()) {
        (None, None) => None,
        (None, Some(tunnel)) => {
            tunnel.stop.store(true, Ordering::Relaxed);
            *state = None;
            None
        }
        (Some(port), None) => {
            *state = Some(start(port));
            None
        }
        (Some(port), Some(tunnel)) if tunnel.local != port => {
            tunnel.stop.store(true, Ordering::Relaxed);
            *state = Some(start(port));
            None
        }
        (Some(_), Some(tunnel)) => {
            if tunnel.told {
                return None;
            }
            let result = tunnel.outcome.lock().ok()?.clone()?;
            tunnel.told = true;
            Some(match result {
                Ok(port) => Note::Address(format!("{SERVER}:{port}")),
                Err(why) => Note::Failed(why),
            })
        }
    }
}

fn start(local: u16) -> Tunnel {
    let stop = Arc::new(AtomicBool::new(false));
    let outcome: Outcome = Arc::new(Mutex::new(None));
    let (s, o) = (stop.clone(), outcome.clone());
    std::thread::spawn(move || {
        if let Err(why) = run(local, &s, &o) {
            // Said once: an address already given stays the last word.
            if let Ok(mut slot) = o.lock() {
                if slot.is_none() {
                    *slot = Some(Err(why.to_string()));
                }
            }
        }
    });
    Tunnel { local, stop, outcome, told: false }
}

fn connect() -> io::Result<TcpStream> {
    let mut last = io::Error::other("no address");
    for addr in (SERVER, CONTROL_PORT).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, CONNECT) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn send(stream: &mut TcpStream, json: &str) -> io::Result<()> {
    let mut frame = json.as_bytes().to_vec();
    frame.push(0);
    stream.write_all(&frame)
}

/// The control connection: asks for a port, reports it, then opens a
/// connection to the LAN port for every visitor, until closed.
fn run(local: u16, stop: &AtomicBool, outcome: &Outcome) -> io::Result<()> {
    let mut control = connect()?;
    send(&mut control, "{\"Hello\":0}")?;
    control.set_read_timeout(Some(WAKE))?;
    let mut reader = BufReader::new(control);
    // A frame is read across timeouts: what arrived so far stays in `frame`.
    let mut frame = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        match reader.read_until(0, &mut frame) {
            Ok(0) => return Err(io::Error::other("the tunnel server closed the connection")),
            Ok(_) if frame.last() == Some(&0) => {
                frame.pop();
                let message = String::from_utf8_lossy(&frame).into_owned();
                frame.clear();
                if let Some(port) = parse_hello(&message) {
                    if let Ok(mut slot) = outcome.lock() {
                        *slot = Some(Ok(port));
                    }
                } else if let Some(id) = parse_connection(&message) {
                    std::thread::spawn(move || {
                        let _ = serve(&id, local);
                    });
                } else if let Some(why) = parse_error(&message) {
                    return Err(io::Error::other(why));
                }
                // "Heartbeat" and anything else: nothing to do.
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// One visitor: a new connection to the server says which visitor it carries,
/// and from then on it and the LAN port pass each other's bytes.
fn serve(id: &str, local: u16) -> io::Result<()> {
    let mut remote = connect()?;
    send(&mut remote, &format!("{{\"Accept\":\"{id}\"}}"))?;
    let lan = TcpStream::connect(("127.0.0.1", local))?;
    let _ = lan.set_nodelay(true);
    let (mut remote_in, mut lan_out) = (remote.try_clone()?, lan.try_clone()?);
    let up = std::thread::spawn(move || {
        let _ = io::copy(&mut remote_in, &mut lan_out);
        let _ = lan_out.shutdown(Shutdown::Both);
    });
    let (mut lan_in, mut remote_out) = (lan, remote);
    let _ = io::copy(&mut lan_in, &mut remote_out);
    let _ = remote_out.shutdown(Shutdown::Both);
    let _ = up.join();
    Ok(())
}

/// `{"Hello":<port>}`: the port the server gave. (`{"Hello":0}` is our own ask.)
pub fn parse_hello(message: &str) -> Option<u16> {
    let rest = message.strip_prefix("{\"Hello\":")?.strip_suffix('}')?;
    rest.trim().parse::<u16>().ok().filter(|&port| port != 0)
}

/// `{"Connection":"<id>"}`: a visitor is waiting. The id is checked, as it goes
/// back into a message: a UUID has nothing but hex digits and dashes.
pub fn parse_connection(message: &str) -> Option<String> {
    let id = message
        .strip_prefix("{\"Connection\":\"")?
        .strip_suffix("\"}")?;
    let fine = !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    fine.then(|| id.to_owned())
}

/// `{"Error":"<why>"}`: the server refused.
pub fn parse_error(message: &str) -> Option<String> {
    let why = message.strip_prefix("{\"Error\":\"")?.strip_suffix("\"}")?;
    Some(why.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_gives_the_port() {
        assert_eq!(parse_hello("{\"Hello\":41234}"), Some(41234));
        assert_eq!(parse_hello("{\"Hello\":0}"), None);
        assert_eq!(parse_hello("\"Heartbeat\""), None);
    }

    #[test]
    fn a_visitor_is_a_uuid() {
        let id = "123e4567-e89b-12d3-a456-426614174000";
        assert_eq!(parse_connection(&format!("{{\"Connection\":\"{id}\"}}")), Some(id.into()));
        assert_eq!(parse_connection("{\"Connection\":\"a\\\"b\"}"), None);
        assert_eq!(parse_connection("{\"Connection\":\"\"}"), None);
    }

    #[test]
    fn an_error_says_why() {
        assert_eq!(
            parse_error("{\"Error\":\"server requires secret\"}").as_deref(),
            Some("server requires secret")
        );
        assert_eq!(parse_error("\"Heartbeat\""), None);
    }
}
