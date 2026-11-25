//! Ping zum verbundenen Server: ICMP, sonst die übliche Serverabfrage von Minecraft (RakNet). Geht nur dorthin, nur solange das Modul angezeigt wird.
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};
use windows::Win32::NetworkManagement::IpHelper::*;

/// Port der Serverabfrage.
const PORT: u16 = 19132;
/// Feste Kennung in jeder Abfrage.
const MAGIC: [u8; 16] = [0, 255, 255, 0, 254, 254, 254, 254, 253, 253, 253, 253, 18, 52, 86, 120];

/// Millisekunden, `None` ohne Antwort.
pub fn measure(host: &str) -> Option<u32> {
    echo(host).or_else(|| query(host_name(host)?, PORT))
}

/// Serverabfrage über RakNet.
fn query(host: &str, port: u16) -> Option<u32> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect((host, port)).ok()?;
    socket.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    let mut m = vec![1u8];
    m.extend([0; 8]);
    m.extend(MAGIC);
    m.extend([0; 8]);
    let start = Instant::now();
    socket.send(&m).ok()?;
    let mut b = [0u8; 1500];
    let n = socket.recv(&mut b).ok()?;
    // Antwort: Kennung 0x1C, Zeit, Server, feste Kennung.
    (n >= 33 && b[0] == 0x1C && b[17..33] == MAGIC).then(|| start.elapsed().as_millis() as u32)
}

fn echo(host: &str) -> Option<u32> {
    let name = host_name(host)?;
    let ip = (name, 0u16).to_socket_addrs().ok()?.find_map(|a| match a {
        SocketAddr::V4(v) => Some(*v.ip()),
        SocketAddr::V6(_) => None,
    })?;
    unsafe {
        let handle = IcmpCreateFile().ok()?;
        let data = [0u8; 8];
        let mut reply = [0u64; 32];
        let size = size_of_val(&reply) as u32;
        let n = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(ip.octets()),
            data.as_ptr().cast(),
            data.len() as u16,
            None,
            reply.as_mut_ptr().cast(),
            size,
            1500,
        );
        let r = &*reply.as_ptr().cast::<ICMP_ECHO_REPLY>();
        let ms = (n > 0 && r.Status == 0).then_some(r.RoundTripTime);
        let _ = IcmpCloseHandle(handle);
        ms
    }
}

/// Entfernt einen Port wie in `server.de:19132`.
fn host_name(host: &str) -> Option<&str> {
    let h = host.trim();
    let name = h.split_once(':').map_or(h, |(n, _)| n);
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_wird_entfernt() {
        assert_eq!(host_name(" server.de:19132 "), Some("server.de"));
        assert_eq!(host_name("1.2.3.4"), Some("1.2.3.4"));
        assert_eq!(host_name("  "), None);
        assert_eq!(host_name(":1"), None);
    }

    #[test]
    fn serverabfrage_prueft_die_antwort() {
        for good in [true, false] {
            let server = UdpSocket::bind("127.0.0.1:0").unwrap();
            let port = server.local_addr().unwrap().port();
            let t = std::thread::spawn(move || {
                let mut b = [0u8; 100];
                let (n, from) = server.recv_from(&mut b).unwrap();
                assert!(n == 33 && b[0] == 1 && b[9..25] == MAGIC);
                let mut a = vec![0x1C];
                a.extend([7; 16]);
                a.extend(if good { MAGIC } else { [0; 16] });
                server.send_to(&a, from).unwrap();
            });
            assert_eq!(query("127.0.0.1", port).is_some(), good);
            t.join().unwrap();
        }
    }

    #[test]
    fn eigener_rechner_antwortet() {
        assert!(measure("127.0.0.1").is_some_and(|ms| ms < 100));
        assert_eq!(measure(""), None);
    }
}
