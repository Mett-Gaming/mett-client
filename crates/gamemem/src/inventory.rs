//! Eigenes Inventar, eigene Rüstung und eigene Effekte (nur lesen). Einstieg sind die Namen der Gegenstände: Neben jedem
//! Namen steht seine Prüfsumme. Nur Stapel mit einer Nummer des Servers sind eigene; bei allen anderen Wesen fehlt sie.
//! Passt der Aufbau nicht, gibt es nichts.
use crate::Reader;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;

type Read<'a> = &'a (dyn Fn(u64, &mut [u8]) -> bool + Sync);

/// Abstand zwischen kurzem und vollem Namen einer Art, und wie weit vorher sie höchstens anfängt.
const PAIR: usize = 0x50;
const REACH: u64 = 0x200;
/// In einem Stapel: Zusatz, Anzahl, Nummer des Servers. Jede Reihe wird damit auf Plausibilität geprüft.
const EXTRA: usize = 0x10;
const COUNT: usize = 0x22;
const NUMBER: usize = 0x80;
/// Art der Nummer; 0 ist die vom Server.
const SORT: usize = 0x90;
const SPAN: usize = 0x98;
/// So oft muss eine Rüstung unverändert gelesen werden, bevor sie als unsere gilt.
const SURE: u32 = 4;
/// Seltenere Klassen sind Zufall.
const ENOUGH: usize = 12;
pub const SLOTS: u64 = 36;
/// Helm, Brust, Beine, Füße und ein Platz für Tiere.
const WORN: u64 = 5;
const MOST: u8 = 64;
/// Effekte: Abstand der Plätze, jeder an der Stelle seiner Nummer.
const EFFECT: u64 = 0xA8;
/// Die Reihe reicht bis zur höchsten Nummer, die es gab.
const EFFECTS: Range<u64> = 2..64;
/// Hier steht bei jedem laufenden Effekt eine 1.
const ONE: usize = 0x50;
const LONGEST: i32 = 20 * 60 * 60 * 8;

fn long(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or_default()))
}

fn int(b: &[u8], o: usize) -> i32 {
    b.get(o..o + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap_or_default()))
}

fn word(read: Read, at: u64) -> Option<u64> {
    let mut b = [0u8; 8];
    read(at, &mut b).then(|| u64::from_le_bytes(b))
}

/// Prüfsumme, die Minecraft neben Namen legt.
fn sum(s: &[u8]) -> u64 {
    s.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| h.wrapping_mul(0x100_0000_01b3) ^ *b as u64)
}

/// Text aus 32 Bytes, kurz darin oder lang über seinen Zeiger.
fn text(read: Read, b: &[u8]) -> Option<Vec<u8>> {
    let (len, room) = (long(b, 16) as usize, long(b, 24));
    if len == 0 || len > 80 || room < len as u64 || !(15..=4096).contains(&room) {
        return None;
    }
    let mut t = vec![0u8; len];
    if room == 15 {
        t.copy_from_slice(b.get(..len)?);
    } else if !read(long(b, 0), &mut t) {
        return None;
    }
    let plain = |c: &u8| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.:-".contains(c);
    t.iter().all(plain).then_some(t)
}

/// Prüfsumme, dann der Text dazu.
fn hashed(read: Read, b: &[u8], o: usize) -> Option<Vec<u8>> {
    let h = long(b, o);
    if h < 1 << 40 {
        return None;
    }
    text(read, b.get(o + 8..o + 40)?).filter(|t| sum(t) == h)
}

/// Arten und Stapelklasse. Gilt für den laufenden Prozess.
#[derive(Clone, Debug, PartialEq)]
pub struct Kinds {
    /// Zeiger in einem Stapel → Name der Art.
    refs: HashMap<u64, String>,
    table: u64,
    step: u64,
}

