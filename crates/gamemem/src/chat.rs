//! Chat-Nachrichten im Speicher erkennen. Aufbau nach öffentlichen Headern nachgebaut, nicht kopiert.
//! Jede Nachricht wird streng geprüft; passt etwas nicht, gilt sie als keine Nachricht.

/// Rechnerische Größe einer Nachricht.
pub const SIZE: usize = 272;
pub const TEXT_AT: usize = 8;
const FILTERED_AT: usize = 40;
const TTS_AT: usize = 80;
const AUTHOR_AT: usize = 112;
const FULL_AT: usize = 144;
const DURATION_AT: usize = 252;
const MAX_TEXT: usize = 4096;

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub kind: u32,
    pub author: String,
    pub text: String,
    /// So, wie das Spiel sie zeigt, mit Absender.
    pub full: String,
}

impl Message {
    pub fn shown(&self) -> &str {
        if self.full.is_empty() { &self.text } else { &self.full }
    }
}

/// `read` füllt den Puffer ab der Adresse und meldet, ob das geklappt hat.
pub fn parse(at: usize, read: &dyn Fn(usize, &mut [u8]) -> bool) -> Option<Message> {
    let mut b = [0u8; SIZE];
    if !read(at, &mut b) {
        return None;
    }
    let kind = u32::from_le_bytes(b[0..4].try_into().ok()?);
    let duration = f32::from_le_bytes(b[DURATION_AT..DURATION_AT + 4].try_into().ok()?);
    // 1 bis 9 sind die bekannten Arten; die Kennung des optionalen Texts ist 0 oder 1.
    // Die Anzeigedauer zählt nach dem Ausblenden ins Negative weiter (im Spiel gesehen).
    if !(1..=9).contains(&kind) || b[FILTERED_AT + 32] > 1 || !duration.is_finite() || duration.abs() > 1e6 {
        return None;
    }
    string(&b[TTS_AT..], read)?;
    Some(Message {
        kind,
        author: string(&b[AUTHOR_AT..], read)?,
        text: string(&b[TEXT_AT..], read)?,
        full: string(&b[FULL_AT..], read)?,
    })
}

/// Mehr Nachrichten hält das Spiel nicht vor; größere Listen sind etwas anderes.
pub const MAX_COUNT: usize = 1000;

/// Lage einer Liste: (erste Nachricht, Anzahl). `None`, wenn die drei Zeiger keine Liste ergeben.
pub fn bounds(b: &[u8]) -> Option<(usize, usize)> {
    let n = |i: usize| Some(usize::from_le_bytes(b.get(i * 8..i * 8 + 8)?.try_into().ok()?));
    let (from, to, cap) = (n(0)?, n(1)?, n(2)?);
    let (used, room) = (to.checked_sub(from)?, cap.checked_sub(from)?);
    let fits = from % 8 == 0 && used <= room && used % SIZE == 0 && room % SIZE == 0 && room <= MAX_COUNT * 4 * SIZE;
    (fits && from > 0x10000 && used / SIZE <= MAX_COUNT).then_some((from, used / SIZE))
}

/// Alle Nachrichten einer Liste; nur wenn jede einzelne gültig ist.
pub fn messages(list: usize, read: &dyn Fn(usize, &mut [u8]) -> bool) -> Option<Vec<Message>> {
    let mut b = [0u8; 24];
    if !read(list, &mut b) {
        return None;
    }
    let (from, count) = bounds(&b)?;
    (0..count).map(|k| parse(from + k * SIZE, read)).collect()
}

/// Sucht in einem Speicherstück nach gefüllten Nachrichtenlisten.
pub fn lists_in(start: usize, buf: &[u8], read: &dyn Fn(usize, &mut [u8]) -> bool) -> Vec<usize> {
    let mut found = vec![];
    for i in (0..buf.len().saturating_sub(23)).step_by(8) {
        if let Some((from, count)) = bounds(&buf[i..i + 24])
            && count > 0
            && parse(from, read).is_some()
            && messages(start + i, read).is_some()
        {
            found.push(start + i);
        }
    }
    found
}

