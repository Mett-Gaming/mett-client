//! Oberfläche des Spiels (nur lesen). Einstieg sind die Namen der Elemente; wo in einem Element was steht, wird bei jedem
//! Spielstart aus den Zusammenhängen vieler Elemente abgeleitet. Nicht eindeutig heißt: keine Auskunft.
use crate::Reader;
use crate::uisizes::{self, Sample, Sizes};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub const PANEL: &str = "chat_panel";
pub const PARENT: &str = "chat_stack";
const ROOT: &str = "root_panel";
/// So viel wird je Element gelesen.
const SPAN: usize = 0x180;
/// Mindestens so viele Elemente müssen eine Aussage stützen.
const ENOUGH: usize = 20;
const MAX_CHILDREN: u64 = 0x8000;

/// Wo in einem Element was steht. Gilt nur für den laufenden Prozess.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// Funktionstabelle aller Elemente.
    pub table: u64,
    pub name: u64,
    /// Lage im Elternelement.
    pub place: u64,
    pub size: u64,
    pub flags: u64,
    /// Merker „Bildschirmlage neu ausrechnen“.
    pub bit: u32,
    pub parent: u64,
    pub children: u64,
    /// Zeiger in der ersten (0) oder zweiten (8) Hälfte eines Listeneintrags.
    pub half: u64,
    /// Deckkraft. Fehlt sie, geht nur das, was sie braucht, nicht.
    pub alpha: Option<u64>,
    /// Bausteine der Elemente. Fehlen sie, fehlt nur das rechtzeitige Nachrücken.
    pub parts: Option<Parts>,
    /// Gespeicherte Maße. Fehlen sie, bleibt die Breite die des Spiels.
    pub sizes: Option<Sizes>,
}

/// Wo die Liste der Bausteine steht, und wo ein Baustein sein Element nennt (mit festem Abstand dazu).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    pub list: u64,
    pub owner: u64,
    pub delta: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panel {
    pub at: u64,
    pub parent: u64,
    /// Größe der ganzen Oberfläche in ihren eigenen Einheiten.
    pub screen: (f32, f32),
    pub width: f32,
}

type Elements = HashMap<u64, Vec<u8>>;
type Read<'a> = &'a dyn Fn(u64, &mut [u8]) -> bool;

pub(crate) fn long(b: &[u8], o: u64) -> u64 {
    let o = o as usize;
    b.get(o..o + 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or_default()))
}

pub(crate) fn short(b: &[u8], o: u64) -> u32 {
    let o = o as usize;
    b.get(o..o + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap_or_default()))
}

pub(crate) fn pair(b: &[u8], o: u64) -> (f32, f32) {
    (f32::from_bits(short(b, o)), f32::from_bits(short(b, o + 4)))
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.6 && (a.1 - b.1).abs() < 0.6
}

/// Kurzer Name, wie er direkt im Element steht.
fn text(name: &str) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[..name.len()].copy_from_slice(name.as_bytes());
    b[16..24].copy_from_slice(&name.len().to_le_bytes());
    b[24..].copy_from_slice(&15usize.to_le_bytes());
    b
}

fn named(b: &[u8], o: u64, name: &str) -> bool {
    b.get(o as usize..o as usize + 32) == Some(&text(name)[..])
}

fn only<T>(mut it: impl Iterator<Item = T>) -> Option<T> {
    let first = it.next()?;
    it.next().is_none().then_some(first)
}

/// Adressen der drei bekannten Namen.
fn spots(r: &Reader) -> [Vec<u64>; 3] {
    let keys = [PANEL, PARENT, ROOT].map(text);
    let hits = r.scan(&|at, b| {
        let mut out = vec![];
        for o in (0..b.len().saturating_sub(31)).step_by(8) {
            if let Some(k) = keys.iter().position(|k| b[o..o + 32] == *k) {
                out.push((k, (at + o) as u64));
            }
        }
        out
    });
    std::array::from_fn(|k| hits.iter().filter(|h| h.0 == k).map(|h| h.1).collect())
}

/// Funktionstabelle und Abstand des Namens: nur, was für alle drei Namen gilt.
fn class(read: Read, spots: &[Vec<u64>; 3], image: &Range<u64>) -> Option<(u64, u64)> {
    let mut all: Option<HashSet<(u64, u64)>> = None;
    for list in spots {
        let mut here = HashSet::new();
        for at in list {
            for back in (8..=0x80).step_by(8) {
                let mut b = [0u8; 8];
                if read(at.wrapping_sub(back), &mut b) && image.contains(&u64::from_le_bytes(b)) {
                    here.insert((u64::from_le_bytes(b), back));
                }
            }
        }
        all = Some(match all {
            Some(a) => a.intersection(&here).copied().collect(),
            None => here,
        });
    }
    only(all?.into_iter())
}

