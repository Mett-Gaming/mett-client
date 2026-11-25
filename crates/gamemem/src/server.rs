//! Mit welchem Server das Spiel verbunden ist (nur lesen). Einstieg ist der Klassenname der Verbindung
//! (`webrtc::ProxyConnection`). Gelesen wird nur die Adresse der Gegenstelle, nur im Arbeitsspeicher.
use crate::Reader;
use std::net::Ipv4Addr;

/// Vollständiger Klassenname.
pub const CLASS: &str = ".?AVProxyConnection@webrtc@@";
/// So viel einer Verbindung wird durchgesehen.
const SPAN: usize = 0x600;
/// Mehr Verbindungen zugleich sind unplausibel.
const MOST: usize = 64;

/// Alle Verbindungen. Durchsucht den ganzen Speicher, also selten aufrufen.
pub fn find(r: &Reader, tables: &[u64]) -> Vec<u64> {
    let mut found = r.scan(&|at, b| {
        let words = b.as_chunks::<8>().0.iter().enumerate();
        words.filter(|(_, w)| tables.contains(&u64::from_le_bytes(**w))).map(|(i, _)| (at + i * 8) as u64).collect()
    });
    found.sort_unstable();
    found.truncate(MOST);
    found
}

/// Die eine Gegenstelle aller noch bestehenden Verbindungen.
pub fn address(r: &Reader, tables: &[u64], objects: &[u64]) -> Option<Ipv4Addr> {
    let mut all = vec![];
    for at in objects {
        let mut b = vec![0u8; SPAN];
        let alive = r.read(*at as usize, &mut b).is_ok()
            && tables.contains(&u64::from_le_bytes(b[..8].try_into().unwrap_or_default()));
        if alive {
            all.extend(one(&b)?);
        }
    }
    all.dedup();
    if let [ip] = all[..] { Some(ip) } else { None }
}

/// Öffentliche Adressen im Text; `None`, wenn es verschiedene sind.
fn one(b: &[u8]) -> Option<Option<Ipv4Addr>> {
    let mut found: Option<Ipv4Addr> = None;
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let n = b[i..].iter().take_while(|c| c.is_ascii_digit() || **c == b'.').count();
        // Nur ein abgeschlossener Text zählt.
        let clean = (i == 0 || !b[i - 1].is_ascii_alphanumeric()) && b.get(i + n) == Some(&0);
        let ip = std::str::from_utf8(&b[i..i + n]).ok().and_then(|s| s.parse::<Ipv4Addr>().ok());
        if let Some(ip) = ip.filter(|ip| clean && public(ip)) {
            if found.is_some_and(|f| f != ip) {
                return None;
            }
            found = Some(ip);
        }
        i += n;
    }
    Some(found)
}

fn public(ip: &Ipv4Addr) -> bool {
    !(ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_multicast())
        && !ip.is_broadcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(texts: &[&str]) -> Vec<u8> {
        let mut b = vec![0u8; SPAN];
        for (i, t) in texts.iter().enumerate() {
            b[0x100 + i * 0x40..][..t.len()].copy_from_slice(t.as_bytes());
        }
        b
    }

    #[test]
    fn genau_eine_oeffentliche_adresse() {
        let ip = Ipv4Addr::new(203, 0, 113, 7);
        assert_eq!(one(&block(&["203.0.113.7"])), Some(Some(ip)));
        assert_eq!(one(&block(&["203.0.113.7", "203.0.113.7", "192.168.1.4", "127.0.0.1"])), Some(Some(ip)));
        // Zwei verschiedene: lieber nichts.
        assert_eq!(one(&block(&["203.0.113.7", "203.0.113.8"])), None);
        assert_eq!(one(&block(&["192.168.1.4"])), Some(None));
    }

    #[test]
    fn nur_vollstaendige_texte_zaehlen() {
        for t in ["203.0.113.7x", "v203.0.113.7", "203.0.113", "203.0.113.7.1", "999.0.1.2", "203.0.113.7:19132"] {
            assert_eq!(one(&block(&[t])), Some(None), "{t}");
        }
        let mut b = vec![0u8; 16];
        b[5..].copy_from_slice(b"203.0.113.7");
        assert_eq!(one(&b), Some(None));
    }
}