pub fn kinds(r: &Reader, image: &Range<u64>) -> Option<Kinds> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    // 1. Namenspaare.
    let pairs: HashMap<u64, String> = r
        .scan(&|at, b| {
            let mut out = vec![];
            for i in (0..b.len().saturating_sub(PAIR + 48)).step_by(8) {
                if long(b, i + PAIR) < 1 << 40
                    || !(1..80).contains(&long(b, i + 24))
                    || !(1..80).contains(&long(b, i + PAIR + 24))
                {
                    continue;
                }
                let Some(full) = hashed(read, b, i + PAIR) else { continue };
                let Some(short) = hashed(read, b, i) else { continue };
                if full.ends_with(&short) && full.len() > short.len() && full.contains(&b':') {
                    out.push(((at + i) as u64, String::from_utf8_lossy(&full).into_owned()));
                }
            }
            out
        })
        .into_iter()
        .collect();
    let mut starts: Vec<u64> = pairs.keys().copied().collect();
    starts.sort_unstable();
    // 2. Wer zeigt kurz davor? Der häufigste Abstand ist der Anfang der Art.
    let near: Vec<(u64, u64, u64)> = r.scan(&|at, b| {
        let words = b.as_chunks::<8>().0.iter().enumerate();
        words
            .filter_map(|(i, w)| {
                let v = u64::from_le_bytes(*w);
                let pair = *starts.get(starts.partition_point(|s| *s < v))?;
                (pair - v < REACH).then_some(((at + i * 8) as u64, v, pair))
            })
            .collect()
    });
    let mut votes: BTreeMap<u64, HashSet<u64>> = BTreeMap::new();
    for (_, v, pair) in &near {
        votes.entry(pair - v).or_default().insert(*pair);
    }
    let in_image = |at: u64| word(read, at).is_some_and(|t| image.contains(&t));
    let (delta, _) = votes
        .iter()
        .filter(|(d, p)| p.iter().take(20).filter(|p| in_image(**p - **d)).count() * 10 >= p.len().min(20) * 9)
        .max_by_key(|(_, p)| p.len())?;
    let refs: HashMap<u64, String> =
        near.iter().filter(|n| n.2 - n.1 == *delta).filter_map(|n| Some((n.0, pairs.get(&n.2)?.clone()))).collect();
    // 3. Stapel: Tabelle des Programms, dann ein solcher Zeiger. Die Klasse des Inventars ergibt Reihen.
    let stacks: Vec<(u64, u64)> = r.scan(&|at, b| {
        let w = b.as_chunks::<8>().0.windows(2).enumerate();
        w.filter_map(|(i, w)| {
            let table = u64::from_le_bytes(w[0]);
            (refs.contains_key(&u64::from_le_bytes(w[1])) && image.contains(&table))
                .then_some(((at + i * 8) as u64, table))
        })
        .collect()
    });
    let mut by: HashMap<u64, Vec<u64>> = HashMap::new();
    stacks.iter().for_each(|(at, t)| by.entry(*t).or_default().push(*at));
    let mut fits = vec![];
    for (table, _) in by.iter().filter(|(_, v)| v.len() >= ENOUGH) {
        let all = places(r, *table);
        // Abstand, bei dem genau 36 Plätze eine Reihe bilden.
        let steps = all.windows(2).map(|w| w[1] - w[0]).filter(|d| (0x60..0x200).contains(d));
        for step in steps.collect::<HashSet<_>>() {
            let n = rows(&all, step).into_iter().filter(|r| r.1 == SLOTS).count();
            if n > 0 && step as usize >= SPAN {
                fits.push((n, *table, step));
            }
        }
    }
    fits.sort_unstable();
    let (_, table, step) = fits.pop()?;
    Some(Kinds { refs, table, step })
}

fn places(r: &Reader, table: u64) -> Vec<u64> {
    let mut all = r.scan(&|at, b| {
        let words = b.as_chunks::<8>().0.iter().enumerate();
        words.filter(|(_, w)| u64::from_le_bytes(**w) == table).map(|(i, _)| (at + i * 8) as u64).collect()
    });
    all.sort_unstable();
    all
}