fn elements(r: &Reader, table: u64) -> Elements {
    let found = r.scan(&|at, buf| {
        let words = buf.as_chunks::<8>().0.iter().enumerate();
        let hits = words.filter(|(_, w)| u64::from_le_bytes(**w) == table);
        hits.filter_map(|(i, _)| {
            let mut b = vec![0u8; SPAN];
            let at = at + i * 8;
            r.read(at, &mut b).is_ok().then_some((at as u64, b))
        })
        .collect()
    });
    found.into_iter().collect()
}

/// Vom Element nach oben.
fn chain(all: &Elements, parent: u64, at: u64) -> Vec<u64> {
    let mut out = vec![];
    let mut at = at;
    while let Some(b) = all.get(&at)
        && out.len() < 40
    {
        out.push(at);
        at = long(b, parent);
    }
    out
}

fn sum(all: &Elements, chain: &[u64], o: u64) -> (f32, f32) {
    chain.iter().map(|a| pair(&all[a], o)).fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1))
}

fn children(read: Read, b: &[u8], o: u64, half: u64) -> Option<Vec<u64>> {
    let (begin, end) = (long(b, o), long(b, o + 8));
    let size = end.checked_sub(begin).filter(|s| *s > 0 && *s <= MAX_CHILDREN && s % 16 == 0)?;
    let mut d = vec![0u8; size as usize];
    read(begin, &mut d).then(|| (0..size / 16).map(|i| long(&d, i * 16 + half)).collect())
}

/// Zeigerliste, deren Einträge alle eine Tabelle des Programms haben und auf ihr Element zurückverweisen. Fast jedes
/// Element hat eine; eine zweite, kürzere Liste haben nur manche.
fn parts(read: Read, tree: &[(Vec<u64>, &Vec<u8>)], image: &Range<u64>) -> Option<Parts> {
    let mut found = vec![];
    for o in (8..SPAN as u64 - 16).step_by(8) {
        let mut votes: HashMap<(u64, u64), usize> = HashMap::new();
        let mut lists = 0;
        for (c, b) in tree.iter().take(600) {
            let (begin, end) = (long(b, o), long(b, o + 8));
            let Some(size) = end.checked_sub(begin).filter(|s| *s > 0 && *s <= 64 && s % 8 == 0) else { continue };
            let mut d = vec![0u8; size as usize];
            if begin <= 0x10000 || !read(begin, &mut d) {
                continue;
            }
            let all: Vec<[u8; 0x20]> = (0..size / 8)
                .map_while(|i| {
                    let mut p = [0u8; 0x20];
                    (long(&d, i * 8) > 0x10000 && read(long(&d, i * 8), &mut p)).then_some(p)
                })
                .collect();
            if all.len() as u64 != size / 8 || all.iter().any(|p| !image.contains(&long(p, 0))) {
                continue;
            }
            lists += 1;
            for f in [8, 16, 24] {
                let delta = long(&all[0], f).wrapping_sub(c[0]);
                if delta < 0x40 && all.iter().all(|p| long(p, f).wrapping_sub(c[0]) == delta) {
                    *votes.entry((f, delta)).or_default() += 1;
                }
            }
        }
        found.extend(
            votes
                .into_iter()
                .filter(|(_, n)| *n >= ENOUGH && n * 10 >= lists * 9 && n * 5 >= tree.len().min(600) * 4)
                .map(|(v, _)| (o, v)),
        );
    }
    let (list, (owner, delta)) = only(found.into_iter())?;
    Some(Parts { list, owner, delta })
}

