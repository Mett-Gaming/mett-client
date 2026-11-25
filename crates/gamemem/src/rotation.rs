//! Eigene Blickrichtung (nur lesen). Einstieg sind Klassennamen, die das Spiel für seine Bausteine selbst mitführt. Die
//! eigene Figur ist die einzige mit `LocalPlayerComponent`; nur ihr Wert wird geliefert. Nicht eindeutig heißt nichts.
use crate::Reader;

const PREFIX: &str = "entt::internal::pretty_function() [Type = ";
const NAMES: [&str; 2] = ["LocalPlayerComponent", "ActorRotationComponent"];
const LOCAL: usize = 0;
const TURN: usize = 1;
/// So viel eines Speichers wird durchgesehen.
const SPAN: usize = 0x80;
/// Ein Eintrag im Verzeichnis: nächster, Kennung, Speicher, Zähler.
const NODE: u64 = 32;
const MOST_NODES: u64 = 4096;
/// Neigung und Richtung, jetzt und im Bild davor.
const VALUE: usize = 16;
const MOST: usize = 100_000;

type Read<'a> = &'a (dyn Fn(u64, &mut [u8]) -> bool + Sync);

/// Beschreibung und Kennung je Name. Gilt für den laufenden Prozess.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Types {
    infos: [(u64, u32); 2],
    image: (u64, u64),
}

/// Wo die eigene Richtung steht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Own {
    types: Types,
    /// Speicher der eigenen Figur und der Richtungen.
    stores: [u64; 2],
    /// Je Spielstart abgeleitet: Beschreibung, Liste der Wesen und Seiten der Werte in einem Speicher.
    info: u64,
    list: u64,
    pages: u64,
}

fn long(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or_default()))
}

fn word(read: Read, at: u64) -> Option<u64> {
    let mut b = [0u8; 8];
    read(at, &mut b).then(|| u64::from_le_bytes(b))
}

/// Anfang und Anzahl, wenn dort drei Zeiger einer Liste stehen.
fn list(b: &[u8], o: usize, size: u64) -> Option<(u64, usize)> {
    let (from, to, end) = (long(b, o), long(b, o + 8), long(b, o + 16));
    let fits = from >= 0x10000 && from.is_multiple_of(8) && from < to && to <= end && (to - from).is_multiple_of(size);
    let n = fits.then(|| ((to - from) / size) as usize)?;
    (n <= MOST).then_some((from, n))
}

/// Namen im Programm, dann ihre Beschreibungen. Je Name genau eine, mit passender Kennung.
pub fn types(base: u64, image: &[u8]) -> Option<Types> {
    let mut infos = [(0, 0); 2];
    for (k, name) in NAMES.iter().enumerate() {
        let full = [PREFIX.as_bytes(), name.as_bytes(), b"]\0"].concat();
        let at = base + (once(image, &full)? + PREFIX.len()) as u64;
        let hash = name.bytes().fold(0x811C_9DC5u32, |h, c| (h ^ c as u32).wrapping_mul(0x0100_0193));
        let mut hits = (8..image.len().saturating_sub(16)).step_by(8).filter(|o| {
            long(image, *o) == at && long(image, o + 8) == name.len() as u64 && long(image, o - 8) >> 32 == hash as u64
        });
        let (first, None) = (hits.next()?, hits.next()) else { return None };
        infos[k] = (base + first as u64 - 8, hash);
    }
    Some(Types { infos, image: (base, base + image.len() as u64) })
}

/// Genau eine Fundstelle.
fn once(image: &[u8], what: &[u8]) -> Option<usize> {
    let mut found = None;
    let mut from = 0;
    while let Some(i) = image[from..].iter().position(|c| *c == what[0]) {
        let at = from + i;
        if image[at..].starts_with(what) {
            if found.is_some() {
                return None;
            }
            found = Some(at);
        }
        from = at + 1;
    }
    found
}

