//! Eigene Werte (nur lesen): Hunger, Sättigung, Erschöpfung, Leben. Einstieg sind ihre Namen. Andere Spieler haben solche
//! Werte auch, der Server schickt sie aber nicht: Bei ihnen stehen Hunger, Sättigung und Erschöpfung immer auf der
//! Vorgabe. Der eigene Block ist der einzige, der davon abweicht; nur der wird gelesen. Nicht eindeutig heißt nichts.
use crate::Reader;
use std::collections::{HashMap, HashSet};

const NAMES: [&str; 4] =
    ["minecraft:player.hunger", "minecraft:player.saturation", "minecraft:player.exhaustion", "minecraft:health"];
/// So viel eines Werts wird gelesen.
const SPAN: usize = 0xA0;
/// Werte eines Wesens liegen nah beieinander.
const NEAR: u64 = 0x900;
/// Abstand des Namens vom Anfang seiner Beschreibung.
const HEADS: [u64; 5] = [0, 8, 16, 24, 32];
const MOST: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Values {
    pub hunger: f32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub health: f32,
}

/// Namen im Datenbereich des Programms. Gilt für den laufenden Prozess.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Names([u64; 4]);

/// Die eigenen Werte: je Wert Adresse und Beschreibung, dazu die Stelle der Zahl.
#[derive(Clone, Debug, PartialEq)]
pub struct Own {
    blocks: Vec<Block>,
    current: u64,
}

type Read<'a> = &'a (dyn Fn(u64, &mut [u8]) -> bool + Sync);

fn long(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or_default()))
}

fn float(b: &[u8], o: usize) -> f32 {
    b.get(o..o + 4).map_or(f32::NAN, |b| f32::from_le_bytes(b.try_into().unwrap_or_default()))
}

/// Texte im Arbeitsspeicher, dann ihre Namensobjekte im Programm. Je Name genau eines.
pub fn names(r: &Reader, base: u64, image: &[u8]) -> Option<Names> {
    let texts = r.scan(&|at, b| {
        let mut out = vec![];
        for o in (0..b.len().saturating_sub(40)).step_by(8) {
            // Alle fangen gleich an.
            if b[o..].starts_with(b"minecraft:") {
                let hit = |n: &&str| b[o..].starts_with(n.as_bytes()) && b[o + n.len()] == 0;
                out.extend(NAMES.iter().position(hit).map(|k| ((at + o) as u64, k)));
            }
        }
        out
    });
    objects(&texts.into_iter().collect(), base, image)
}

fn objects(texts: &HashMap<u64, usize>, base: u64, image: &[u8]) -> Option<Names> {
    let words = image.as_chunks::<8>().0;
    let mut found: [Vec<u64>; 4] = Default::default();
    for (i, w) in words.iter().enumerate() {
        let Some(k) = texts.get(&u64::from_le_bytes(*w)) else { continue };
        let len = words.get(i + 2).map_or(0, |w| u64::from_le_bytes(*w));
        if len == NAMES[*k].len() as u64 {
            found[*k].push(base + i as u64 * 8);
        }
    }
    let one = |k: usize| if let [x] = found[k][..] { Some(x) } else { None };
    Some(Names([one(0)?, one(1)?, one(2)?, one(3)?]))
}

/// Alle Stellen mit zwei Dreiern (Grenzen und Zahl dazwischen) hintereinander: Vorgabe und Stand, beide von 0 bis zur
/// selben Obergrenze. Bei Hunger und Sättigung ist sie 20, bei der Erschöpfung nicht.
fn pairs(b: &[u8], most: Option<f32>) -> HashSet<usize> {
    let triple =
        |o: usize, max: f32| float(b, o) == 0.0 && float(b, o + 4) == max && (0.0..=max).contains(&float(b, o + 8));
    let fits = |o: &usize| {
        let max = float(b, o + 4);
        let range = most.map_or((1.0..=1000.0).contains(&max), |m| m == max);
        range && triple(*o, max) && triple(o + 12, max)
    };
    (0x20..SPAN - 24).step_by(4).filter(fits).collect()
}

pub type Block = [(u64, u64); 4];