/// Leitet ab, wo was steht.
fn learn(read: Read, all: &Elements, image: &Range<u64>, table: u64, name: u64) -> Option<Layout> {
    let words = || (8..SPAN as u64 - 16).step_by(8);
    let panels: Vec<u64> = all.iter().filter(|(_, b)| named(b, name, PANEL)).map(|(a, _)| *a).collect();
    // Eltern: bei fast allen ein anderes Element, und die Namen passen.
    let parent = only(words().filter(|o| {
        let fits = all.iter().filter(|(a, b)| long(b, *o) != **a && all.contains_key(&long(b, *o))).count();
        let up = |a: u64, n: &str| {
            all.get(&a).map(|b| long(b, *o)).filter(|a| all.get(a).is_some_and(|b| named(b, name, n)))
        };
        fits * 5 > all.len() * 4 && panels.iter().any(|p| up(*p, PARENT).and_then(|s| up(s, ROOT)).is_some())
    }))?;
    let mut kids: HashMap<u64, HashSet<u64>> = HashMap::new();
    for (a, b) in all {
        kids.entry(long(b, parent)).or_default().insert(*a);
    }
    // Kinder: Einträge nennen dieses Element als Eltern.
    let mut lists = vec![];
    for o in words() {
        for half in [0, 8] {
            let (mut good, mut bad, mut owners) = (0, 0, 0);
            for (a, b) in all.iter().filter(|(a, _)| kids.contains_key(a)).take(2000) {
                let Some(list) = children(read, b, o, half) else { continue };
                let mine = list.iter().filter(|c| kids[a].contains(c)).count();
                good += mine;
                bad += list.len() - mine;
                owners += (mine > 0) as usize;
            }
            if owners >= ENOUGH && good > bad * 10 {
                lists.push((o, half));
            }
        }
    }
    let (children, half) = only(lists.into_iter())?;
    // Nur der Baum mit dem Chat: dort ist die Oberfläche gerade in Benutzung.
    // Freigegebene Elemente liegen noch eine Weile im Speicher, aber niemand führt sie mehr als Kind.
    let listed: HashSet<(u64, u64)> = all
        .iter()
        .filter_map(|(a, b)| Some((a, self::children(read, b, children, half)?)))
        .flat_map(|(a, list)| list.into_iter().map(|c| (*a, c)))
        .collect();
    let tops: HashSet<u64> = panels.iter().filter_map(|p| chain(all, parent, *p).last().copied()).collect();
    let tree: Vec<(Vec<u64>, &Vec<u8>)> = all
        .iter()
        .map(|(a, b)| (chain(all, parent, *a), b))
        .filter(|(c, _)| c.windows(2).all(|w| listed.contains(&(w[1], w[0]))))
        .filter(|(c, _)| c.len() > 1 && c.last().is_some_and(|t| tops.contains(t)))
        .collect();
    // Fertige Bildschirmlage = Summe der Lagen aller Eltern. Das Paar mit den meisten Treffern muss klar vorn liegen.
    let floats: Vec<u64> = (8..SPAN as u64 - 8).step_by(4).filter(|o| *o + 8 <= name || *o >= name + 32).collect();
    let mut scores = vec![];
    for rel in &floats {
        let sums: Vec<_> = tree
            .iter()
            .filter_map(|(c, b)| {
                let (s, own) = (sum(all, c, *rel), pair(b, *rel));
                let told = (s.0 - own.0).abs().min((s.1 - own.1).abs()) > 1.0 && s.0.abs().max(s.1.abs()) < 20_000.0;
                told.then_some((s, *b))
            })
            .collect();
        for abs in floats.iter().filter(|a| a.abs_diff(*rel) >= 8) {
            let hits = sums.iter().filter(|(s, b)| near(*s, pair(b, *abs))).count();
            if hits >= ENOUGH {
                scores.push((hits, *rel, *abs));
            }
        }
    }
    scores.sort_unstable_by(|a, b| b.cmp(a));
    let (hits, place, abs) = *scores.first()?;
    if scores.get(1).is_some_and(|s| s.0 * 2 > hits) {
        return None;
    }
    // Merker: leer stimmt die fertige Lage immer, und jede falsche trägt ihn.
    let told: Vec<(bool, &Vec<u8>)> = tree
        .iter()
        .filter_map(|(c, b)| {
            let s = sum(all, c, place);
            (!near(s, pair(b, place))).then_some((near(s, pair(b, abs)), *b))
        })
        .collect();
    let mut bits = vec![];
    for o in (8..SPAN as u64 - 4).step_by(4) {
        for bit in 0..32 {
            let count = |set: bool, fits: bool| {
                told.iter().filter(|(f, b)| *f == fits && (short(b, o) >> bit & 1 == 1) == set).count()
            };
            let (good, stale) = (count(false, true), count(true, false));
            // Während des Lesens darf sich einzelnes ändern. Gesetzt kann die alte Lage zufällig noch stimmen.
            let safe = count(false, false) * 20 <= good && count(true, true) * 2 <= stale;
            if good >= ENOUGH && stale >= ENOUGH && safe {
                bits.push((o, 1 << bit));
            }
        }
    }
    let (flags, bit) = only(bits.into_iter())?;
    // Größe: die Hülle und das Element darum sind gleich groß und füllen den Bildschirm.
    let hulls: Vec<(&Vec<u8>, &Vec<u8>)> = panels
        .iter()
        .filter_map(|p| {
            let c = chain(all, parent, *p);
            Some((&all[c.get(2)?], &all[c.get(3)?]))
        })
        .collect();
    let size = only(floats.iter().copied().filter(|o| {
        let fills = |p: (f32, f32)| p.0 > 200.0 && p.1 > 150.0 && p.0 < 10_000.0 && p.1 < 10_000.0;
        let usual = |(w, h): (f32, f32)| w > 10.0 && w < 10_000.0 && (0.0..10_000.0).contains(&h);
        *o != place
            && *o != abs
            && !hulls.is_empty()
            && hulls.iter().all(|(a, b)| fills(pair(a, *o)) && near(pair(a, *o), pair(b, *o)))
            && panels.iter().all(|p| usual(pair(&all[p], *o)))
    }))?;
    // Deckkraft: überall zwischen 0 und 1, meist genau 1, manchmal dazwischen.
    let alpha = only(floats.iter().copied().filter(|o| {
        let values: Vec<f32> = tree.iter().map(|(_, b)| f32::from_bits(short(b, *o))).collect();
        let valid = values.iter().all(|v| *v == 0.0 || (0.001..=1.0).contains(v));
        let full = values.iter().filter(|v| **v == 1.0).count();
        let partly = values.iter().filter(|v| **v > 0.0 && **v < 1.0).count();
        let apart = [place, size, abs].iter().all(|f| o.abs_diff(*f) >= 4 && *o != f + 4);
        apart && valid && full * 10 > values.len() * 7 && partly >= 3
    }));
    let parts = parts(read, &tree, image);
    let sizes = parts.and_then(|p| {
        let samples: Vec<Sample> = tree.iter().map(|(c, b)| Sample { at: c[0], bytes: b }).collect();
        uisizes::learn(read, &p, size, &samples)
    });
    Some(Layout { table, name, place, size, flags, bit, parent, children, half, alpha, parts, sizes })
}

