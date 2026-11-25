//! Namen aus Minecrafts Spielerliste (nur lesen), für das Tab-Menü. Die Liste hat keinen lesbaren Namen; erkannt wird sie
//! an ihrer Form: ein geschlossener Ring, jeder Eintrag mit doppelter Kennung, Name und Xbox-Kennung. Der eigene Eintrag
//! taugt nicht als Einstieg, auf Servern steht er nicht immer darin. Alles außer den Namen bleibt liegen.
use crate::Reader;
use std::collections::HashSet;

/// Eintrag: nächster, voriger, Kennung (16), Nummer, dieselbe Kennung, Name, Xbox-Kennung.
const KEY: usize = 0x10;
const SAME: usize = 0x28;
const NAME: usize = 0x38;
const XUID: usize = 0x58;
const SPAN: usize = 0x78;
/// Mehr als ein Server je in der Liste hat.
const MOST: usize = 2000;
const LONGEST: usize = 256;
/// Hinter den Namen hängt der Skin: gezählter Zeiger, dessen Zähler 16 Byte hinter dem Objekt steht.
const SKIN: std::ops::Range<u64> = 0x78..0x180;
const SKIN_SPAN: usize = 0x200;
const RGBA: u32 = 4;
pub const FACE: usize = 8;

/// Gesicht, 8 × 8 Punkte, Rot, Grün, Blau, Deckkraft.
pub type Face = Vec<u8>;

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub name: String,
    pub face: Option<Face>,
}

type Read<'a> = &'a (dyn Fn(u64, &mut [u8]) -> bool + Sync);

fn word(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or_default()))
}

/// Text an dieser Stelle eines gelesenen Eintrags, kurz im Objekt selbst oder lang über seinen Zeiger.
fn text(read: Read, b: &[u8], o: usize) -> Option<String> {
    let (len, cap) = (word(b, o + 16) as usize, word(b, o + 24) as usize);
    if len > LONGEST || cap < len || !(15..=4096).contains(&cap) {
        return None;
    }
    let mut t = vec![0u8; len];
    if cap == 15 {
        t.copy_from_slice(b.get(o..o + len)?);
    } else if !read(word(b, o), &mut t) {
        return None;
    }
    String::from_utf8(t).ok()
}

/// Sieht das wie ein Eintrag aus? Dann sein Name.
fn entry(read: Read, b: &[u8]) -> Option<String> {
    let key = b.get(KEY..KEY + 16)?;
    let pointers = [0, 8].iter().all(|o| word(b, *o) > 0x10000 && word(b, *o).is_multiple_of(8));
    if !pointers || key != &b[SAME..SAME + 16] || key.iter().all(|x| *x == 0) {
        return None;
    }
    let (name, xuid) = (text(read, b, NAME)?, text(read, b, XUID)?);
    let fine = !name.is_empty() && !name.chars().any(char::is_control) && xuid.bytes().all(|c| c.is_ascii_digit());
    fine.then_some(name)
}

fn short(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap_or_default()))
}

/// Bild im Skin: Format, Breite, Höhe, dahinter Zeiger und passende Größe. Das erste ist der Skin selbst.
fn picture(b: &[u8]) -> Option<(usize, u64)> {
    (0..b.len().saturating_sub(0x30)).step_by(8).find_map(|o| {
        let (w, h) = (short(b, o + 4), short(b, o + 8));
        let shape = [64, 128, 256, 512].contains(&w) && (h == w || h * 2 == w);
        let (at, size) = (word(b, o + 0x20), word(b, o + 0x28));
        let fits = shape && short(b, o) == RGBA && size == (w * h * 4) as u64 && at > 0x10000;
        fits.then_some((w as usize, at))
    })
}