/// Alle Blöcke mit diesen Werten. Durchsucht den ganzen Speicher, also selten aufrufen.
pub fn blocks(r: &Reader, n: &Names) -> Vec<Block> {
    let wanted: HashMap<u64, (usize, u64)> =
        HEADS.iter().flat_map(|d| (0..4).map(move |k| (k, *d))).map(|(k, d)| (n.0[k] - d, (k, d))).collect();
    let spots = r.scan(&|at, b| {
        let words = b.as_chunks::<8>().0.iter().enumerate();
        words.filter_map(|(i, w)| Some(((at + i * 8) as u64, *wanted.get(&u64::from_le_bytes(*w))?))).collect()
    });
    group(n, &spots)
}

/// Der eigene unter den Blöcken; liest nur sie.
pub fn find(r: &Reader, blocks: &[Block]) -> Option<Own> {
    own(&|at, out| r.read(at as usize, out).is_ok(), blocks)
}

fn group(n: &Names, spots: &[(u64, (usize, u64))]) -> Vec<Block> {
    let mut by: HashMap<(usize, u64), Vec<u64>> = HashMap::new();
    spots.iter().for_each(|(at, key)| by.entry(*key).or_default().push(*at));
    by.values_mut().for_each(|v| v.sort_unstable());
    // Zu jedem Hunger die anderen Werte desselben Wesens.
    let mut blocks = vec![];
    for d in HEADS {
        for at in by.get(&(0, d)).into_iter().flatten() {
            let near = |k: usize| {
                let mut it = by.get(&(k, d))?.iter().filter(|o| o.abs_diff(*at) < NEAR);
                let first = *it.next()?;
                it.next().is_none().then_some((first, n.0[k] - d))
            };
            if let (Some(s), Some(e), Some(h)) = (near(1), near(2), near(3)) {
                blocks.push([(*at, n.0[0] - d), s, e, h]);
            }
        }
    }
    blocks
}

fn own(read: Read, blocks: &[Block]) -> Option<Own> {
    // In jedem Wert stehen Vorgabe und Stand mit denselben Grenzen hintereinander.
    let look = |block: &Block| {
        let mut data = vec![];
        for (at, _) in &block[..3] {
            let mut b = vec![0u8; SPAN];
            if !read(*at, &mut b) {
                return None;
            }
            data.push(b);
        }
        let sets = data.iter().enumerate().map(|(k, b)| pairs(b, (k < 2).then_some(MOST)));
        let both: Vec<usize> = sets.reduce(|a, b| &a & &b)?.into_iter().collect();
        let [a] = both[..] else { return None };
        Some((a + 12, data.iter().map(|d| (float(d, a + 8), float(d, a + 20))).collect::<Vec<_>>()))
    };
    let seen: Vec<_> = blocks.iter().filter_map(|b| look(b).map(|l| (b, l))).collect();
    let places: HashSet<usize> = seen.iter().map(|s| s.1.0).collect();
    let [place] = places.into_iter().collect::<Vec<_>>()[..] else { return None };
    // Nur der eigene weicht von den Vorgaben ab. Im Einzelspieler gibt es ihn zweimal, dann aber gleich.
    let ours: Vec<_> = seen.iter().filter(|s| s.1.1.iter().any(|(d, v)| (d - v).abs() > 1e-4)).collect();
    let first = &ours.first()?.1.1;
    let same = ours.iter().all(|s| s.1.1.iter().zip(first).all(|(a, b)| (a.1 - b.1).abs() < 0.5));
    (same && ours.len() <= 2).then(|| Own { blocks: ours.iter().map(|s| *s.0).collect(), current: place as u64 + 8 })
}

impl Own {
    /// `None`: Die Werte gibt es nicht mehr.
    pub fn read(&self, r: &Reader) -> Option<Values> {
        self.values(&|at, out| r.read(at as usize, out).is_ok())
    }

