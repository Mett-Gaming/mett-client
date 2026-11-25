//! Geladenes Programmabbild durchsuchen: Klassennamen (RTTI), Knopfkennungen, Funktionen. Keine festen Adressen.
//! Alle Adressen sind Abstände zum Anfang des Abbilds.
use std::collections::HashSet;
use std::ops::Range;

pub struct Image<'a> {
    base: u64,
    bytes: &'a [u8],
    pub text: Range<usize>,
    pub rdata: Range<usize>,
    pub data: Range<usize>,
    pdata: Range<usize>,
}

/// Kennung, die Minecraft aus einem Knopfnamen berechnet (FNV-1a, 32 Bit).
pub fn name_id(name: &str) -> u32 {
    name.bytes().fold(0x811C_9DC5, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

impl<'a> Image<'a> {
    /// `bytes` ist das Abbild, so wie es im Speicher liegt. `None`, wenn es keine lesbare 64-Bit-Programmdatei ist.
    pub fn new(base: u64, bytes: &'a [u8]) -> Option<Self> {
        let mut s = Self { base, bytes, text: 0..0, rdata: 0..0, data: 0..0, pdata: 0..0 };
        let pe = s.u32(0x3c)? as usize;
        if bytes.get(pe..pe + 4)? != b"PE\0\0" || s.u16(pe + 24)? != 0x20b {
            return None;
        }
        let table = pe + 24 + s.u16(pe + 20)? as usize;
        for i in 0..s.u16(pe + 6)? as usize {
            let at = table + i * 40;
            let (size, start) = (s.u32(at + 8)? as usize, s.u32(at + 12)? as usize);
            let range = start..start.checked_add(size)?.min(bytes.len());
            match bytes.get(at..at + 8)? {
                b".text\0\0\0" => s.text = range,
                b".rdata\0\0" => s.rdata = range,
                b".data\0\0\0" => s.data = range,
                b".pdata\0\0" => s.pdata = range,
                _ => {}
            }
        }
        (!s.text.is_empty() && !s.rdata.is_empty()).then_some(s)
    }

    pub fn u16(&self, at: usize) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes.get(at..at.checked_add(2)?)?.try_into().ok()?))
    }

    pub fn u32(&self, at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes.get(at..at.checked_add(4)?)?.try_into().ok()?))
    }

    /// Zeiger ins Abbild, als Abstand.
    pub fn pointer(&self, at: usize) -> Option<usize> {
        let v = u64::from_le_bytes(self.bytes.get(at..at.checked_add(8)?)?.try_into().ok()?);
        let rva = usize::try_from(v.checked_sub(self.base)?).ok()?;
        (rva < self.bytes.len()).then_some(rva)
    }

    pub fn code(&self, r: Range<usize>) -> Option<&'a [u8]> {
        self.bytes.get(r)
    }

    /// Alle Vorkommen eines Musters in einem Abschnitt.
    pub fn find(&self, pattern: &[u8], within: &Range<usize>) -> Vec<usize> {
        let Some(area) = self.bytes.get(within.clone()) else { return vec![] };
        let Some(first) = pattern.first() else { return vec![] };
        let mut found = vec![];
        let mut at = 0;
        while let Some(i) = area[at..].iter().position(|b| b == first) {
            at += i;
            if area[at..].starts_with(pattern) {
                found.push(within.start + at);
            }
            at += 1;
        }
        found
    }

    /// `lea`-Befehle im Code, die auf eines der Ziele zeigen: (Stelle, Ziel).
    pub fn lea_to(&self, targets: &HashSet<usize>) -> Vec<(usize, usize)> {
        self.leas(&self.text).filter(|(_, t)| targets.contains(t)).collect()
    }

    pub fn lea_targets(&self, within: &Range<usize>) -> Vec<usize> {
        self.leas(within).map(|(_, t)| t).collect()
    }

    /// Alle `lea reg, [rip+x]`.
    fn leas<'b>(&'b self, within: &Range<usize>) -> impl Iterator<Item = (usize, usize)> + 'b {
        let start = within.start;
        let code = self.bytes.get(within.clone()).unwrap_or_default();
        code.windows(7).enumerate().filter(|(_, w)| w[1] == 0x8D && w[0] & 0xFB == 0x48 && w[2] & 0xC7 == 5).filter_map(
            move |(i, w)| {
                let rel = i32::from_le_bytes([w[3], w[4], w[5], w[6]]) as i64;
                let at = start + i;
                Some((at, usize::try_from(at as i64 + 7 + rel).ok()?))
            },
        )
    }

    /// Stellen in den Konstanten, an denen ein Zeiger auf eines der Ziele steht.
    pub fn pointers_to(&self, targets: &HashSet<usize>) -> Vec<usize> {
        self.rdata.clone().step_by(8).filter(|at| self.pointer(*at).is_some_and(|t| targets.contains(&t))).collect()
    }

    /// Funktion, in der diese Stelle liegt.
    pub fn function_around(&self, at: usize) -> Option<Range<usize>> {
        let n = self.pdata.len() / 12;
        let entry = |i: usize| self.u32(self.pdata.start + i * 12).map(|v| v as usize);
        let (mut lo, mut hi) = (0, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match entry(mid)? {
                0 => hi = mid,
                v if v <= at => lo = mid + 1,
                _ => hi = mid,
            }
        }
        self.function(entry(lo.checked_sub(1)?)?).filter(|f| f.contains(&at))
    }

    /// Namen aller Klassen, die den Text enthalten, mit der Lage ihres Typeintrags.
    pub fn classes(&self, part: &str) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = vec![];
        for at in self.find(part.as_bytes(), &self.data) {
            let start = self.bytes[..at].iter().rposition(|b| *b == 0).map_or(0, |i| i + 1);
            let Some(len) = self.bytes[at..].iter().position(|b| *b == 0) else { continue };
            let Ok(name) = std::str::from_utf8(&self.bytes[start..at + len]) else { continue };
            // Vor dem Namen liegen zwei Zeiger.
            if name.starts_with(".?A") && start >= self.data.start + 16 && out.iter().all(|o| o.1 != start - 16) {
                out.push((name.into(), start - 16));
            }
        }
        out
    }

    /// Funktionstabellen einer Klasse über ihren vollständigen Namen, z. B. `.?AVGuiData@@`.
    pub fn vtables(&self, class: &str) -> Vec<usize> {
        let mut out = vec![];
        for (_, td) in self.classes(class).into_iter().filter(|c| c.0 == class) {
            for at in self.find(&(td as u32).to_le_bytes(), &self.rdata) {
                // Beschreibung: Kennung 1, ..., Typ bei 12, eigene Lage bei 20.
                let Some(col) = at.checked_sub(12) else { continue };
                if at % 4 != 0 || self.u32(col) != Some(1) || self.u32(col + 20) != Some(col as u32) {
                    continue;
                }
                // Die Tabelle beginnt direkt hinter dem Zeiger auf die Beschreibung.
                let pointer = (self.base + col as u64).to_le_bytes();
                out.extend(self.find(&pointer, &self.rdata).into_iter().filter(|p| p % 8 == 0).map(|p| p + 8));
            }
        }
        out
    }

    /// Grenzen der Funktion, die genau hier beginnt.
    pub fn function(&self, start: usize) -> Option<Range<usize>> {
        let n = self.pdata.len() / 12;
        let entry = |i: usize| self.u32(self.pdata.start + i * 12).map(|v| v as usize);
        let (mut lo, mut hi) = (0, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            // Leere Einträge stehen am Ende.
            match entry(mid)? {
                0 => hi = mid,
                v if v < start => lo = mid + 1,
                _ => hi = mid,
            }
        }
        let end = self.u32(self.pdata.start + lo * 12 + 4)? as usize;
        (lo < n && entry(lo)? == start && end > start && end <= self.text.end).then_some(start..end)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const BASE: u64 = 0x1_4000_0000;

    /// Kleines Abbild: Code ab 0x1000, Daten ab 0x2000, Namen ab 0x3000, Funktionsliste ab 0x4000.
    pub fn blank() -> Vec<u8> {
        let mut b = vec![0u8; 0x5000];
        put(&mut b, 0x3c, &0x80u32.to_le_bytes());
        put(&mut b, 0x80, b"PE\0\0");
        put(&mut b, 0x86, &4u16.to_le_bytes());
        put(&mut b, 0x94, &240u16.to_le_bytes());
        put(&mut b, 0x98, &0x20bu16.to_le_bytes());
        let names: [&[u8]; 4] = [b".text", b".rdata", b".data", b".pdata"];
        for (i, name) in names.into_iter().enumerate() {
            let at = 0x98 + 240 + i * 40;
            put(&mut b, at, name);
            put(&mut b, at + 8, &0x1000u32.to_le_bytes());
            put(&mut b, at + 12, &(0x1000 * (i as u32 + 1)).to_le_bytes());
        }
        b
    }

    pub fn put(b: &mut [u8], at: usize, v: &[u8]) {
        b[at..at + v.len()].copy_from_slice(v);
    }

    #[test]
    fn kennung_wie_im_spiel() {
        // Im laufenden Spiel gesehen.
        assert_eq!(name_id("button.send"), 0x9C96_DF27);
    }

    #[test]
    fn klasse_bis_zur_funktionstabelle() {
        let mut b = blank();
        put(&mut b, 0x3110, b".?AVBeispiel@@\0");
        put(&mut b, 0x3210, b".?AVBeispielAnders@@\0");
        // Beschreibung bei 0x2100, Zeiger darauf bei 0x2200, Tabelle ab 0x2208.
        put(&mut b, 0x2100, &1u32.to_le_bytes());
        put(&mut b, 0x210c, &0x3100u32.to_le_bytes());
        put(&mut b, 0x2114, &0x2100u32.to_le_bytes());
        put(&mut b, 0x2200, &(BASE + 0x2100).to_le_bytes());
        let i = Image::new(BASE, &b).unwrap();
        assert_eq!(i.classes("Beispiel").len(), 2);
        assert_eq!(i.vtables(".?AVBeispiel@@"), [0x2208]);
        assert!(i.vtables(".?AVBeispielAnders@@").is_empty());
        assert!(i.vtables(".?AVFehlt@@").is_empty());
    }

    #[test]
    fn funktionsgrenzen() {
        let mut b = blank();
        for (k, (s, e)) in [(0x1000u32, 0x1040u32), (0x1040, 0x1100), (0x1200, 0x1300)].into_iter().enumerate() {
            put(&mut b, 0x4000 + k * 12, &s.to_le_bytes());
            put(&mut b, 0x4004 + k * 12, &e.to_le_bytes());
        }
        let i = Image::new(BASE, &b).unwrap();
        assert_eq!(i.function(0x1040), Some(0x1040..0x1100));
        assert_eq!(i.function(0x1050), None);
        assert_eq!(i.function(0x1300), None);
    }

    #[test]
    fn unsinn_ist_kein_abbild() {
        assert!(Image::new(BASE, &[]).is_none());
        assert!(Image::new(BASE, &[0xff; 4096]).is_none());
        assert!(Image::new(BASE, &vec![0; 4096]).is_none());
    }
}