/// Zusammenhängende Reihen: Anfang und Länge.
fn rows(all: &[u64], step: u64) -> Vec<(u64, u64)> {
    let have: HashSet<u64> = all.iter().copied().collect();
    let mut out = vec![];
    for a in all {
        if have.contains(&(a - step)) {
            continue;
        }
        let mut n = 1;
        while have.contains(&(a + n * step)) && n < 100 {
            n += 1;
        }
        out.push((*a, n));
    }
    out
}

/// Ein belegter Platz.
#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    pub slot: u64,
    pub name: String,
    pub count: u8,
    pub damage: i32,
    number: i32,
}

/// Plätze von Inventar und Rüstung, auch veraltete Kopien; welche gilt, zeigt sich beim Lesen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Places {
    main: Vec<u64>,
    worn: Vec<u64>,
}

impl Places {
    pub fn is_empty(&self) -> bool {
        self.main.is_empty()
    }
}

/// Nur die Rüstung neu suchen.
pub fn find_worn(r: &Reader, k: &Kinds, p: &mut Places) {
    p.worn = find(r, k).worn;
}

pub fn find(r: &Reader, k: &Kinds) -> Places {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let all = rows(&places(r, k.table), k.step);
    let of = |n: u64| {
        let mine = all.iter().filter(|x| x.1 == n).map(|x| x.0);
        mine.filter(|at| row(read, k, *at, n).is_some()).collect::<Vec<_>>()
    };
    Places { main: of(SLOTS), worn: of(WORN) }
}

/// Alle belegten Plätze einer Reihe. `None`: nicht unsere oder nicht mehr da.
fn row(read: Read, k: &Kinds, at: u64, n: u64) -> Option<Vec<Stack>> {
    let mut out = vec![];
    let mut numbers = HashSet::new();
    let mut all = vec![0u8; (n * k.step) as usize];
    if !read(at, &mut all) {
        return None;
    }
    for slot in 0..n {
        let b = &all[(slot * k.step) as usize..];
        if long(b, 0) != k.table {
            return None;
        }
        let Some(name) = k.refs.get(&long(b, 8)) else { continue };
        let (count, number) = (b[COUNT], int(b, NUMBER));
        if count == 0 {
            continue;
        }
        // Eigene haben eine eigene Nummer; gibt es sie nicht, gehört die ganze Reihe nicht uns.
        if count > MOST || number <= 0 || b[SORT] != 0 || !numbers.insert(number) {
            return None;
        }
        out.push(Stack { slot, name: name.clone(), count, damage: damage(read, long(b, EXTRA)), number });
    }
    Some(out)
}

/// Abnutzung aus dem Zusatz: ein geordneter Baum mit Namen.
fn damage(read: Read, extra: u64) -> i32 {
    let mut b = [0u8; 24];
    if extra < 0x10000 || !read(extra, &mut b) {
        return 0;
    }
    let (head, size) = (long(&b, 8), long(&b, 16));
    if size == 0 || size > 256 {
        return 0;
    }
    let mut at = word(read, head + 8).unwrap_or(0);
    for _ in 0..64 {
        let mut n = [0u8; 0x50];
        if at < 0x10000 || at == head || !read(at, &mut n) || n[0x19] != 0 {
            return 0;
        }
        let Some(key) = name(read, &n[0x20..0x40]) else { return 0 };
        at = match key.as_slice().cmp(b"Damage") {
            std::cmp::Ordering::Equal => return int(&n, 0x48).clamp(0, 100_000),
            std::cmp::Ordering::Less => long(&n, 0x10),
            std::cmp::Ordering::Greater => long(&n, 0),
        };
    }
    0
}