    fn values(&self, read: Read) -> Option<Values> {
        let block = self.blocks.first()?;
        let get = |k: usize| {
            let (at, about) = block[k];
            let mut b = [0u8; SPAN];
            let v = read(at, &mut b).then(|| float(&b, self.current as usize))?;
            (long(&b, 0) == about && v.is_finite() && (0.0..=1024.0).contains(&v)).then_some(v)
        };
        Some(Values { hunger: get(0)?, saturation: get(1)?, exhaustion: get(2)?, health: get(3)? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: Names = Names([0x7000_0010, 0x7000_0110, 0x7000_0210, 0x7000_0310]);
    const STEP: u64 = 0x88;

    struct Game {
        mem: HashMap<u64, Vec<u8>>,
        spots: Vec<(u64, (usize, u64))>,
    }

    impl Game {
        fn new() -> Self {
            Game { mem: HashMap::new(), spots: vec![] }
        }

        /// Ein Wesen: Hunger, Sättigung, Erschöpfung, Leben.
        fn add(&mut self, at: u64, now: [f32; 4]) {
            for (k, v) in now.into_iter().enumerate() {
                let at = at + k as u64 * STEP;
                let mut b = vec![0u8; SPAN];
                b[..8].copy_from_slice(&(N.0[k] - 16).to_le_bytes());
                let default = [20.0f32, 5.0, 0.0, 20.0][k];
                let most = [20.0f32, 20.0, 5.0, 20.0][k];
                for (o, f) in [(0x64, most), (0x68, default), (0x70, most), (0x74, v)] {
                    b[o..o + 4].copy_from_slice(&f.to_le_bytes());
                }
                self.mem.insert(at, b);
                self.spots.push((at, (k, 16)));
            }
        }

        fn own(&self) -> Option<Values> {
            let read = |at: u64, out: &mut [u8]| {
                let Some(b) = self.mem.get(&at).and_then(|b| b.get(..out.len())) else { return false };
                out.copy_from_slice(b);
                true
            };
            own(&read, &group(&N, &self.spots))?.values(&read)
        }
    }

    #[test]
    fn nur_der_abweichende_block_ist_unserer() {
        let mut g = Game::new();
        for i in 0..30 {
            // Andere: Vorgaben, nur das Leben ist bekannt.
            g.add(0x10_0000 + i * 0x10000, [20.0, 5.0, 0.0, 3.0 + i as f32 * 0.5]);
        }
        assert_eq!(g.own(), None);
        g.add(0x90_0000, [14.0, 2.5, 1.75, 9.0]);
        let want = Values { hunger: 14.0, saturation: 2.5, exhaustion: 1.75, health: 9.0 };
        assert_eq!(g.own(), Some(want));
        // Einzelspieler: zweimal dasselbe.
        g.add(0xA0_0000, [14.0, 2.5, 1.75, 9.0]);
        assert_eq!(g.own(), Some(want));
    }

    #[test]
    fn mehrdeutig_heisst_nichts() {
        let mut g = Game::new();
        g.add(0x10_0000, [14.0, 2.5, 1.75, 9.0]);
        g.add(0x20_0000, [6.0, 0.0, 3.0, 9.0]);
        assert_eq!(g.own(), None);
        // Kaputter Aufbau zählt nicht.
        let mut g = Game::new();
        g.add(0x10_0000, [14.0, 2.5, 1.75, 9.0]);
        g.mem.get_mut(&0x10_0088).unwrap()[0x70..0x74].copy_from_slice(&7f32.to_le_bytes());
        assert_eq!(g.own(), None);
    }

    #[test]
    fn verschwundene_werte_werden_bemerkt() {
        let mut g = Game::new();
        g.add(0x10_0000, [14.0, 2.5, 1.75, 9.0]);
        let found = {
            let read = |at: u64, out: &mut [u8]| {
                let Some(b) = g.mem.get(&at).and_then(|b| b.get(..out.len())) else { return false };
                out.copy_from_slice(b);
                true
            };
            own(&read, &group(&N, &g.spots)).unwrap()
        };
        g.mem.get_mut(&0x10_0000).unwrap()[0] ^= 1;
        let read = |at: u64, out: &mut [u8]| {
            let Some(b) = g.mem.get(&at).and_then(|b| b.get(..out.len())) else { return false };
            out.copy_from_slice(b);
            true
        };
        assert_eq!(found.values(&read), None);
    }

    #[test]
    fn namensobjekte_muessen_eindeutig_sein() {
        let texts: HashMap<u64, usize> = (0..4).map(|k| (0x5000 + k as u64 * 0x40, k)).collect();
        let mut image = vec![0u8; 0x400];
        let put = |m: &mut Vec<u8>, at: usize, k: usize| {
            m[at..at + 8].copy_from_slice(&(0x5000 + k as u64 * 0x40).to_le_bytes());
            m[at + 16..at + 24].copy_from_slice(&NAMES[k].len().to_le_bytes());
        };
        (0..4).for_each(|k| put(&mut image, 0x40 + k * 0x40, k));
        assert_eq!(objects(&texts, 0x1000, &image), Some(Names([0x1040, 0x1080, 0x10C0, 0x1100])));
        put(&mut image, 0x300, 2);
        assert_eq!(objects(&texts, 0x1000, &image), None);
    }
}
