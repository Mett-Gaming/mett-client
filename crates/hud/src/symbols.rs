//! Schrifttafeln von Servern: je Tafel 16 × 16 Zeichen, eine Seite des Zeichensatzes (`U+E400` bis `U+E4FF` liegen auf
//! `glyph_E4`). Meist Symbole in den selbst belegten Zeichen, aber jede Seite ist möglich. Nur Rechnen, kein Windows.
use std::collections::BTreeMap;
use std::sync::Arc;

pub const MAX_SIDE: u32 = 512;
/// So hoch ist ein Symbol in Java-Pixeln.
pub const HEIGHT: f32 = 8.0;

/// Fertig entpackte Tafel, quadratisch, vier Byte je Punkt (Blau, Grün, Rot, Deckkraft; schon verrechnet).
#[derive(Clone, PartialEq)]
pub struct Sheet {
    pub id: u8,
    pub side: u32,
    pub pixels: Arc<Vec<u8>>,
    /// Breite jedes Symbols in Java-Pixeln, 0 heißt leer.
    widths: Arc<Vec<f32>>,
}

impl std::fmt::Debug for Sheet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tafel {:X} {}", self.id, self.side)
    }
}

impl Sheet {
    /// `None`, wenn die Maße nicht zu einer Tafel passen.
    pub fn new(id: u8, side: u32, pixels: Vec<u8>) -> Option<Self> {
        let fits = (16..=MAX_SIDE).contains(&side) && side.is_multiple_of(16);
        if !fits || pixels.len() != (side * side * 4) as usize {
            return None;
        }
        let cell = (side / 16) as usize;
        let widths = (0..256)
            .map(|i| {
                let (col, row) = (i % 16, i / 16);
                let solid = |x: usize| {
                    (0..cell).any(|y| pixels[((row * cell + y) * side as usize + col * cell + x) * 4 + 3] > 8)
                };
                let right = (0..cell).rev().find(|x| solid(*x));
                right.map_or(0.0, |r| (r + 1) as f32 * HEIGHT / cell as f32)
            })
            .collect();
        Some(Self { id, side, pixels: Arc::new(pixels), widths: Arc::new(widths) })
    }

    /// Ausschnitt eines Symbols in Bildpunkten: links, oben, Breite, Höhe. `None`, wenn die Zelle leer ist.
    pub fn cell(&self, index: u8) -> Option<(f32, f32, f32, f32)> {
        let width = self.widths[index as usize];
        let cell = (self.side / 16) as f32;
        let (col, row) = ((index % 16) as f32, (index / 16) as f32);
        (width > 0.0).then_some((col * cell, row * cell, width * cell / HEIGHT, cell))
    }

    /// Platz bis zum nächsten Zeichen in Java-Pixeln.
    pub fn advance(&self, index: u8) -> Option<f32> {
        let w = self.widths[index as usize];
        (w > 0.0).then_some(w + 1.0)
    }
}

/// Für Server freigehalten; dafür gibt es in keiner Schrift ein Bild.
pub fn private(c: char) -> bool {
    matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{10FFFF}')
}

/// Tafel und Stelle eines Zeichens.
pub fn place(c: char) -> Option<(u8, u8)> {
    let n = c as u32;
    (n <= 0xFFFF).then_some(((n >> 8) as u8, n as u8))
}

/// Alle bekannten Tafeln.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Symbols(pub BTreeMap<u8, Sheet>);

impl Symbols {
    pub fn find(&self, c: char) -> Option<(&Sheet, u8)> {
        let (sheet, index) = place(c)?;
        self.0.get(&sheet).map(|s| (s, index))
    }

    pub fn advance(&self, c: char) -> Option<f32> {
        self.find(c).and_then(|(s, i)| s.advance(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tafel mit 32 Punkten Kante: Zelle 0 ist ganz gefüllt, Zelle 0xDF nur links.
    fn sheet() -> Sheet {
        let mut p = vec![0u8; 32 * 32 * 4];
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1), (30, 26)] {
            p[(y * 32 + x) * 4 + 3] = 255;
        }
        Sheet::new(0xE4, 32, p).unwrap()
    }

    #[test]
    fn zellen_und_breiten() {
        let s = sheet();
        assert_eq!(s.cell(0), Some((0.0, 0.0, 2.0, 2.0)));
        assert_eq!(s.advance(0), Some(9.0));
        // Das Zeichen aus dem Spiel: U+E4DF, Zeile 13, Spalte 15.
        assert_eq!(s.cell(0xDF), Some((30.0, 26.0, 1.0, 2.0)));
        assert_eq!(s.advance(0xDF), Some(5.0));
        assert_eq!((s.cell(1), s.advance(1)), (None, None));
        let all = Symbols([(0xE4, s)].into());
        assert_eq!(all.advance('\u{E4DF}'), Some(5.0));
        assert_eq!(all.advance('\u{E5DF}'), None);
        assert_eq!(all.advance('a'), None);
        // Andere Seiten genauso: U+2A00 liegt auf Tafel 2A.
        assert_eq!(place('\u{2A00}'), Some((0x2A, 0)));
        assert_eq!(place('😀'), None);
        assert!(private('\u{E4DF}') && !private('\u{2A00}') && !private('a'));
    }

    #[test]
    fn falsche_masse_gelten_nicht() {
        assert!(Sheet::new(0xE4, 32, vec![0; 10]).is_none());
        assert!(Sheet::new(0xE4, 17, vec![0; 17 * 17 * 4]).is_none());
        assert!(Sheet::new(0xE4, 1024, vec![0; 1024 * 1024 * 4]).is_none());
        assert!(Sheet::new(0xE4, 0, vec![]).is_none());
    }
}