/// Beliebiger Name im Baum.
fn name(read: Read, b: &[u8]) -> Option<Vec<u8>> {
    let (len, room) = (long(b, 16) as usize, long(b, 24));
    if len > 200 || room < len as u64 || room > 4096 {
        return None;
    }
    let mut t = vec![0u8; len];
    if room == 15 {
        t.copy_from_slice(b.get(..len)?);
    } else if !read(long(b, 0), &mut t) {
        return None;
    }
    Some(t)
}

/// Was man gerade hat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Own {
    pub main: Vec<Stack>,
    pub worn: Vec<Stack>,
}

/// Merkt sich, welche Kopie zuletzt lebte: Veraltete ändern sich nie mehr.
#[derive(Default)]
pub struct Watch {
    seen: HashMap<u64, Vec<Stack>>,
    best: [Option<u64>; 2],
    /// Nummern je Rüstungsreihe und wie oft sie so gelesen wurden.
    steady: HashMap<u64, (Vec<i32>, u32)>,
    worn: Vec<Stack>,
}

impl Watch {
    /// `None`: Es gibt keine mehr.
    pub fn read(&mut self, r: &Reader, k: &Kinds, p: &Places) -> Option<Own> {
        self.look(&|at, out| r.read(at as usize, out).is_ok(), k, p)
    }

    fn look(&mut self, read: Read, k: &Kinds, p: &Places) -> Option<Own> {
        let alive = |list: &[u64], n: u64| -> Vec<(u64, Vec<Stack>)> {
            list.iter().filter_map(|at| Some((*at, row(read, k, *at, n)?))).collect()
        };
        let main = self.pick(0, alive(&p.main, SLOTS))?;
        let worn = alive(&p.worn, WORN).into_iter().filter(|(_, w)| ours(&main, w)).collect::<Vec<_>>();
        self.steady.retain(|at, _| worn.iter().any(|w| w.0 == *at));
        let mut sure = vec![];
        for (at, w) in worn {
            let now: Vec<i32> = w.iter().map(|s| s.number).collect();
            let e = self.steady.entry(at).or_insert_with(|| (now.clone(), 0));
            *e = if e.0 == now { (now, e.1 + 1) } else { (now, 1) };
            if e.1 >= SURE {
                sure.push((at, w));
            }
        }
        // Während einer Änderung gilt kurz noch der alte Stand.
        let pending = sure.is_empty() && self.steady.values().any(|s| !s.0.is_empty());
        if !pending {
            self.worn = self.pick(1, sure).unwrap_or_default();
        }
        Some(Own { main, worn: self.worn.clone() })
    }

    /// Die Kopie, die zuletzt lebte; solange sich nichts bewegt, die vollste.
    fn pick(&mut self, which: usize, alive: Vec<(u64, Vec<Stack>)>) -> Option<Vec<Stack>> {
        for (at, now) in &alive {
            let before = self.seen.insert(*at, now.clone());
            if before.is_some_and(|b| b != *now) {
                self.best[which] = Some(*at);
            }
        }
        let best = self.best[which].filter(|b| alive.iter().any(|a| a.0 == *b));
        self.best[which] = best;
        let pick = best.or_else(|| alive.iter().max_by_key(|a| a.1.len()).map(|a| a.0));
        alive.into_iter().find(|a| Some(a.0) == pick).map(|a| a.1)
    }
}

/// Der Server zählt unsere Stapel fortlaufend. Datenmüll bei anderen Wesen fällt weit daraus heraus.
fn ours(main: &[Stack], worn: &[Stack]) -> bool {
    let most = main.iter().map(|s| s.number).max().unwrap_or(0);
    worn.iter().all(|w| w.number <= most.saturating_mul(2).saturating_add(200))
        && !worn.iter().any(|w| main.iter().any(|m| m.number == w.number))
}

/// Ein laufender Effekt, Rest in Zwanzigstelsekunden.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effect {
    pub id: u32,
    pub left: i32,
}

