//! Lebendes Chat-Modell im Speicher finden (nur lesen). Jedes Feld wird geprüft; es muss genau eines geben.
use crate::Reader;
use std::collections::HashSet;
use std::ops::Range;

/// Modell und der lebende Controller, in dem es steht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub controller: u64,
    pub table: u64,
    pub offset: u64,
    pub model: u64,
}

impl Found {
    /// Gilt die Fundstelle noch?
    pub fn alive(&self, r: &Reader, image: &Range<u64>) -> bool {
        word(r, self.controller) == Some(self.table)
            && word(r, self.controller + self.offset) == Some(self.model)
            && valid(r, self.model, image)
    }
}

/// `tables`: absolute Adressen der Controller-Tabellen, `image`: Adressbereich des Spiels.
pub fn find(r: &Reader, tables: &HashSet<u64>, offset: usize, image: &Range<u64>) -> Option<Found> {
    let found: Vec<Found> = r.scan(&|at, buf| {
        let mut out = vec![];
        for (i, w) in buf.as_chunks::<8>().0.iter().enumerate() {
            let table = u64::from_le_bytes(*w);
            if tables.contains(&table)
                && let Some(m) = word(r, (at + i * 8 + offset) as u64)
                && valid(r, m, image)
            {
                out.push(Found { controller: (at + i * 8) as u64, table, offset: offset as u64, model: m });
            }
        }
        out
    });
    // Lieber nichts als das Falsche.
    let first = *found.first()?;
    found.iter().all(|f| f.model == first.model).then_some(first)
}

/// Form des Modells, wie sie der Sende-Code voraussetzt.
pub fn valid(r: &Reader, model: u64, image: &Range<u64>) -> bool {
    let is = |p: Option<u64>| p.is_some_and(|v| image.contains(&v));
    model > 0x10000
        && model.is_multiple_of(8)
        && is(word(r, model))
        && word(r, model + 0x48).and_then(|c| word(r, c)).is_some_and(|v| v & 0xff == 1)
        && word(r, model + 0x58).is_some_and(|c| is(word(r, c)))
}

fn word(r: &Reader, at: u64) -> Option<u64> {
    let mut b = [0u8; 8];
    r.read(at as usize, &mut b).ok()?;
    Some(u64::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modell_wird_streng_geprueft() {
        let r = Reader::open(std::process::id()).unwrap();
        let control = [1u64];
        let client = [0x5000_0000u64];
        let mut model = [0u64; 12];
        model[0] = 0x5000_0100;
        model[9] = control.as_ptr() as u64;
        model[11] = client.as_ptr() as u64;
        let image = 0x5000_0000..0x6000_0000;
        let at = std::hint::black_box(&model).as_ptr() as u64;
        assert!(valid(&r, at, &image));
        assert!(!valid(&r, at + 4, &image));
        assert!(!valid(&r, at, &(0..1)));
        assert!(!valid(&r, 0, &image));
        let dead = [0u64];
        model[9] = dead.as_ptr() as u64;
        assert!(!valid(&r, std::hint::black_box(&model).as_ptr() as u64, &image));
    }
}
