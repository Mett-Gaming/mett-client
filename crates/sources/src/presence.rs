//! Meldet dem Mett Launcher den Status für Discord. Nur, wenn er die Verbindung anbietet.
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Die Verbindung bleibt offen, solange der Status gelten soll.
pub struct Presence {
    _stream: TcpStream,
}

impl Presence {
    pub fn connect(details: &str) -> Option<Self> {
        let addr = std::env::var("METT_PRESENCE_ADDR").ok()?;
        let token = std::env::var("METT_PRESENCE_TOKEN").ok()?;
        Self::open(&addr, &token, details)
    }

    fn open(addr: &str, token: &str, details: &str) -> Option<Self> {
        let addr: SocketAddr = addr.parse().ok()?;
        // Das Token darf den Rechner nie verlassen.
        if !addr.ip().is_loopback() {
            return None;
        }
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
        stream.set_write_timeout(Some(Duration::from_millis(500))).ok()?;
        let line = serde_json::json!({ "token": token, "details": details }).to_string() + "\n";
        stream.write_all(line.as_bytes()).ok()?;
        Some(Self { _stream: stream })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    #[test]
    fn sendet_eine_zeile_mit_token() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let p = Presence::open(&l.local_addr().unwrap().to_string(), "abc", "Test \"x\"");
        let mut line = String::new();
        BufReader::new(l.accept().unwrap().0).read_line(&mut line).unwrap();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!((v["token"].as_str(), v["details"].as_str()), (Some("abc"), Some("Test \"x\"")));
        assert!(p.is_some());
    }

    #[test]
    fn fremde_adressen_werden_abgelehnt() {
        assert!(Presence::open("8.8.8.8:80", "abc", "x").is_none());
        assert!(Presence::open("unsinn", "abc", "x").is_none());
    }
}