/// Anfang und Plätze der Reihe, auf die eine Liste zeigt.
fn list(b: &[u8]) -> Option<(u64, u64)> {
    let (begin, end, room) = (long(b, 0), long(b, 8), long(b, 16));
    let size = end.checked_sub(begin)?;
    let fits = begin > 0x10000 && begin.is_multiple_of(8) && size.is_multiple_of(EFFECT) && room >= end;
    (fits && EFFECTS.contains(&(size / EFFECT)) && (room - begin).is_multiple_of(EFFECT))
        .then_some((begin, size / EFFECT))
}

/// Listen mit laufenden Effekten: Jeder belegte Platz trägt seine eigene Nummer. Die Listen bleiben, auch wenn ihre Reihe
/// wächst oder leer wird.
pub fn effects(r: &Reader) -> Vec<u64> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let all: Vec<(u64, (u64, u64))> = r.scan(&|at, b| {
        let o = (0..b.len().saturating_sub(23)).step_by(8);
        o.filter_map(|i| Some(((at + i) as u64, list(&b[i..i + 24])?))).collect()
    });
    let good = |(_, (at, n)): &(u64, (u64, u64))| running(read, *at, *n).is_some_and(|e| !e.is_empty());
    all.into_iter().filter(good).map(|l| l.0).collect()
}

fn running(read: Read, at: u64, n: u64) -> Option<Vec<Effect>> {
    let mut b = vec![0u8; (n * EFFECT) as usize];
    if !read(at, &mut b) {
        return None;
    }
    let mut out = vec![];
    for k in 0..n {
        let o = (k * EFFECT) as usize;
        let (id, left) = (int(&b, o), int(&b, o + 4));
        match id {
            0 => {}
            i if i as u64 == k && left > 0 && int(&b, o + ONE) == 1f32.to_bits() as i32 => {
                out.push(Effect { id: i as u32, left })
            }
            // Abgelaufen.
            i if i as u64 == k && left <= 0 => {}
            _ => return None,
        }
        if left > LONGEST {
            return None;
        }
    }
    Some(out)
}

/// Die eigenen Effekte. Minecraft hält davon viele Kopien der letzten Takte, die jüngste hat am wenigsten übrig. Haben
/// zwei Reihen nichts miteinander zu tun, gehört eine nicht uns; dann gibt es nichts.
pub fn own_effects(r: &Reader, lists: &[u64]) -> Option<Vec<Effect>> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let now = |at: &u64| {
        let mut b = [0u8; 24];
        let (begin, n) = read(*at, &mut b).then(|| list(&b))??;
        running(read, begin, n)
    };
    newest(lists.iter().filter_map(now).collect())
}