/// Einmal je Spielstart, sobald es die Oberfläche einer Welt gibt.
pub fn layout(r: &Reader, image: &Range<u64>) -> Option<Layout> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let (table, name) = class(read, &spots(r), image)?;
    learn(read, &elements(r, table), image, table, name)
}

fn element(read: Read, l: &Layout, at: u64, name: &str) -> Option<Vec<u8>> {
    let mut b = vec![0u8; SPAN];
    let ok = at > 0x10000 && at.is_multiple_of(8) && read(at, &mut b);
    (ok && long(&b, 0) == l.table && named(&b, l.name, name)).then_some(b)
}

/// Eingehängt: Die Kette der Namen stimmt, und die Eltern führen das Element als Kind.
fn check(read: Read, l: &Layout, at: u64) -> Option<Panel> {
    let b = element(read, l, at, PANEL)?;
    let parent = long(&b, l.parent);
    let p = element(read, l, parent, PARENT)?;
    let root = element(read, l, long(&p, l.parent), ROOT)?;
    let holds = |b: &[u8], c: u64| children(read, b, l.children, l.half).is_some_and(|k| k.contains(&c));
    let screen = pair(&root, l.size);
    (holds(&p, at) && holds(&root, parent) && screen.1 > 100.0).then(|| Panel {
        at,
        parent,
        screen,
        width: pair(&b, l.size).0,
    })
}

impl Panel {
    /// Gilt die Fundstelle noch? Liefert auch eine geänderte Größe der Oberfläche.
    pub fn now(&self, r: &Reader, l: &Layout) -> Option<Panel> {
        check(&|at, out| r.read(at as usize, out).is_ok(), l, self.at).filter(|p| p.parent == self.parent)
    }
}

/// Ein gezeigtes Element mit kurzem Namen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    at: u64,
    name: [u8; 32],
}

/// Lage auf dem Bildschirm und Höhe der Oberfläche. Versteckte Elemente rechnet das Spiel nicht aus, ihr Merker bleibt
/// gesetzt; sie zählen nicht.
fn locate(read: Read, l: &Layout, at: u64, name: &[u8; 32]) -> Option<(f32, f32, f32)> {
    let mut sum = (0.0, 0.0);
    let mut here = at;
    for depth in 0..40 {
        let mut b = vec![0u8; SPAN];
        if here <= 0x10000 || !here.is_multiple_of(8) || !read(here, &mut b) || long(&b, 0) != l.table {
            return None;
        }
        if depth == 0
            && (b.get(l.name as usize..l.name as usize + 32) != Some(&name[..]) || short(&b, l.flags) & l.bit != 0)
        {
            return None;
        }
        // Nur was seine Eltern auch als Kind führen.
        let up = long(&b, l.parent);
        let p = pair(&b, l.place);
        sum = (sum.0 + p.0, sum.1 + p.1);
        if named(&b, l.name, ROOT) {
            let height = pair(&b, l.size).1;
            return (height > 100.0 && sum.0.is_finite() && sum.1.is_finite()).then_some((sum.0, sum.1, height));
        }
        let mut pb = vec![0u8; SPAN];
        if !read(up, &mut pb) || !children(read, &pb, l.children, l.half)?.contains(&here) {
            return None;
        }
        here = up;
    }
    None
}

