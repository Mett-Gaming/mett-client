//! Schrifttafeln von Servern für den Chat. Gelesen wird nur das Bild, das Minecraft zum Anzeigen schon ausgepackt hat,
//! nur im Arbeitsspeicher: nichts wird entschlüsselt, gespeichert oder weitergegeben.
use gamemem::{Reader, sheets};
use hud::symbols::{Sheet, Symbols};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Nach dem Beitreten lädt der Server noch Packs nach: erst oft, dann selten suchen. Gebraucht werden die Tafeln nur in
/// der Chat-Eingabe, deshalb wird beim Öffnen nachgesehen.
const FIRST: Duration = Duration::from_millis(700);
const CALM: Duration = Duration::from_secs(15);
const LAST: Duration = Duration::from_secs(180);

/// Rot, Grün, Blau, Deckkraft → Blau, Grün, Rot mit verrechneter Deckkraft.
fn convert(rgba: &[u8]) -> Vec<u8> {
    let mul = |c: u8, a: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
    rgba.as_chunks::<4>().0.iter().flat_map(|p| [mul(p[2], p[3]), mul(p[1], p[3]), mul(p[0], p[3]), p[3]]).collect()
}

fn translate(raw: &[sheets::Sheet]) -> Symbols {
    let sheet = |s: &sheets::Sheet| Some((s.id, Sheet::new(s.id, s.side, convert(&s.pixels))?));
    Symbols(raw.iter().filter_map(sheet).collect())
}

pub struct Source {
    reader: Reader,
    pid: u32,
    inside: bool,
    next: Instant,
    wait: Duration,
    /// Wartezeit, die gerade läuft.
    gap: Duration,
    raw: Vec<sheets::Sheet>,
    found: Arc<Symbols>,
}

impl Source {
    pub fn open(pid: u32) -> Option<Self> {
        let reader = Reader::open_for_scan(pid).ok()?;
        let next = Instant::now();
        Some(Self {
            reader,
            pid,
            inside: false,
            next,
            wait: FIRST,
            gap: Duration::ZERO,
            raw: vec![],
            found: Arc::default(),
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// `inside`: eine Welt oder ein Server ist offen. Draußen werden die Tafeln verworfen.
    pub fn poll(&mut self, inside: bool, opened: bool) -> Arc<Symbols> {
        if inside != self.inside {
            self.inside = inside;
            self.next = Instant::now();
            self.wait = FIRST;
            if !inside {
                self.raw.clear();
                self.found = Arc::default();
            }
        }
        // Im ruhigen Takt liegt die letzte Suche bis zu `LAST` zurück.
        let stale = self.next.checked_sub(self.gap).is_some_and(|t| t.elapsed() >= CALM);
        if inside && (Instant::now() >= self.next || (opened && stale)) {
            let raw = sheets::find(&self.reader);
            if raw != self.raw {
                self.found = Arc::new(translate(&raw));
                self.raw = raw;
                // Es ändert sich noch etwas.
                self.wait = FIRST;
            }
            (self.next, self.gap) = (Instant::now() + self.wait, self.wait);
            self.wait = (self.wait * 3 / 2).min(LAST);
        }
        self.found.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn farben_und_deckkraft() {
        assert_eq!(convert(&[255, 128, 0, 255, 200, 100, 50, 0]), [0, 128, 255, 255, 0, 0, 0, 0]);
        assert_eq!(convert(&[255, 0, 100, 128]), [50, 0, 128, 128]);
    }

    #[test]
    fn nur_gueltige_tafeln() {
        let good = sheets::Sheet { id: 0xE4, side: 16, pixels: vec![255; 1024] };
        let bad = sheets::Sheet { id: 0xE5, side: 17, pixels: vec![255; 17 * 17 * 4] };
        let s = translate(&[good, bad]);
        assert_eq!(s.0.keys().collect::<Vec<_>>(), [&0xE4]);
        assert_eq!(s.advance('\u{E4DF}'), Some(9.0));
    }
}