fn newest(all: Vec<Vec<Effect>>) -> Option<Vec<Effect>> {
    let ids = |e: &[Effect]| e.iter().map(|e| e.id).collect::<HashSet<_>>();
    let filled: Vec<&Vec<Effect>> = all.iter().filter(|e| !e.is_empty()).collect();
    let Some(first) = filled.first() else { return (!all.is_empty()).then(Vec::new) };
    let shared = filled.iter().fold(ids(first), |a, e| &a & &ids(e));
    let one = shared.iter().min()?;
    let comparable =
        filled.iter().all(|a| filled.iter().all(|b| ids(a).is_subset(&ids(b)) || ids(b).is_subset(&ids(a))));
    let left = |e: &&&Vec<Effect>| e.iter().find(|x| x.id == *one).map_or(i32::MAX, |x| x.left);
    let best = filled.iter().min_by_key(left)?;
    comparable.then(|| best.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mem(HashMap<u64, Vec<u8>>);

    impl Mem {
        fn read(&self, at: u64, out: &mut [u8]) -> bool {
            for (start, b) in &self.0 {
                if let Some(o) = at.checked_sub(*start)
                    && let Some(part) = b.get(o as usize..o as usize + out.len())
                {
                    out.copy_from_slice(part);
                    return true;
                }
            }
            false
        }
    }

    const TABLE: u64 = 0x7000;

    fn kinds() -> Kinds {
        let refs = [(0x9000, "minecraft:arrow".into()), (0x9010, "minecraft:iron_boots".into())].into();
        Kinds { refs, table: TABLE, step: 0x98 }
    }

    /// Reihe mit (Platz, Art, Anzahl, Nummer).
    fn line(n: u64, items: &[(u64, u64, u8, i32)]) -> Vec<u8> {
        let mut b = vec![0u8; (n * 0x98) as usize];
        for k in 0..n as usize {
            b[k * 0x98..][..8].copy_from_slice(&TABLE.to_le_bytes());
        }
        for (slot, kind, count, number) in items {
            let o = *slot as usize * 0x98;
            b[o + 8..o + 16].copy_from_slice(&kind.to_le_bytes());
            b[o + COUNT] = *count;
            b[o + NUMBER..o + NUMBER + 4].copy_from_slice(&number.to_le_bytes());
        }
        b
    }

    #[test]
    fn pruefsumme_wie_im_spiel() {
        assert_eq!(sum(b""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(sum(b"bread"), sum(b"daerb"));
    }

    #[test]
    fn nur_reihen_mit_eigenen_nummern() {
        let k = kinds();
        let check = |items: &[(u64, u64, u8, i32)]| {
            let m = Mem([(0x100000, line(5, items))].into());
            row(&|a, o| m.read(a, o), &k, 0x100000, 5)
        };
        assert_eq!(check(&[(3, 0x9010, 1, 28)]).unwrap()[0].name, "minecraft:iron_boots");
        // Andere Wesen: keine Nummer.
        assert_eq!(check(&[(3, 0x9010, 1, 0)]), None);
        assert_eq!(check(&[(3, 0x9010, 1, 28), (2, 0x9010, 1, 0)]), None);
        // Doppelte Nummer oder unmögliche Anzahl: falscher Aufbau.
        assert_eq!(check(&[(3, 0x9010, 1, 28), (2, 0x9010, 1, 28)]), None);
        assert_eq!(check(&[(3, 0x9000, 200, 28)]), None);
        // Reste mit Anzahl 0 zählen nicht.
        assert_eq!(check(&[(1, 0x9000, 0, 0)]), Some(vec![]));
    }

    #[test]
    fn die_lebende_kopie_gewinnt() {
        let (k, mut w) = (kinds(), Watch::default());
        let p = Places { main: vec![0x100000, 0x200000], worn: vec![] };
        let mem = |a: u8, b: u8| {
            Mem([(0x100000, line(36, &[(0, 0x9000, a, 5)])), (0x200000, line(36, &[(0, 0x9000, b, 5)]))].into())
        };
        let mut count = |a, b| {
            let m = mem(a, b);
            w.look(&|x, o| m.read(x, o), &k, &p).unwrap().main[0].count
        };
        assert_eq!(count(10, 10), 10);
        assert_eq!(count(10, 9), 9);
        assert_eq!(count(10, 9), 9);
        assert_eq!(count(10, 8), 8);
    }

    #[test]
    fn fremde_ruestung_wird_nie_unsere() {
        let k = kinds();
        let p = Places { main: vec![0x100000], worn: vec![0x200000] };
        let look = |w: &mut Watch, number: i32| {
            let m =
                Mem([(0x100000, line(36, &[(0, 0x9000, 3, 45)])), (0x200000, line(5, &[(1, 0x9010, 1, number)]))]
                    .into());
            w.look(&|x, o| m.read(x, o), &k, &p).unwrap().worn.len()
        };
        // Datenmüll statt einer Nummer, auch dauerhaft.
        let mut w = Watch::default();
        assert!((0..20).all(|_| look(&mut w, 428_087_936) == 0));
        // Dieselbe Nummer wie ein Stapel im Inventar.
        assert!((0..20).all(|_| look(&mut w, 45) == 0));
        // Kurz aufblitzende Werte zählen nicht.
        assert!((0..20).all(|i| look(&mut w, 10 + i) == 0));
        // Eigene: erst sicher, dann da, und nach dem Ablegen weg.
        let mut w = Watch::default();
        let seen: Vec<usize> = (0..6).map(|_| look(&mut w, 46)).collect();
        assert_eq!(seen, [0, 0, 0, 1, 1, 1]);
        assert_eq!((0..5).map(|_| look(&mut w, 0)).last(), Some(0));
    }

    #[test]
    fn abnutzung_aus_dem_baum() {
        let mut head = vec![0u8; 0x50];
        head[8..16].copy_from_slice(&0x30000u64.to_le_bytes());
        let node = |key: &str, left: u64, right: u64, v: i32| {
            let mut n = vec![0u8; 0x50];
            n[..8].copy_from_slice(&left.to_le_bytes());
            n[0x10..0x18].copy_from_slice(&right.to_le_bytes());
            n[0x20..0x20 + key.len()].copy_from_slice(key.as_bytes());
            n[0x30..0x38].copy_from_slice(&key.len().to_le_bytes());
            n[0x38..0x40].copy_from_slice(&15u64.to_le_bytes());
            n[0x48..0x4C].copy_from_slice(&v.to_le_bytes());
            n
        };
        let mut tag = vec![0u8; 24];
        tag[8..16].copy_from_slice(&0x20000u64.to_le_bytes());
        tag[16] = 2;
        let m = Mem([
            (0x10000, tag),
            (0x20000, head),
            (0x30000, node("RepairCost", 0x40000, 0x20000, 9)),
            (0x40000, node("Damage", 0x20000, 0x20000, 77)),
        ]
        .into());
        assert_eq!(damage(&|a, o| m.read(a, o), 0x10000), 77);
        assert_eq!(damage(&|a, o| m.read(a, o), 0), 0);
        assert_eq!(damage(&|a, o| m.read(a, o), 0x50000), 0);
    }

    #[test]
    fn juengste_kopie_der_effekte() {
        let e = |id, left| Effect { id, left };
        let old = vec![e(16, 120), e(18, 5)];
        let new = vec![e(16, 100)];
        assert_eq!(newest(vec![old.clone(), new.clone(), vec![]]), Some(new));
        // Neuer Effekt: steht nur in der jüngsten.
        let more = vec![e(1, 600), e(16, 90)];
        assert_eq!(newest(vec![more.clone(), vec![e(16, 120)]]), Some(more));
        // Fremde Effekte: lieber nichts.
        assert_eq!(newest(vec![old, vec![e(20, 100)]]), None);
        assert_eq!(newest(vec![vec![e(1, 5), e(2, 5)], vec![e(2, 4), e(3, 4)]]), None);
        assert_eq!(newest(vec![vec![]]), Some(vec![]));
        assert_eq!(newest(vec![]), None);
    }

    #[test]
    fn effekte_stehen_an_ihrer_nummer() {
        let list = |items: &[(usize, i32, i32)]| {
            let mut b = vec![0u8; 37 * EFFECT as usize];
            for (slot, id, left) in items {
                let o = slot * EFFECT as usize;
                b[o..o + 4].copy_from_slice(&id.to_le_bytes());
                b[o + 4..o + 8].copy_from_slice(&left.to_le_bytes());
                b[o + ONE..o + ONE + 4].copy_from_slice(&1f32.to_le_bytes());
            }
            let m = Mem([(0x100000, b)].into());
            running(&|a, o| m.read(a, o), 0x100000, 37)
        };
        let want = vec![Effect { id: 10, left: 1811 }, Effect { id: 16, left: 5433 }];
        assert_eq!(list(&[(10, 10, 1811), (16, 16, 5433)]), Some(want));
        assert_eq!(list(&[(10, 10, 0)]), Some(vec![]));
        assert_eq!(list(&[(10, 11, 1811)]), None);
        assert_eq!(list(&[(10, 10, i32::MAX)]), None);
    }
}