impl Spot {
    /// Lage jetzt; `None`, wenn es das Element nicht mehr gibt oder es gerade versteckt ist.
    pub fn now(&self, r: &Reader, l: &Layout) -> Option<(f32, f32, f32)> {
        locate(&|at, out| r.read(at as usize, out).is_ok(), l, self.at, &self.name)
    }
}

/// Das eine gezeigte Element dieses Namens.
pub fn spot(r: &Reader, l: &Layout, name: &str) -> Option<Spot> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let key = text(name);
    let spots = r.scan(&|at, b| {
        let o = (0..b.len().saturating_sub(31)).step_by(8);
        o.filter(|o| b[*o..*o + 32] == key).map(|o| (at + o) as u64).collect()
    });
    let shown = |s: &u64| s.checked_sub(l.name).filter(|at| locate(read, l, *at, &key).is_some());
    only(spots.iter().filter_map(shown)).map(|at| Spot { at, name: key })
}

/// Minecrafts Fadenkreuz.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crosshair {
    pub at: u64,
    pub parent: u64,
}

const CROSS: &str = "cursor";
const CROSS_SCREEN: &str = "hud_crosshair_screen";

/// Kurzer Name; hinter seinem Ende können Reste stehen.
fn is_cross(b: &[u8]) -> bool {
    b.starts_with(CROSS.as_bytes()) && b[CROSS.len()] == 0 && long(b, 16) == CROSS.len() as u64 && long(b, 24) == 15
}

/// Das Element liegt unmittelbar in seinem Bildschirm, und der führt es als Kind.
fn cross(read: Read, l: &Layout, at: u64) -> Option<Crosshair> {
    let mut b = vec![0u8; SPAN];
    if at <= 0x10000 || !at.is_multiple_of(8) || !read(at, &mut b) || long(&b, 0) != l.table {
        return None;
    }
    let parent = long(&b, l.parent);
    let mut p = vec![0u8; SPAN];
    let name = b.get(l.name as usize..l.name as usize + 32);
    if !name.is_some_and(is_cross) || !read(parent, &mut p) || long(&p, 0) != l.table {
        return None;
    }
    // Lange Namen stehen außerhalb.
    let mut name = vec![0u8; CROSS_SCREEN.len()];
    let right = long(&p, l.name + 16) == name.len() as u64
        && read(long(&p, l.name), &mut name)
        && name == CROSS_SCREEN.as_bytes();
    let child = right && children(read, &p, l.children, l.half)?.contains(&at);
    child.then_some(Crosshair { at, parent })
}

impl Crosshair {
    pub fn alive(&self, r: &Reader, l: &Layout) -> bool {
        cross(&|at, out| r.read(at as usize, out).is_ok(), l, self.at) == Some(*self)
    }
}

/// Merker „selbst sichtbar“ und „samt Eltern sichtbar“.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Visible {
    pub own: u32,
    pub all: u32,
}

/// Aus allen Elementen gelernt: Ein Element ist samt Eltern sichtbar, wenn es selbst und sein Elternteil es sind, und
/// alles Ausgerechnete ist sichtbar. Passt mehr als ein Paar, gilt keines.
pub fn visible(r: &Reader, l: &Layout) -> Option<Visible> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let found = r.scan(&|at, b| {
        let table = l.table.to_le_bytes();
        let o = (0..b.len().saturating_sub(SPAN)).step_by(8);
        o.filter(|o| b[*o..*o + 8] == table).map(|o| ((at + o) as u64, b[o..o + SPAN].to_vec())).collect()
    });
    let all: HashMap<u64, Vec<u8>> = found.into_iter().collect();
    let mut lists: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut pairs = vec![];
    for (at, b) in &all {
        let up = long(b, l.parent);
        let Some(p) = all.get(&up) else { continue };
        // Nur was die Eltern auch als Kind führen.
        let list = lists.entry(up).or_insert_with(|| children(read, p, l.children, l.half).unwrap_or_default());
        if list.contains(at) {
            pairs.push((short(p, l.flags), short(b, l.flags)));
        }
    }
    learn_visible(&pairs, l.bit)
}

