//! Symboltafeln (`font/glyph_XX`), die das Spiel zum Anzeigen ausgepackt im Arbeitsspeicher hält (nur lesen). Einstieg ist
//! ihr Name im Verzeichnis des Spiels; jede Stufe prüft die Form. Es wird nichts entschlüsselt.
use crate::Reader;
use std::collections::BTreeMap;

const PREFIX: &[u8; 11] = b"font/glyph_";
/// Name mit zwei Stellen: 13 Zeichen, kurz genug, um direkt im Eintrag zu stehen.
const LEN: usize = 13;
// Eintrag: Name (32 Byte), Kennzahl (16 Byte), Zeiger auf das Bild.
const IMAGE: usize = 0x30;
// Bild: Aufräumfunktion, Zeiger auf die Punkte, Größe, Breite, Höhe, Format.
const PIXELS: usize = 0x08;
const SIZE: usize = 0x10;
const WIDTH: usize = 0x18;
const HEIGHT: usize = 0x1C;
const FORMAT: usize = 0x20;
const RGBA: u32 = 28;
const MAX_SIDE: u32 = 512;

#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub id: u8,
    pub side: u32,
    /// Rot, Grün, Blau, Deckkraft.
    pub pixels: Vec<u8>,
}

type Read<'a> = &'a (dyn Fn(usize, &mut [u8]) -> bool + Sync);

/// Kurzer Name an dieser Stelle: Nummer der Tafel, für alle 256 Seiten des Zeichensatzes.
fn name(b: &[u8]) -> Option<u8> {
    let b = b.get(..32)?;
    let n = |o: usize| usize::from_le_bytes(b[o..o + 8].try_into().unwrap_or_default());
    if !b.starts_with(PREFIX) || b[LEN..16] != [0; 3] || n(16) != LEN || n(24) != 15 {
        return None;
    }
    let hex = std::str::from_utf8(&b[11..LEN]).ok().filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit()))?;
    u8::from_str_radix(hex, 16).ok()
}

/// Bild zu einem Eintrag, wenn alles zusammenpasst.
fn image(read: Read, id: u8, entry: usize) -> Option<Sheet> {
    let word = |at: usize| {
        let mut b = [0u8; 8];
        read(at, &mut b).then(|| usize::from_le_bytes(b))
    };
    let object = word(entry + IMAGE).filter(|o| *o > 0x10000 && o.is_multiple_of(8))?;
    let mut b = [0u8; 0x28];
    if !read(object, &mut b) {
        return None;
    }
    let long = |o: usize| usize::from_le_bytes(b[o..o + 8].try_into().unwrap_or_default());
    let short = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap_or_default());
    let (at, size, w, h) = (long(PIXELS), long(SIZE), short(WIDTH), short(HEIGHT));
    let fits = w == h && (16..=MAX_SIDE).contains(&w) && w.is_multiple_of(16) && short(FORMAT) == RGBA;
    if !fits || size != (w * h * 4) as usize || at <= 0x10000 {
        return None;
    }
    let mut pixels = vec![0u8; size];
    read(at, &mut pixels).then_some(Sheet { id, side: w, pixels })
}

/// Alle Tafeln. Nennt das Spiel für eine Tafel verschiedene Bilder, gilt keins davon.
pub fn find(r: &Reader) -> Vec<Sheet> {
    let read: Read = &|at, out| r.read(at, out).is_ok();
    let found = r.scan(&|start, buf| {
        let spots = (0..buf.len().saturating_sub(31)).step_by(8);
        spots.filter_map(|o| name(&buf[o..]).and_then(|id| image(read, id, start + o))).collect()
    });
    unique(found)
}

fn unique(found: Vec<Sheet>) -> Vec<Sheet> {
    let mut by_id: BTreeMap<u8, Option<Sheet>> = BTreeMap::new();
    for s in found {
        by_id.entry(s.id).and_modify(|old| *old = old.take().filter(|o| *o == s)).or_insert(Some(s));
    }
    by_id.into_values().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: usize = 0x20000;

    fn entry(text: &str) -> Vec<u8> {
        let mut b = vec![0u8; 32];
        b[..text.len()].copy_from_slice(text.as_bytes());
        b[16..24].copy_from_slice(&text.len().to_le_bytes());
        b[24..].copy_from_slice(&15usize.to_le_bytes());
        b
    }

    /// Eintrag bei 0, Bild bei 0x100, Punkte bei 0x200 (16 × 16).
    fn game() -> Vec<u8> {
        let mut m = vec![0u8; 0x1000];
        let mut put = |at: usize, b: &[u8]| m[at..at + b.len()].copy_from_slice(b);
        put(0, &entry("font/glyph_E4"));
        put(IMAGE, &(BASE + 0x100).to_le_bytes());
        put(0x100 + PIXELS, &(BASE + 0x200).to_le_bytes());
        put(0x100 + SIZE, &1024usize.to_le_bytes());
        put(0x100 + WIDTH, &16u32.to_le_bytes());
        put(0x100 + HEIGHT, &16u32.to_le_bytes());
        put(0x100 + FORMAT, &RGBA.to_le_bytes());
        put(0x200, &[7; 1024]);
        m
    }

    fn load(m: &[u8]) -> Option<Sheet> {
        let read = |at: usize, out: &mut [u8]| {
            let Some(b) = at.checked_sub(BASE).and_then(|o| m.get(o..o + out.len())) else { return false };
            out.copy_from_slice(b);
            true
        };
        name(m).and_then(|id| image(&read, id, BASE))
    }

    #[test]
    fn folgt_dem_namen_bis_zum_bild() {
        assert_eq!(load(&game()), Some(Sheet { id: 0xE4, side: 16, pixels: vec![7; 1024] }));
    }

    #[test]
    fn jede_abweichung_gilt_nicht() {
        for text in ["font/glyph_+4", "font/glyph_E", "font/glyphsE4", "font/default8", "font/glyph_GG"] {
            assert_eq!(name(&entry(text)), None, "{text}");
        }
        // Zeiger, Größe, Breite, Format, Lage der Punkte.
        let breaks: [(usize, &[u8]); 6] = [
            (IMAGE, &[0; 8]),
            (0x100 + SIZE, &[1, 4]),
            (0x100 + WIDTH, &[17]),
            (0x100 + FORMAT, &[71]),
            (0x100 + PIXELS, &[0; 8]),
            (0x100 + PIXELS, &[0xFF; 8]),
        ];
        for (at, bytes) in breaks {
            let mut m = game();
            m[at..at + bytes.len()].copy_from_slice(bytes);
            assert_eq!(load(&m), None, "{at:x}");
        }
        assert_eq!(name(&game()[..31]), None);
        // Auch Seiten außerhalb der selbst belegten Zeichen.
        assert_eq!((name(&entry("font/glyph_00")), name(&entry("font/glyph_2A"))), (Some(0), Some(0x2A)));
    }

    #[test]
    fn verschiedene_bilder_fuer_eine_tafel_gelten_nicht() {
        let s = |id, v| Sheet { id, side: 16, pixels: vec![v; 4] };
        let all = vec![s(0xE4, 1), s(0xE4, 1), s(0xE1, 1), s(0xE1, 2), s(0xE1, 2)];
        assert_eq!(unique(all), [s(0xE4, 1)]);
    }
}