/// Zeichenkette im Aufbau von MSVC: 16 Byte Text oder Zeiger, Länge, Kapazität.
fn string(b: &[u8], read: &dyn Fn(usize, &mut [u8]) -> bool) -> Option<String> {
    let num = |i: usize| Some(usize::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?));
    let (len, cap) = (num(16)?, num(24)?);
    if len > cap || cap < 15 || len > MAX_TEXT || cap > MAX_TEXT * 16 {
        return None;
    }
    let mut text = vec![0; len];
    if cap == 15 {
        text.copy_from_slice(&b[..len]);
    } else if !read(num(0)?, &mut text) {
        return None;
    }
    String::from_utf8(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Speicher mit einer Nachricht ab 1000 und langem Text ab 5000.
    fn memory(text: &str) -> Vec<u8> {
        let mut m = vec![0u8; 10_000];
        let mut put = |at: usize, b: &[u8]| m[at..at + b.len()].copy_from_slice(b);
        put(1000, &1u32.to_le_bytes());
        for at in [TEXT_AT, TTS_AT, AUTHOR_AT, FULL_AT] {
            put(1000 + at + 24, &15usize.to_le_bytes());
        }
        put(1000 + AUTHOR_AT, b"Mett");
        put(1000 + AUTHOR_AT + 16, &4usize.to_le_bytes());
        put(1000 + DURATION_AT, &10f32.to_le_bytes());
        put(1000 + TEXT_AT + 16, &text.len().to_le_bytes());
        if text.len() <= 15 {
            put(1000 + TEXT_AT, text.as_bytes());
        } else {
            put(1000 + TEXT_AT, &5000usize.to_le_bytes());
            put(1000 + TEXT_AT + 24, &31usize.max(text.len()).to_le_bytes());
            put(5000, text.as_bytes());
        }
        m
    }

    fn run(m: &[u8]) -> Option<Message> {
        parse(1000, &|at, out| match m.get(at..at + out.len()) {
            Some(s) => {
                out.copy_from_slice(s);
                true
            }
            None => false,
        })
    }

    #[test]
    fn kurze_und_lange_texte() {
        for t in ["Hallo", "Ein deutlich längerer Text mit Umlauten: äöü"] {
            let want = Message { kind: 1, author: "Mett".into(), text: t.into(), full: String::new() };
            assert_eq!(run(&memory(t)), Some(want));
        }
    }

    #[test]
    fn abgelaufene_nachricht_zaehlt() {
        let mut m = memory("Hallo");
        m[1000 + DURATION_AT..1000 + DURATION_AT + 4].copy_from_slice(&(-55.3f32).to_le_bytes());
        assert!(run(&m).is_some());
    }

    #[test]
    fn liste_wird_gefunden_und_gelesen() {
        let mut m = memory("Hallo");
        for (k, v) in [1000usize, 1000 + SIZE, 1000 + 2 * SIZE].into_iter().enumerate() {
            m[8000 + k * 8..8008 + k * 8].copy_from_slice(&(v + 0x100000).to_le_bytes());
        }
        // Adressen beginnen hier bei 0x100000.
        let read = |at: usize, out: &mut [u8]| match at.checked_sub(0x100000).and_then(|a| m.get(a..a + out.len())) {
            Some(s) => {
                out.copy_from_slice(s);
                true
            }
            None => false,
        };
        assert_eq!(lists_in(0x100000, &m, &read), [0x100000 + 8000]);
        assert_eq!(messages(0x100000 + 8000, &read).unwrap()[0].text, "Hallo");
        assert!(messages(0x100000 + 9000, &read).is_none());
    }

    #[test]
    fn listen_mit_krummer_groesse_zaehlen_nicht() {
        let b = |v: [usize; 3]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let a = 0x20000;
        assert_eq!(bounds(&b([a, a + SIZE, a + SIZE])), Some((a, 1)));
        for bad in [[a, a + 100, a + SIZE], [a + SIZE, a, a], [a, a + 2 * SIZE, a + SIZE], [0, 0, 0]] {
            assert_eq!(bounds(&b(bad)), None);
        }
    }

    #[test]
    fn unsinn_wird_abgelehnt() {
        let cases: [(usize, &[u8]); 6] = [
            (0, &77u32.to_le_bytes()),
            (FILTERED_AT + 32, &[9]),
            (TEXT_AT + 16, &99usize.to_le_bytes()),
            (TEXT_AT + 24, &3usize.to_le_bytes()),
            (DURATION_AT, &f32::NAN.to_le_bytes()),
            (AUTHOR_AT, &[0xff, 0xfe]),
        ];
        for (at, b) in cases {
            let mut m = memory("Hallo");
            m[1000 + at..1000 + at + b.len()].copy_from_slice(b);
            assert_eq!(run(&m), None, "Stelle {at}");
        }
        assert_eq!(run(&[0; 10]), None);
    }
}