/// Merker von Elternteil und Kind; `dirty`: noch nicht ausgerechnet.
fn learn_visible(pairs: &[(u32, u32)], dirty: u32) -> Option<Visible> {
    let clean = pairs.iter().filter(|(p, c)| (p | c) & dirty == 0);
    let shown = clean.clone().fold(!dirty, |m, (p, c)| m & p & c);
    let bits: Vec<u32> = (0..32).map(|i| 1 << i).filter(|b| shown & b != 0).collect();
    let mut fits = vec![];
    for own in &bits {
        for all in bits.iter().filter(|a| *a != own) {
            let is = |f: u32, b: &u32| f & b != 0;
            // Alle drei Fälle müssen oft genug vorkommen: gezeigt, selbst versteckt, mit den Eltern versteckt.
            let mut seen = [0usize; 3];
            let mut wrong = 0;
            for (p, c) in pairs {
                let (up, me, whole) = (is(*p, all), is(*c, own), is(*c, all));
                match (up && me) == whole {
                    true if whole => seen[0] += 1,
                    true if up => seen[1] += 1,
                    true if me => seen[2] += 1,
                    true => {}
                    false => wrong += 1,
                }
            }
            if seen.iter().all(|n| *n >= ENOUGH) && wrong * 100 <= pairs.len() {
                fits.push(Visible { own: *own, all: *all });
            }
        }
    }
    only(fits.into_iter()).filter(|_| clean.count() >= ENOUGH)
}

/// Das eine Fadenkreuz der offenen Welt.
pub fn crosshair(r: &Reader, l: &Layout) -> Option<Crosshair> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let spots = r.scan(&|at, b| {
        let o = (0..b.len().saturating_sub(31)).step_by(8);
        o.filter(|o| is_cross(&b[*o..*o + 32])).map(|o| (at + o) as u64).collect()
    });
    only(spots.into_iter().filter_map(|s| cross(read, l, s.checked_sub(l.name)?)))
}

