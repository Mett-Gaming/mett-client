//! Gespeicherte Maße der Oberfläche (nur lesen): In dem Baustein, den jedes Element hat, steht je Maß ein Zeiger auf ein
//! kleines Objekt mit dem ausgerechneten Wert. Wo, ergibt sich daraus, dass er der fertigen Größe gleicht.
use crate::uitree::{Parts, long, pair, short};
use std::collections::{HashMap, HashSet};

/// So viel vom Baustein und vom Maß wird gelesen.
const PART: usize = 0xB0;
const VALUE: usize = 0x50;
const ENOUGH: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizes {
    /// Tabelle des Bausteins.
    pub table: u64,
    /// Wo im Baustein der Zeiger auf die Breite steht.
    pub width: u64,
    /// Wo im Maß der Wert steht.
    pub value: u64,
}

/// Ein Element: Adresse und Inhalt.
pub struct Sample<'a> {
    pub at: u64,
    pub bytes: &'a [u8],
}

type Read<'a> = &'a dyn Fn(u64, &mut [u8]) -> bool;

fn only<T>(mut it: impl Iterator<Item = T>) -> Option<T> {
    let first = it.next()?;
    it.next().is_none().then_some(first)
}

pub fn learn(read: Read, p: &Parts, size: u64, samples: &[Sample]) -> Option<Sizes> {
    let list = |s: &Sample| {
        let (begin, end) = (long(s.bytes, p.list), long(s.bytes, p.list + 8));
        let n = end.checked_sub(begin).filter(|n| *n > 0 && *n <= 64 && n % 8 == 0)?;
        let mut d = vec![0u8; n as usize];
        read(begin, &mut d).then(|| (0..n / 8).map(|i| long(&d, i * 8)).collect::<Vec<u64>>())
    };
    // Der Baustein, den fast alle haben: seine Tabelle ist die häufigste.
    let mut seen: HashMap<u64, usize> = HashMap::new();
    let mut parts: Vec<(&Sample, u64, Vec<u8>)> = vec![];
    for s in samples {
        let mut tables = HashSet::new();
        for at in list(s).unwrap_or_default() {
            let mut b = vec![0u8; PART];
            if at > 0x10000 && read(at, &mut b) {
                tables.insert(long(&b, 0));
                parts.push((s, long(&b, 0), b));
            }
        }
        tables.into_iter().for_each(|t| *seen.entry(t).or_default() += 1);
    }
    let table = only(seen.iter().filter(|(_, n)| **n * 10 >= samples.len() * 9).map(|(t, _)| *t))?;

    // Ein Maß nennt in seinem ersten Wort das Element, zu dem es gehört.
    let mut hits: HashMap<(u64, u64), [usize; 2]> = HashMap::new();
    let mut all: HashMap<u64, usize> = HashMap::new();
    for (s, _, b) in parts.iter().filter(|p| p.1 == table) {
        let own = pair(s.bytes, size);
        // Gleiche oder leere Seiten verraten nichts.
        if own.0 <= 1.0 || own.1 <= 1.0 || (own.0 - own.1).abs() < 1.0 {
            continue;
        }
        for o in (0x10..PART as u64).step_by(8) {
            let at = long(b, o);
            let mut v = [0u8; VALUE];
            if at <= 0x10000 || !at.is_multiple_of(8) || !read(at, &mut v) || long(&v, 0) != s.at + p.delta {
                continue;
            }
            *all.entry(o).or_default() += 1;
            for f in (8..VALUE as u64).step_by(4) {
                let x = f32::from_bits(short(&v, f));
                let n = hits.entry((o, f)).or_default();
                n[0] += ((x - own.0).abs() < 0.1) as usize;
                n[1] += ((x - own.1).abs() < 0.1) as usize;
            }
        }
    }
    let find = |side: usize| {
        let good = |((o, _), n): &(&(u64, u64), &[usize; 2])| n[side] >= ENOUGH && n[side] * 10 >= all[o] * 9;
        only(hits.iter().filter(good).map(|(k, _)| *k))
    };
    let ((width, value), (height, same)) = (find(0)?, find(1)?);
    (value == same && width != height).then_some(Sizes { table, width, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: u64 = 0x7000_0040;
    const P: Parts = Parts { list: 0xC8, owner: 8, delta: 0x10 };

    struct Game {
        mem: HashMap<u64, Vec<u8>>,
        list: Vec<u64>,
    }

    impl Game {
        fn put(&mut self, at: u64, o: usize, v: &[u8]) {
            self.mem.get_mut(&at).unwrap()[o..o + v.len()].copy_from_slice(v);
        }

        fn new() -> Self {
            let mut g = Game { mem: HashMap::new(), list: vec![] };
            for i in 0..40u64 {
                let at = 0x10_0000 + i * 0x1000;
                let size = (30.0 + i as f32 * 3.0, 11.0 + i as f32);
                g.mem.insert(at, vec![0; 0x180]);
                g.put(at, 0x58, &[size.0.to_le_bytes(), size.1.to_le_bytes()].concat());
                g.put(at, 0xC8, &[(at + 0x400).to_le_bytes(), (at + 0x410).to_le_bytes()].concat());
                g.mem.insert(at + 0x400, [at + 0x500, at + 0x600].map(u64::to_le_bytes).concat());
                g.mem.insert(at + 0x500, vec![0; PART]);
                g.mem.insert(at + 0x600, vec![0; PART]);
                g.put(at + 0x500, 0, &TABLE.to_le_bytes());
                g.put(at + 0x600, 0, &(TABLE + 8 + i * 8).to_le_bytes());
                // Waagrecht, senkrecht, Breite, Höhe.
                for (k, v) in [1.0f32, 2.0, size.0, size.1].into_iter().enumerate() {
                    let var = at + 0x800 + k as u64 * 0x80;
                    g.mem.insert(var, vec![0; VALUE]);
                    g.put(var, 0, &(at + 0x10).to_le_bytes());
                    g.put(var, 0x20, &v.to_le_bytes());
                    g.put(at + 0x500, 0x20 + k * 8, &var.to_le_bytes());
                }
                g.list.push(at);
            }
            g
        }

        fn learn(&self) -> Option<Sizes> {
            let samples: Vec<Sample> = self.list.iter().map(|at| Sample { at: *at, bytes: &self.mem[at] }).collect();
            let read = |at: u64, out: &mut [u8]| {
                let Some(b) = self.mem.get(&at).and_then(|b| b.get(..out.len())) else { return false };
                out.copy_from_slice(b);
                true
            };
            learn(&read, &P, 0x58, &samples)
        }
    }

    #[test]
    fn findet_die_gespeicherte_breite() {
        assert_eq!(Game::new().learn(), Some(Sizes { table: TABLE, width: 0x30, value: 0x20 }));
    }

    #[test]
    fn unklares_gilt_nicht() {
        for what in 0..4 {
            let mut g = Game::new();
            for at in g.list.clone() {
                match what {
                    // Zweites Maß mit demselben Wert, zweiter Wert im Maß, fremder Besitzer, falscher Wert.
                    0 => g.put(at + 0x500, 0x40, &(at + 0x900).to_le_bytes()),
                    1 => {
                        let v = g.mem[&(at + 0x900)][0x20..0x24].to_vec();
                        g.put(at + 0x900, 0x30, &v);
                    }
                    2 => g.put(at + 0x900, 0, &7u64.to_le_bytes()),
                    _ => g.put(at + 0x900, 0x20, &5f32.to_le_bytes()),
                }
            }
            assert_eq!(g.learn(), None, "{what}");
        }
    }
}
