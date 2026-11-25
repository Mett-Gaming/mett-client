//! ICMP-Ping zum eingetragenen Server. Geht nur dorthin, nur solange das Modul angezeigt wird.
use std::net::{SocketAddr, ToSocketAddrs};
use windows::Win32::NetworkManagement::IpHelper::*;

/// Millisekunden, `None` ohne Antwort.
pub fn measure(host: &str) -> Option<u32> {
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
    fn eigener_rechner_antwortet() {
        assert!(measure("127.0.0.1").is_some_and(|ms| ms < 100));
        assert_eq!(measure(""), None);
    }
}