/// Gesicht mit Kopfbedeckung darüber, wie Java es in der Liste zeigt.
fn face(read: Read, node: u64) -> Option<Face> {
    let (width, pixels) = SKIN.step_by(8).find_map(|o| {
        let mut p = [0u8; 16];
        let (count, skin) = read(node + o, &mut p).then(|| (word(&p, 0), word(&p, 8)))?;
        if skin <= 0x10000 || !skin.is_multiple_of(8) || count != skin + 0x10 {
            return None;
        }
        let mut b = [0u8; SKIN_SPAN];
        read(skin, &mut b).then(|| picture(&b)).flatten()
    })?;
    let step = width / 64;
    let mut out = Vec::with_capacity(FACE * FACE * 4);
    for y in 0..FACE {
        // Eine Bildzeile reicht: Gesicht ab 8, Kopfbedeckung ab 40.
        let mut row = vec![0u8; width * 4];
        if !read(pixels + (((8 + y) * step + step / 2) * width * 4) as u64, &mut row) {
            return None;
        }
        for x in 0..FACE {
            let dot = |from: usize| {
                let at = ((from + x) * step + step / 2) * 4;
                [row[at], row[at + 1], row[at + 2], row[at + 3]]
            };
            let (base, hat) = (dot(8), dot(40));
            let c = if hat[3] > 127 { hat } else { base };
            out.extend([c[0], c[1], c[2], if c[3] > 127 { 255 } else { 0 }]);
        }
    }
    // Ganz leer heißt: kein Bild da.
    out.as_chunks::<4>().0.iter().any(|c| c[3] > 0).then_some(out)
}

/// Anfänge aller Spielerlisten. Im Einzelspieler gibt es zwei, Server und Client.
pub fn heads(r: &Reader) -> Vec<u64> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let found = r.scan(&|at, buf| {
        let mut out = vec![];
        for o in (0..buf.len().saturating_sub(SPAN)).step_by(8) {
            let b = &buf[o..o + SPAN];
            // Billig zuerst: dieselbe Kennung zweimal.
            if b[KEY..KEY + 16] == b[SAME..SAME + 16] && word(b, KEY) != 0 && entry(read, b).is_some() {
                out.push((at + o) as u64);
            }
        }
        out
    });
    let mut heads: Vec<u64> = found.iter().filter_map(|e| head(read, *e)).collect();
    heads.sort_unstable();
    heads.dedup();
    heads
}

/// Vom Eintrag zum leeren Anfang seiner Liste.
fn head(read: Read, from: u64) -> Option<u64> {
    let mut at = from;
    for _ in 0..MOST {
        let mut b = [0u8; SPAN];
        if !read(at, &mut b) || entry(read, &b).is_none() {
            // Der Anfang muss wirklich zurück zu dieser Liste führen.
            return list(read, at).map(|_| at);
        }
        at = word(&b, 0);
    }
    None
}

/// Alle Einträge der Liste mit diesem Anfang: Name und Adresse. `None`: Die Liste gibt es nicht mehr.
fn list(read: Read, head: u64) -> Option<Vec<(String, u64)>> {
    let mut names = vec![];
    let mut seen = HashSet::new();
    let mut at = head;
    loop {
        // Ein Kreis, der nicht beim Anfang endet, ist keiner.
        if seen.len() > MOST || !seen.insert(at) {
            return None;
        }
        let mut b = [0u8; SPAN];
        let mut links = [0u8; 16];
        if !read(at, &mut links) {
            return None;
        }
        // Genau der Anfang ist kein Spieler.
        let player = read(at, &mut b).then(|| entry(read, &b)).flatten();
        match player {
            Some(name) if at != head => names.push((name, at)),
            None if at == head => {}
            _ => return None,
        }
        let next = word(&links, 0);
        let mut back = [0u8; 16];
        if next <= 0x10000 || !next.is_multiple_of(8) || !read(next, &mut back) || word(&back, 8) != at {
            return None;
        }
        if next == head {
            return (!names.is_empty()).then_some(names);
        }
        at = next;
    }
}