/// Minecrafts Chat-Anzeige in der offenen Welt.
pub fn panel(r: &Reader, l: &Layout) -> Option<Panel> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let key = text(PANEL);
    let spots = r.scan(&|at, b| {
        let o = (0..b.len().saturating_sub(31)).step_by(8);
        o.filter(|o| b[*o..*o + 32] == key).map(|o| (at + o) as u64).collect()
    });
    only(spots.into_iter().filter_map(|s| check(read, l, s.checked_sub(l.name)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: u64 = 0x7000_0000;

    /// Nachgebaute Oberfläche: Hülle, Chat und 60 verschachtelte Elemente.
    struct Game {
        mem: HashMap<u64, Vec<u8>>,
    }

    fn floats(p: (f32, f32)) -> Vec<u8> {
        [p.0.to_le_bytes(), p.1.to_le_bytes()].concat()
    }

    impl Game {
        fn put(&mut self, at: u64, o: u64, v: &[u8]) {
            self.mem.get_mut(&at).unwrap()[o as usize..o as usize + v.len()].copy_from_slice(v);
        }

        fn new() -> Self {
            let mut g = Game { mem: HashMap::new() };
            // (Adresse, Name, Eltern, Lage)
            let mut list = vec![
                (0x10_0000, "hud_screen".to_string(), 0, (0.0, 0.0)),
                (0x10_1000, "hull".into(), 0x10_0000, (0.0, 0.0)),
                (0x10_2000, ROOT.into(), 0x10_1000, (0.0, 0.0)),
                (0x10_3000, PARENT.into(), 0x10_2000, (5.0, 7.0)),
                (0x10_4000, PANEL.into(), 0x10_3000, (0.0, 62.0)),
            ];
            let mut last = 0x10_4000;
            for i in 0..60u64 {
                let at = 0x20_0000 + i * 0x1000;
                list.push((at, format!("e{i}"), last, (2.0 + i as f32, 3.0 + (i % 7) as f32 * 2.0)));
                last = if i % 2 == 1 { 0x10_4000 } else { at };
            }
            let place: HashMap<u64, (u64, (f32, f32))> = list.iter().map(|e| (e.0, (e.2, e.3))).collect();
            for (i, (at, name, parent, rel)) in list.iter().enumerate() {
                g.mem.insert(*at, vec![0; SPAN]);
                g.put(*at, 0, &TABLE.to_le_bytes());
                let mut abs = (0.0, 0.0);
                let mut up = *at;
                while let Some((p, r)) = place.get(&up) {
                    abs = (abs.0 + r.0, abs.1 + r.1);
                    up = *p;
                }
                let stale = i > 5 && i % 5 < 2;
                if !stale {
                    g.put(*at, 0x20, &floats(abs));
                }
                g.put(*at, 0x28, &(0x67cu32 | stale as u32).to_le_bytes());
                g.put(*at, 0x30, &text(name));
                g.put(*at, 0x50, &floats(*rel));
                g.put(*at, 0x58, &floats(if i < 4 { (854.0, 480.0) } else { (341.6, 22.0) }));
                g.put(*at, 0x60, &floats((-32768.0, -32768.0)));
                g.put(*at, 0x68, &floats(if name == PANEL { (341.6, 240.0) } else { (32767.0, 32767.0) }));
                g.put(*at, 0x70, &(if i % 9 == 8 { 0.5f32 } else { 1.0 }).to_le_bytes());
                g.put(*at, 0xA0, &parent.to_le_bytes());
                // Ein Baustein je Element.
                g.mem.insert(at + 0x900, (at + 0xA00).to_le_bytes().to_vec());
                let mut part = vec![0u8; 0x20];
                part[..8].copy_from_slice(&(TABLE + 0x40).to_le_bytes());
                part[8..16].copy_from_slice(&(at + 0x10).to_le_bytes());
                g.mem.insert(at + 0xA00, part);
                g.put(*at, 0xC8, &(at + 0x900).to_le_bytes());
                g.put(*at, 0xD0, &(at + 0x908).to_le_bytes());
                let kids: Vec<u64> = list.iter().filter(|c| c.2 == *at).flat_map(|c| [c.0 + 16, c.0]).collect();
                if !kids.is_empty() {
                    let l = at + 0x800;
                    let bytes: Vec<u8> = kids.iter().flat_map(|k| k.to_le_bytes()).collect();
                    g.put(*at, 0xA8, &l.to_le_bytes());
                    g.put(*at, 0xB0, &(l + bytes.len() as u64).to_le_bytes());
                    g.mem.insert(l, bytes);
                }
            }
            g
        }

        fn read(&self, at: u64, out: &mut [u8]) -> bool {
            let Some(b) = self.mem.get(&at).and_then(|b| b.get(..out.len())) else { return false };
            out.copy_from_slice(b);
            true
        }

        fn learn(&self) -> Option<Layout> {
            let all = self.mem.iter().filter(|(_, b)| b.len() == SPAN && long(b, 0) == TABLE);
            let image = TABLE..TABLE + 0x1000;
            learn(&|a, o| self.read(a, o), &all.map(|(a, b)| (*a, b.clone())).collect(), &image, TABLE, 0x30)
        }
    }

    const EXPECTED: Layout = Layout {
        table: TABLE,
        name: 0x30,
        place: 0x50,
        size: 0x58,
        flags: 0x28,
        bit: 1,
        parent: 0xA0,
        children: 0xA8,
        half: 8,
        alpha: Some(0x70),
        parts: Some(Parts { list: 0xC8, owner: 8, delta: 0x10 }),
        sizes: None,
    };

    #[test]
    fn leitet_den_aufbau_ab() {
        assert_eq!(Game::new().learn(), Some(EXPECTED));
    }

    #[test]
    fn zweideutig_heisst_nichts() {
        // Zweites Elternfeld, zweiter Merker, zweite fertige Lage, kein Chat.
        for what in 0..4 {
            let mut g = Game::new();
            let all: Vec<u64> = g.mem.iter().filter(|(_, b)| b.len() == SPAN).map(|(a, _)| *a).collect();
            for a in all {
                let b = g.mem[&a].clone();
                match what {
                    0 => g.put(a, 0x100, &b[0xA0..0xA8]),
                    1 => g.put(a, 0x110, &b[0x28..0x2C]),
                    2 => g.put(a, 0x120, &b[0x20..0x28]),
                    _ if named(&b, 0x30, PANEL) => g.put(a, 0x30, &text("other")),
                    _ => {}
                }
            }
            assert_eq!(g.learn(), None, "{what}");
        }
    }

    #[test]
    fn freigegebene_zeilen_stoeren_nicht() {
        let mut g = Game::new();
        for i in 0..40u64 {
            let at = 0x90_0000 + i * 0x1000;
            g.mem.insert(at, g.mem[&0x20_0000].clone());
            g.put(at, 0x20, &floats((700.0 + i as f32, 5.0)));
            g.put(at, 0x28, &0x67cu32.to_le_bytes());
            g.put(at, 0xA8, &[0; 16]);
        }
        assert_eq!(g.learn(), Some(EXPECTED));
    }

    #[test]
    fn ohne_klare_bausteine_bleibt_der_rest() {
        let mut g = Game::new();
        // Die Bausteine verweisen nicht auf ihr Element.
        g.mem.iter_mut().filter(|(_, b)| b.len() == 0x20).for_each(|(_, b)| b[8] ^= 0x80);
        g.mem.iter_mut().filter(|(_, b)| b.len() == 0x20).for_each(|(_, b)| b[10] ^= 0x80);
        assert_eq!(g.learn(), Some(Layout { parts: None, ..EXPECTED }));
    }

    #[test]
    fn ohne_klare_deckkraft_bleibt_der_rest() {
        let mut g = Game::new();
        let all: Vec<u64> = g.mem.iter().filter(|(_, b)| b.len() == SPAN).map(|(a, _)| *a).collect();
        for a in all {
            let b = g.mem[&a].clone();
            g.put(a, 0x120, &b[0x70..0x74]);
        }
        assert_eq!(g.learn(), Some(Layout { alpha: None, ..EXPECTED }));
    }

    #[test]
    fn nur_eingehaengt_zaehlt() {
        let found = |g: &Game| check(&|a, o| g.read(a, o), &EXPECTED, 0x10_4000);
        let panel = Panel { at: 0x10_4000, parent: 0x10_3000, screen: (854.0, 480.0), width: 341.6 };
        assert_eq!(found(&Game::new()), Some(panel));
        for (at, o) in [(0x10_4000, 0), (0x10_4000, 0x30), (0x10_4000, 0xA0), (0x10_3000, 0xA8), (0x10_2000, 0x30)] {
            let mut g = Game::new();
            g.put(at, o, &[0xEE]);
            assert_eq!(found(&g), None, "{at:x} {o:x}");
        }
    }

    #[test]
    fn sichtbarkeit_wird_aus_der_regel_gelernt() {
        // Zeigen: 4 und 16; 1 ist „noch nicht ausgerechnet“, 8 ist immer gesetzt.
        let mut pairs = vec![];
        for _ in 0..50 {
            pairs.extend([(0x1C, 0x1C), (0x1C, 0x09), (0x09, 0x0D), (0x0D, 0x0D), (0x1C, 0x1D)]);
        }
        assert_eq!(learn_visible(&pairs, 1), Some(Visible { own: 4, all: 16 }));
        // Ein paar Ausreißer während einer Umstellung schaden nicht.
        pairs.push((0x1C, 0x0D));
        assert_eq!(learn_visible(&pairs, 1), Some(Visible { own: 4, all: 16 }));
        // Ohne versteckte Elemente lässt sich nichts unterscheiden.
        assert_eq!(learn_visible(&[(0x1C, 0x1C); 100], 1), None);
        // Zwei gleichwertige Merker: lieber nichts.
        let double: Vec<_> = pairs.iter().map(|(p, c)| (p | (p & 4) << 4, c | (c & 4) << 4)).collect();
        assert_eq!(learn_visible(&double, 1), None);
        assert_eq!(learn_visible(&[], 1), None);
    }

    #[test]
    fn fadenkreuz_name_mit_resten() {
        let mut b = text(CROSS);
        assert!(is_cross(&b));
        b[7..16].fill(b'x');
        assert!(is_cross(&b));
        b[6] = b'x';
        assert!(!is_cross(&b));
        assert!(!is_cross(&text("cursors")));
    }

    #[test]
    fn gezeigtes_element_mit_lage() {
        let read = |g: &Game, name: &str| {
            let at = g.mem.iter().find(|(_, b)| b.len() == SPAN && named(b, 0x30, name)).map(|(a, _)| *a).unwrap();
            locate(&|a, o| g.read(a, o), &EXPECTED, at, &text(name))
        };
        let mut g = Game::new();
        // Chat bei (0, 62) in der Spalte bei (5, 7).
        assert_eq!(read(&g, PANEL), Some((5.0, 69.0, 480.0)));
        // Versteckt, ausgehängt.
        g.put(0x10_4000, 0x28, &0x67du32.to_le_bytes());
        assert_eq!(read(&g, PANEL), None);
        let mut g = Game::new();
        g.put(0x10_3000, 0xA8, &[0; 16]);
        assert_eq!(read(&g, PANEL), None);
    }

    #[test]
    fn klasse_gilt_fuer_alle_namen() {
        let mut mem: HashMap<u64, u64> = [0x1000, 0x2000, 0x3000].map(|a| (a, TABLE)).into();
        // Nur vor einem der Namen steht noch ein anderer Verweis.
        mem.insert(0x1008, TABLE + 8);
        let read = |at: u64, out: &mut [u8]| {
            mem.get(&at).is_some_and(|v| {
                out.copy_from_slice(&v.to_le_bytes());
                true
            })
        };
        let image = TABLE..TABLE + 0x1000;
        let spots = [vec![0x1030], vec![0x2030, 0x9000], vec![0x3030]];
        assert_eq!(class(&read, &spots, &image), Some((TABLE, 0x30)));
        assert_eq!(class(&read, &[vec![0x1030], vec![], vec![0x3030]], &image), None);
    }
}