pub fn find(r: &Reader, t: &Types) -> Option<Own> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let spots = r.scan(&|at, b| {
        let words = b.as_chunks::<8>().0.iter().enumerate();
        let hit = |(i, w): (usize, &[u8; 8])| {
            let v = u64::from_le_bytes(*w);
            Some(((at + i * 8) as u64, t.infos.iter().position(|x| x.0 == v)?))
        };
        words.filter_map(hit).collect::<Vec<_>>()
    });
    if spots.len() > 256 {
        return None;
    }
    // Einträge im Verzeichnis: Kennung, dann ein Zeiger kurz vor eine Fundstelle.
    let nodes = r.scan(&|at, b| {
        let mut out = vec![];
        for (i, w) in b.as_chunks::<8>().0.windows(2).enumerate() {
            let id = u64::from_le_bytes(w[0]) as u32;
            let Some(k) = t.infos.iter().position(|x| x.1 == id) else { continue };
            let store = u64::from_le_bytes(w[1]);
            let before = |s: &&(u64, usize)| s.1 == k && s.0.wrapping_sub(store) < SPAN as u64;
            out.extend(spots.iter().find(before).map(|s| Node {
                at: (at + i * 8) as u64,
                kind: k,
                store,
                info: s.0 - store,
            }));
        }
        out
    });
    pair(read, t, &nodes)
}

#[derive(Clone, Copy, Debug)]
struct Node {
    /// Stelle der Kennung.
    at: u64,
    kind: usize,
    store: u64,
    info: u64,
}

fn pair(read: Read, t: &Types, nodes: &[Node]) -> Option<Own> {
    let mut found: Vec<Own> = vec![];
    for l in nodes.iter().filter(|n| n.kind == LOCAL) {
        for a in nodes.iter().filter(|n| n.kind == TURN && n.info == l.info) {
            if let Some(o) = learn(read, t, l, a).filter(|_| together(read, t, l.at, a.at))
                && !found.contains(&o)
            {
                found.push(o);
            }
        }
    }
    if let [one] = found[..] { Some(one) } else { None }
}

/// Beide Einträge stehen im selben Verzeichnis: dazwischen nur weitere Speicher.
fn together(read: Read, t: &Types, a: u64, b: u64) -> bool {
    let (from, to) = (a.min(b), a.max(b));
    if !(to - from).is_multiple_of(NODE) || (to - from) / NODE > MOST_NODES {
        return false;
    }
    (from..to).step_by(NODE as usize).all(|at| {
        let table = word(read, at + 8).and_then(|s| word(read, s));
        table.is_some_and(|v| (t.image.0..t.image.1).contains(&v))
    })
}

/// Leitet den Aufbau ab: vor der Beschreibung die Liste der Wesen, dahinter die Seiten der Werte.
fn learn(read: Read, t: &Types, l: &Node, a: &Node) -> Option<Own> {
    let (mut lb, mut ab) = ([0u8; SPAN], [0u8; SPAN]);
    if !read(l.store, &mut lb) || !read(a.store, &mut ab) {
        return None;
    }
    let mut fits = vec![];
    for o in (8..l.info.saturating_sub(16)).step_by(8) {
        let Some((from, 1)) = list(&lb, o as usize, 4) else { continue };
        let Some(me) = entity(read, from) else { continue };
        if place(read, &ab, o, me).is_some() {
            fits.push(o);
        }
    }
    let [ids] = fits[..] else { return None };
    let mut own = Own { types: *t, stores: [l.store, a.store], info: l.info, list: ids, pages: 0 };
    let mut pages = vec![];
    for o in (l.info + 8..SPAN as u64 - 24).step_by(8) {
        own.pages = o;
        if own.read_with(read).is_some() && shape(read, &ab, &own) {
            pages.push(o);
        }
    }
    let [one] = pages[..] else { return None };
    own.pages = one;
    Some(own)
}

fn entity(read: Read, at: u64) -> Option<u32> {
    let mut b = [0u8; 4];
    read(at, &mut b).then(|| u32::from_le_bytes(b))
}

/// Platz des Wesens in der Liste, und wie viele es sind.
fn place(read: Read, store: &[u8], o: u64, me: u32) -> Option<(usize, usize)> {
    let (from, n) = list(store, o as usize, 4)?;
    let mut b = vec![0u8; n * 4];
    if !read(from, &mut b) {
        return None;
    }
    let mut hits = b.as_chunks::<4>().0.iter().enumerate().filter(|(_, e)| u32::from_le_bytes(**e) == me);
    let (first, None) = (hits.next()?, hits.next()) else { return None };
    Some((first.0, n))
}