/// Spieler, wie sie gerade in den Listen stehen. Mehrere Listen müssen dieselben Namen haben.
pub fn players(r: &Reader, heads: &[u64]) -> Option<Vec<Player>> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let lists: Vec<Option<Vec<(String, u64)>>> = heads.iter().map(|h| list(read, *h)).collect();
    // Eine verschwundene Liste heißt: andere Welt.
    let first = lists.iter().all(Option::is_some).then(|| agree(lists.into_iter().flatten())).flatten()?;
    Some(first.into_iter().map(|(name, at)| Player { name, face: face(read, at) }).collect())
}

fn agree(lists: impl Iterator<Item = Vec<(String, u64)>>) -> Option<Vec<(String, u64)>> {
    let mut lists = lists.map(|mut l| {
        l.sort();
        l
    });
    let first = lists.next()?;
    let same = |l: Vec<(String, u64)>| l.iter().map(|e| &e.0).eq(first.iter().map(|e| &e.0));
    lists.all(same).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const ME: &str = "2535450000000001";

    struct Game(HashMap<u64, Vec<u8>>);

    impl Game {
        fn put(&mut self, at: u64, o: usize, v: &[u8]) {
            self.0.get_mut(&at).unwrap()[o..o + v.len()].copy_from_slice(v);
        }

        fn text(&mut self, at: u64, o: usize, t: &str) {
            if t.len() <= 15 {
                self.put(at, o, t.as_bytes());
            } else {
                let heap = at * 16 + o as u64;
                self.0.insert(heap, t.as_bytes().to_vec());
                self.put(at, o, &heap.to_le_bytes());
            }
            self.put(at, o + 16, &t.len().to_le_bytes());
            self.put(at, o + 24, &(if t.len() <= 15 { 15usize } else { 31 }).to_le_bytes());
        }

        /// Anfang bei 0x20000, dann je Spieler ein Knoten.
        fn new(names: &[(&str, &str)]) -> Self {
            let mut g = Game(HashMap::new());
            let nodes: Vec<u64> = (0..=names.len() as u64).map(|i| 0x20000 + i * 0x1000).collect();
            for (i, at) in nodes.iter().enumerate() {
                g.0.insert(*at, vec![0; SPAN]);
                let n = nodes.len();
                g.put(*at, 0, &nodes[(i + 1) % n].to_le_bytes());
                g.put(*at, 8, &nodes[(i + n - 1) % n].to_le_bytes());
                if i > 0 {
                    for o in [KEY, SAME] {
                        g.put(*at, o, &[i as u8; 16]);
                    }
                    g.text(*at, NAME, names[i - 1].0);
                    g.text(*at, XUID, names[i - 1].1);
                }
            }
            g
        }

        fn run<T>(&self, f: impl Fn(Read) -> T) -> T {
            let read = |at: u64, out: &mut [u8]| {
                let Some(b) = self.0.get(&at).and_then(|b| b.get(..out.len())) else { return false };
                out.copy_from_slice(b);
                true
            };
            f(&read)
        }
    }

    const PLAYERS: [(&str, &str); 3] = [("Anna", "1"), ("Ich", ME), ("§aEin langer Name Grüße", "")];

    #[test]
    fn liest_die_ganze_liste() {
        let g = Game::new(&PLAYERS);
        // Von jedem Eintrag führt der Weg zum selben Anfang.
        for at in [0x21000, 0x22000, 0x23000] {
            assert_eq!(g.run(|r| head(r, at)), Some(0x20000));
        }
        let mut names: Vec<String> = g.run(|r| list(r, 0x20000)).unwrap().into_iter().map(|e| e.0).collect();
        names.sort();
        assert_eq!(names, ["Anna", "Ich", "§aEin langer Name Grüße"]);
        assert_eq!(Game::new(&[("Ich", ME)]).run(|r| list(r, 0x20000)), Some(vec![("Ich".into(), 0x21000)]));
    }

    #[test]
    fn kaputtes_gilt_nicht() {
        // Ein Spieler ist kein Anfang; falscher Rückweg, kaputter Name, zwei leere Knoten, Kreis ohne Anfang.
        assert_eq!(Game::new(&PLAYERS).run(|r| list(r, 0x21000)), None);
        for (at, o, v) in
            [(0x23000, 8, 0x21000u64), (0x23000, NAME + 16, 9999), (0x23000, KEY, 77), (0x22000, 0, 0x22000)]
        {
            let mut g = Game::new(&PLAYERS);
            g.put(at, o, &v.to_le_bytes());
            assert_eq!(g.run(|r| list(r, 0x20000)), None, "{at:x} {o:x}");
            assert_eq!(g.run(|r| head(r, 0x21000)), None, "{at:x} {o:x}");
        }
        assert_eq!(Game::new(&[]).run(|r| list(r, 0x20000)), None);
    }

    #[test]
    fn zwei_listen_muessen_gleich_sein() {
        let l = |n: &[&str], at: u64| n.iter().map(|s| (s.to_string(), at)).collect::<Vec<_>>();
        assert_eq!(agree([l(&["b", "a"], 1), l(&["a", "b"], 2)].into_iter()), Some(l(&["a", "b"], 1)));
        assert_eq!(agree([l(&["a"], 1), l(&["a", "b"], 2)].into_iter()), None);
        assert_eq!(agree(std::iter::empty()), None);
    }

    /// Knoten bei 0x11000, Skin bei 0x25000, Bild bei 0x100000.
    fn skin(width: usize, paint: impl Fn(usize, usize) -> [u8; 4]) -> HashMap<u64, Vec<u8>> {
        let mut node = vec![0u8; 0x200];
        node[0xC0..0xC8].copy_from_slice(&0x2_5010u64.to_le_bytes());
        node[0xC8..0xD0].copy_from_slice(&0x2_5000u64.to_le_bytes());
        let mut skin = vec![0u8; SKIN_SPAN];
        let w = width as u32;
        for (o, v) in [(0x90, RGBA), (0x94, w), (0x98, w), (0xB8, w * w * 4)] {
            skin[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        skin[0xB0..0xB8].copy_from_slice(&0x10_0000u64.to_le_bytes());
        let pixels = (0..width * width).flat_map(|i| paint(i % width * 64 / width, i / width * 64 / width)).collect();
        [(0x1_1000, node), (0x2_5000, skin), (0x10_0000, pixels)].into()
    }

    fn look(mem: &HashMap<u64, Vec<u8>>) -> Option<Face> {
        let read = |at: u64, out: &mut [u8]| {
            let hit = mem.iter().find(|(a, b)| **a <= at && at + out.len() as u64 <= **a + b.len() as u64);
            let Some((a, b)) = hit else { return false };
            let o = (at - a) as usize;
            out.copy_from_slice(&b[o..o + out.len()]);
            true
        };
        face(&read, 0x1_1000)
    }

    #[test]
    fn gesicht_mit_kopfbedeckung_in_jeder_groesse() {
        for width in [64, 128, 256] {
            let mem = skin(width, |x, y| match (x, y) {
                // Kopfbedeckung nur in der oberen Reihe.
                (40..48, 8) => [1, 2, 3, 255],
                (8..16, 8..16) => [x as u8, y as u8, 9, 255],
                _ => [0; 4],
            });
            let f = look(&mem).unwrap();
            assert_eq!(f.len(), 256);
            assert_eq!(f[..4], [1, 2, 3, 255]);
            assert_eq!(f[(3 * 8 + 5) * 4..(3 * 8 + 5) * 4 + 4], [13, 11, 9, 255], "{width}");
        }
    }

    #[test]
    fn ohne_passendes_bild_kein_gesicht() {
        assert_eq!(look(&skin(64, |_, _| [0; 4])), None);
        let breaks = [(0x2_5000u64, 0x90, 9u8), (0x2_5000, 0xB8, 1), (0x2_5000, 0xB2, 0x77), (0x1_1000, 0xC0, 0x18)];
        for (at, o, v) in breaks {
            let mut mem = skin(64, |_, _| [5, 5, 5, 255]);
            mem.get_mut(&at).unwrap()[o] = v;
            assert_eq!(look(&mem), None, "{at:x} {o:x}");
        }
    }
}