/// Stimmt die Größe eines Werts? Die meisten Wesen stehen gerade still, dann sind beide Hälften gleich. Nur verglichen,
/// nichts davon wird behalten.
fn shape(read: Read, store: &[u8], o: &Own) -> bool {
    let (Some((_, n)), Some((pages, _))) = (list(store, o.list as usize, 4), list(store, o.pages as usize, 8)) else {
        return false;
    };
    // Mit nur einem Wesen gibt es nichts zu verwechseln.
    if n == 1 {
        return true;
    }
    let n = n.min(64);
    let mut b = vec![0u8; n * VALUE];
    let Some(page) = word(read, pages) else { return false };
    read(page, &mut b) && b.as_chunks::<VALUE>().0.iter().filter(|v| v[..8] == v[8..]).count() * 2 > n
}

impl Own {
    /// Neigung und Richtung in Grad, wenn noch alles stimmt.
    pub fn read(&self, r: &Reader) -> Option<(f32, f32)> {
        self.read_with(&|at, out| r.read(at as usize, out).is_ok())
    }

    fn read_with(&self, read: Read) -> Option<(f32, f32)> {
        let mut stores = [[0u8; SPAN]; 2];
        for (k, b) in stores.iter_mut().enumerate() {
            let table = read(self.stores[k], b).then(|| long(b, 0))?;
            let ours = (self.types.image.0..self.types.image.1).contains(&table)
                && long(b, self.info as usize) == self.types.infos[k].0;
            if !ours {
                return None;
            }
        }
        let (from, 1) = list(&stores[LOCAL], self.list as usize, 4)? else { return None };
        let (at, n) = place(read, &stores[TURN], self.list, entity(read, from)?)?;
        let (pages, count) = list(&stores[TURN], self.pages as usize, 8)?;
        // Wie viele Werte auf eine Seite passen, ist nur von unten bekannt. Auf der ersten ist es egal.
        if at >= n.div_ceil(count).next_power_of_two() && count > 1 {
            return None;
        }
        let mut b = [0u8; VALUE];
        if !read(word(read, pages)?.checked_add((at * VALUE) as u64)?, &mut b) {
            return None;
        }
        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let sane = |p: f32, y: f32| p.abs() <= 90.5 && y.abs() <= 360.5 && (p == 0.0 || p.abs() > 1e-20);
        (sane(f(0), f(4)) && sane(f(8), f(12))).then_some((f(0), f(4)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Mem(HashMap<u64, Vec<u8>>);

    impl Mem {
        fn read(&self, at: u64, out: &mut [u8]) -> bool {
            for (start, b) in &self.0 {
                if let Some(o) = at.checked_sub(*start).map(|o| o as usize)
                    && o + out.len() <= b.len()
                {
                    out.copy_from_slice(&b[o..o + out.len()]);
                    return true;
                }
            }
            false
        }

        fn put(&mut self, at: u64, o: usize, v: u64) {
            self.0.get_mut(&at).unwrap()[o..o + 8].copy_from_slice(&v.to_le_bytes());
        }

        fn float(&mut self, at: u64, o: usize, v: f32) {
            self.0.get_mut(&at).unwrap()[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
    }

    const T: Types = Types { infos: [(0x50000, 11), (0x50100, 22)], image: (0x50000, 0x60000) };
    const D: u64 = 0x38;

    fn store(m: &mut Mem, at: u64, info: u64, ids: &[u32]) {
        m.0.insert(at, vec![0; SPAN]);
        m.put(at, 0, 0x51000);
        m.put(at, D as usize, info);
        let list = at + 0x1000;
        m.0.insert(list, ids.iter().flat_map(|i| i.to_le_bytes()).collect());
        for (i, v) in [list, list + ids.len() as u64 * 4, list + ids.len() as u64 * 4].into_iter().enumerate() {
            m.put(at, 0x20 + i * 8, v);
        }
    }

    /// Eigene Figur 77 an Platz 1 von dreien.
    fn world() -> (Mem, Vec<Node>) {
        let mut m = Mem(HashMap::new());
        store(&mut m, 0x100000, T.infos[0].0, &[77]);
        store(&mut m, 0x200000, T.infos[1].0, &[5, 77, 9]);
        m.0.insert(0x300000, vec![0; 8]);
        m.put(0x300000, 0, 0x400000);
        m.0.insert(0x400000, vec![0; 128 * VALUE]);
        for (i, v) in [0x300000, 0x300008, 0x300008].into_iter().enumerate() {
            m.put(0x200000, 0x50 + i * 8, v);
        }
        for (i, (p, y)) in [(1.0, 2.0), (-30.0, 90.0), (3.0, 4.0)].into_iter().enumerate() {
            for half in [0, 8] {
                m.float(0x400000, i * VALUE + half, p);
                m.float(0x400000, i * VALUE + half + 4, y);
            }
        }
        // Verzeichnis: Kennung bei 8, Speicher bei 16 in jedem Eintrag.
        m.0.insert(0x500000, vec![0; 96]);
        for (i, s) in [0x200000, 0x100000, 0x100000].into_iter().enumerate() {
            m.put(0x500000, i * 32 + 16, s);
        }
        let node = |at, kind, store| Node { at, kind, store, info: D };
        (m, vec![node(0x500008, TURN, 0x200000), node(0x500028, LOCAL, 0x100000)])
    }

    #[test]
    fn nur_die_eigene_richtung() {
        let (mut m, nodes) = world();
        let own = pair(&|a, o| m.read(a, o), &T, &nodes).unwrap();
        assert_eq!((own.list, own.pages), (0x20, 0x50));
        assert_eq!(own.read_with(&|a, o| m.read(a, o)), Some((-30.0, 90.0)));
        // Die Liste wird umsortiert: Es bleibt die eigene Figur.
        m.0.insert(0x201000, [77u32, 5, 9].iter().flat_map(|i| i.to_le_bytes()).collect());
        assert_eq!(own.read_with(&|a, o| m.read(a, o)), Some((1.0, 2.0)));
        // Keine eigene Figur mehr, etwa nach dem Verlassen der Welt.
        m.put(0x100000, 0x28, 0x101000);
        assert_eq!(own.read_with(&|a, o| m.read(a, o)), None);
    }

    #[test]
    fn anderes_wird_abgelehnt() {
        // Zwei verschiedene Verzeichnisse.
        let (m, mut nodes) = world();
        nodes[0].at += 0x100000;
        assert_eq!(pair(&|a, o| m.read(a, o), &T, &nodes), None);
        // Dazwischen liegt etwas anderes.
        let (mut m, mut nodes) = world();
        nodes[1].at = 0x500048;
        m.put(0x500000, 32 + 16, 0x400000);
        assert_eq!(pair(&|a, o| m.read(a, o), &T, &nodes), None);
        // Die Werte sind anders groß.
        let (mut m, nodes) = world();
        for i in [0, 2] {
            m.float(0x400000, i * VALUE + 8, 55.0);
        }
        assert_eq!(pair(&|a, o| m.read(a, o), &T, &nodes), None);
        // Unsinn statt Winkel.
        let (mut m, nodes) = world();
        let own = pair(&|a, o| m.read(a, o), &T, &nodes).unwrap();
        for bad in [f32::NAN, 400.0] {
            m.float(0x400000, VALUE + 4, bad);
            assert_eq!(own.read_with(&|a, o| m.read(a, o)), None);
        }
        // Der Speicher gehört jetzt jemand anderem.
        let (mut m, _) = world();
        m.put(0x200000, D as usize, 0x50200);
        assert_eq!(own.read_with(&|a, o| m.read(a, o)), None);
    }

    #[test]
    fn namen_mit_kennung() {
        let mut image = vec![0u8; 0x1000];
        let mut at = 0x100;
        for (k, n) in NAMES.iter().enumerate() {
            let full = [PREFIX.as_bytes(), n.as_bytes(), b"]\0"].concat();
            image[at..at + full.len()].copy_from_slice(&full);
            let hash = n.bytes().fold(0x811C_9DC5u32, |h, c| (h ^ c as u32).wrapping_mul(0x0100_0193));
            let o = 0x800 + k * 0x40;
            image[o + 4..o + 8].copy_from_slice(&hash.to_le_bytes());
            image[o + 8..o + 16].copy_from_slice(&(0x10000 + (at + PREFIX.len()) as u64).to_le_bytes());
            image[o + 16..o + 24].copy_from_slice(&(n.len() as u64).to_le_bytes());
            at += 0x100;
        }
        let t = types(0x10000, &image).unwrap();
        assert_eq!((t.infos[0].0, t.infos[1].0), (0x10800, 0x10840));
        // Die Kennung passt nicht zum Namen.
        image[0x804] ^= 1;
        assert_eq!(types(0x10000, &image), None);
    }
}
