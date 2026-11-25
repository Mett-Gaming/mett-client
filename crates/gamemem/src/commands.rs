//! Befehlsliste des Spiels lesen (nur lesen): die Client-Seite, also genau die Befehle, die auch der Chat des Spiels
//! vorschlägt. Darin stehen auch
//! eigene Befehle von Servern. Einstieg sind Befehle, die der Client selbst mitbringt; jede Stufe prüft die Form. Unklares heißt: keine Hinweise.
use crate::Reader;
use std::collections::{BTreeMap, HashSet};

/// Alles über einen Parameter, so wie das Spiel ihn kennt.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub optional: bool,
    pub values: Values,
    /// Gleiche Art heißt gleiche Zahl; 0, wenn das Spiel keine nennt. Gilt nur in diesem Spielstart.
    pub kind: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Values {
    /// Freie Eingabe.
    Free,
    /// Feste Werte, als Stelle in `Commands::lists`.
    List(usize),
    /// Werte, die sich im Spiel ändern, als Stelle in `Commands::lists`.
    Changing(usize),
    /// Danach folgt wieder eine Variante desselben Befehls (`execute`).
    Chain,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Command {
    /// Übersetzungsschlüssel.
    pub description: String,
    pub variants: Vec<Vec<Param>>,
    /// Andere Namen, so wie das Spiel sie verknüpft.
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct List {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Commands {
    pub commands: BTreeMap<String, Command>,
    pub lists: Vec<List>,
}

/// Wo die Liste liegt. Gilt nur für den laufenden Prozess.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    map: usize,
    texts: usize,
    lists: usize,
    /// Nur, wenn eindeutig.
    changing: Option<usize>,
}

type Read<'a> = &'a (dyn Fn(usize, &mut [u8]) -> bool + Sync);

/// Bringt der Client selbst mit, unabhängig vom Server.
const ANCHORS: [&str; 2] = ["help", "closechat"];
// Baumknoten: links, oben, rechts, Kennungen, Name, Befehl {Name, Beschreibung, Varianten}.
const KEY: usize = 0x20;
const NAME: usize = 0x40;
const TEXT: usize = 0x60;
const VARIANTS: usize = 0x80;
/// Verweis auf die Liste der anderen Namen.
const ALIASES: usize = 0xB8;
const NODE: usize = 0xC0;
const VARIANT: usize = 0x80;
const VARIANT_RUN: usize = 8;
const VARIANT_PARAMS: usize = 0x48;
const PARAM: usize = 0x78;
const LIST: usize = 0x48;
const LIST_VALUES: usize = 0x30;
const CHANGING: usize = 0x38;
const CHANGING_VALUES: usize = 0x20;
const MAX_COMMANDS: usize = 2000;
const MAX_VARIANTS: usize = 500;
const MAX_PARAMS: usize = 40;
const MAX_LISTS: usize = 4000;
const MAX_TEXTS: usize = 200_000;
/// So weit vor und hinter der Befehlskarte liegen die übrigen Listen.
const NEAR: usize = 0x400;

fn num(b: &[u8], at: usize) -> Option<usize> {
    Some(usize::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn small(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn bytes(read: Read, at: usize, n: usize) -> Option<Vec<u8>> {
    let mut b = vec![0u8; n];
    (at > 0x10000 && read(at, &mut b)).then_some(b)
}

/// Zeichenkette im Aufbau von MSVC, ohne Steuerzeichen.
fn string(read: Read, b: &[u8]) -> Option<String> {
    let (len, cap) = (num(b, 16)?, num(b, 24)?);
    if len > cap || cap < 15 || len > 512 || cap > 1 << 20 {
        return None;
    }
    let t = if cap == 15 { b.get(..len)?.to_vec() } else { bytes(read, num(b, 0)?, len.max(1))?[..len].to_vec() };
    String::from_utf8(t).ok().filter(|t| !t.chars().any(char::is_control))
}

/// Liste {Anfang, Ende, Platz} mit fester Schrittweite: (Anfang, Anzahl).
fn vector(b: &[u8], at: usize, step: usize, max: usize) -> Option<(usize, usize)> {
    let (from, to, cap) = (num(b, at)?, num(b, at + 8)?, num(b, at + 16)?);
    if from == 0 && to == 0 {
        return Some((0, 0));
    }
    let used = to.checked_sub(from)?;
    let ok = from > 0x10000 && from % 8 == 0 && to <= cap && used % step == 0 && used / step <= max;
    ok.then_some((from, used / step))
}

/// Kurzer Text so, wie er im Knoten steht.
fn short(t: &str) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[..t.len()].copy_from_slice(t.as_bytes());
    b[16..24].copy_from_slice(&t.len().to_le_bytes());
    b[24..].copy_from_slice(&15usize.to_le_bytes());
    b
}

struct Node {
    left: usize,
    parent: usize,
    right: usize,
    nil: bool,
    body: Vec<u8>,
}

fn node(read: Read, at: usize) -> Option<Node> {
    let body = bytes(read, at, NODE)?;
    let (color, nil) = (body[0x18], body[0x19]);
    let ok = at.is_multiple_of(8) && color <= 1 && nil <= 1;
    let word = |o| num(&body, o);
    ok.then_some(Node { left: word(0)?, parent: word(8)?, right: word(16)?, nil: nil == 1, body })
}

/// (Varianten, davon ohne Ausführfunktion, Befehle ganz ohne Varianten). Auf der Server-Seite des Einzelspielers hat
/// jede Variante eine Funktion; was ein Server dem Client schickt, hat keine.
fn counts(read: Read, nodes: &[Node]) -> Option<(usize, usize, usize)> {
    let (mut all, mut empty, mut bare) = (0, 0, 0);
    for n in nodes {
        let (from, count) = vector(&n.body, VARIANTS, VARIANT, MAX_VARIANTS)?;
        let b = if count == 0 { vec![] } else { bytes(read, from, count * VARIANT)? };
        all += count;
        bare += (count == 0) as usize;
        empty += (0..count).filter(|i| num(&b, i * VARIANT + VARIANT_RUN) == Some(0)).count();
    }
    Some((all, empty, bare))
}

/// Die Client-Seite unter mehreren Bäumen. Kleine Server schicken nur wenige Befehle, deshalb zählt die Mehrheit erst,
/// wenn es sonst nicht eindeutig ist.
fn client_side(read: Read, heads: &[usize]) -> Option<usize> {
    let seen: Vec<_> = heads.iter().filter_map(|h| Some((*h, counts(read, &walk(read, *h)?)?))).collect();
    let sent: Vec<_> = seen.iter().filter(|(_, c)| c.1 + c.2 > 0).collect();
    if let [(h, _)] = sent[..] {
        return Some(*h);
    }
    let most: Vec<_> = sent.into_iter().filter(|(_, c)| c.1 * 2 > c.0).collect();
    match most[..] {
        [(h, _)] => Some(*h),
        _ => None,
    }
}

/// Alle Knoten in Reihenfolge, vom Kopf aus.
fn walk(read: Read, head: usize) -> Option<Vec<Node>> {
    let h = node(read, head).filter(|h| h.nil)?;
    let (mut out, mut seen, mut stack, mut at) = (vec![], HashSet::new(), vec![], h.parent);
    loop {
        // Ganz nach links, dann der Knoten selbst, dann rechts weiter.
        while at != head {
            if !seen.insert(at) || seen.len() > MAX_COMMANDS {
                return None;
            }
            let n = node(read, at).filter(|n| !n.nil)?;
            at = n.left;
            stack.push(n);
        }
        let Some(n) = stack.pop() else { return Some(out) };
        at = n.right;
        out.push(n);
    }
}

/// Sucht die Liste im ganzen Speicher. `None`: nicht da oder nicht eindeutig.
pub fn find(r: &Reader) -> Option<Place> {
    let read: Read = &|at, out| r.read(at, out).is_ok();
    let keys = ANCHORS.map(short);
    // Der Name steht zweimal: als Schlüssel und im Befehl.
    let nodes = r.scan(&|start, buf| {
        let words = buf.as_chunks::<8>().0.len();
        let mut out = vec![];
        for i in 0..words {
            let o = i * 8;
            if keys.iter().any(|k| buf[o..].starts_with(k) && buf.get(o + 32..o + 64) == Some(&k[..])) {
                out.push((start + o).wrapping_sub(KEY));
            }
        }
        out
    });
    let mut heads: Vec<usize> = nodes.into_iter().filter_map(|n| head_of(read, n)).collect();
    heads.sort_unstable();
    heads.dedup();
    let head = client_side(read, &heads)?;
    let nodes = walk(read, head)?;
    let (count, needed) = (nodes.len(), needed(read, &nodes)?);
    let mut wanted = head.to_le_bytes().to_vec();
    wanted.extend(count.to_le_bytes());
    let maps = r.scan(&|start, buf| {
        let at = (0..buf.len().saturating_sub(15)).step_by(8);
        at.filter(|o| buf[*o..*o + 16] == wanted[..]).map(|o| start + o).collect()
    });
    let mut places: Vec<Place> = maps.into_iter().filter_map(|m| around(read, m, needed)).collect();
    places.dedup();
    match places[..] {
        [p] => Some(p),
        _ => None,
    }
}

/// Vom gefundenen Knoten zum Kopf.
fn head_of(read: Read, at: usize) -> Option<usize> {
    let mut n = node(read, at).filter(|n| !n.nil)?;
    // Beschreibungen dürfen Server frei wählen; hier zählt nur, dass es ein Text ist.
    string(read, &n.body[TEXT..])?;
    for _ in 0..64 {
        let up = n.parent;
        n = node(read, up)?;
        if n.nil {
            return Some(up);
        }
    }
    None
}

/// Die Listen rund um die Karte. Werte und Wertelisten müssen zueinander passen, und zwar genau auf eine Weise.
fn around(read: Read, map: usize, needed: usize) -> Option<Place> {
    let start = map.checked_sub(NEAR)?;
    let b = bytes(read, start, NEAR * 2)?;
    let vectors = |step, max| -> Vec<(usize, usize, usize)> {
        let spots = (0..b.len() - 24).step_by(8);
        spots.filter_map(|o| vector(&b, o, step, max).filter(|v| v.1 > 0).map(|v| (start + o, v.0, v.1))).collect()
    };
    let texts: Vec<_> = vectors(32, MAX_TEXTS).into_iter().filter(|v| all_strings(read, v.1, v.2, 32, 40)).collect();
    // Ein zweites, kleines Paar gilt für Unterbefehle; dort passen die Nummern nicht hinein.
    let lists: Vec<_> = vectors(LIST, MAX_LISTS)
        .into_iter()
        .filter(|v| v.2 >= needed)
        .filter_map(|v| Some((v.0, highest(read, v.1, v.2)?)))
        .collect();
    // Die Listen nutzen fast alle Werte und nie mehr, als es gibt.
    let pairs: Vec<_> = texts
        .iter()
        .flat_map(|t| lists.iter().map(move |l| (t, l)))
        .filter(|(t, l)| l.1 < t.2 && (l.1 + 1) * 10 >= t.2 * 9)
        .collect();
    let [(t, l)] = pairs[..] else { return None };
    let changing: Vec<_> = vectors(CHANGING, MAX_LISTS)
        .into_iter()
        .filter(|v| v.0 != l.0 && all_strings(read, v.1, v.2, CHANGING, v.2) && changing_ok(read, v.1, v.2))
        .collect();
    let changing = match changing[..] {
        [c] => Some(c.0),
        _ => None,
    };
    Some(Place { map, texts: t.0, lists: l.0, changing })
}

/// So viele Wertelisten setzen die Parameter voraus.
fn needed(read: Read, nodes: &[Node]) -> Option<usize> {
    let mut high = 0;
    for n in nodes {
        let (from, count) = vector(&n.body, VARIANTS, VARIANT, MAX_VARIANTS)?;
        let b = if count == 0 { vec![] } else { bytes(read, from, count * VARIANT)? };
        for i in 0..count {
            for p in params(read, &b[i * VARIANT..(i + 1) * VARIANT], usize::MAX, usize::MAX)? {
                if let Values::List(l) = p.values {
                    high = high.max(l + 1);
                }
            }
        }
    }
    Some(high)
}

/// Höchste benutzte Wertnummer, wenn wirklich jede Liste eine ist.
fn highest(read: Read, from: usize, n: usize) -> Option<usize> {
    if !all_strings(read, from, n, LIST, n) {
        return None;
    }
    let b = bytes(read, from, n * LIST)?;
    let mut high = None;
    for i in 0..n {
        let (at, count) = vector(&b, i * LIST + LIST_VALUES, 16, MAX_TEXTS)?;
        let v = if count == 0 { vec![] } else { bytes(read, at, count * 16)? };
        for k in 0..count {
            high = high.max(Some(small(&v, k * 16)? as usize));
        }
    }
    high.filter(|h| *h < MAX_TEXTS)
}

/// Stichprobe am Anfang: lauter nicht leere Texte.
fn all_strings(read: Read, from: usize, n: usize, step: usize, sample: usize) -> bool {
    let n = n.min(sample);
    let Some(b) = bytes(read, from, n * step) else { return false };
    (0..n).all(|i| string(read, &b[i * step..]).is_some_and(|s| !s.is_empty()))
}

fn changing_ok(read: Read, from: usize, n: usize) -> bool {
    let Some(b) = bytes(read, from, n * CHANGING) else { return false };
    (0..n).all(|i| {
        vector(&b, i * CHANGING + CHANGING_VALUES, 32, MAX_TEXTS)
            .is_some_and(|(at, n)| n == 0 || all_strings(read, at, n, 32, 5))
    })
}

fn strings(read: Read, from: usize, n: usize) -> Option<Vec<String>> {
    if n == 0 {
        return Some(vec![]);
    }
    let b = bytes(read, from, n * 32)?;
    (0..n).map(|i| string(read, &b[i * 32..])).collect()
}

/// Billige Kennung, um Änderungen zu bemerken.
pub fn stamp(r: &Reader, p: &Place) -> Option<(usize, usize)> {
    let mut b = [0u8; 16];
    r.read(p.map, &mut b).ok()?;
    let count = num(&b, 8)?;
    (count > 0 && count <= MAX_COMMANDS).then_some((num(&b, 0)?, count))
}

/// Alles neu lesen. `None`, sobald etwas nicht passt (z. B. andere Welt).
pub fn load(r: &Reader, p: &Place) -> Option<Commands> {
    load_with(&|at, out| r.read(at, out).is_ok(), p)
}

fn load_with(read: Read, p: &Place) -> Option<Commands> {
    let map = bytes(read, p.map, 16)?;
    let nodes = walk(read, num(&map, 0)?)?;
    if nodes.len() != num(&map, 8)? {
        return None;
    }
    let (from, n) = vector(&bytes(read, p.texts, 24)?, 0, 32, MAX_TEXTS)?;
    let texts = strings(read, from, n)?;
    let mut lists = vec![];
    let (from, fixed) = vector(&bytes(read, p.lists, 24)?, 0, LIST, MAX_LISTS)?;
    let b = bytes(read, from, fixed * LIST)?;
    for i in 0..fixed {
        let l = &b[i * LIST..(i + 1) * LIST];
        let (at, n) = vector(l, LIST_VALUES, 16, MAX_TEXTS)?;
        let values = if n == 0 { vec![] } else { bytes(read, at, n * 16)? };
        let values: Option<Vec<String>> =
            (0..n).map(|k| texts.get(small(&values, k * 16)? as usize).cloned()).collect();
        lists.push(List { name: string(read, l)?, values: values? });
    }
    let (from, n) = match p.changing {
        Some(at) => vector(&bytes(read, at, 24)?, 0, CHANGING, MAX_LISTS)?,
        None => (0, 0),
    };
    let b = if n == 0 { vec![] } else { bytes(read, from, n * CHANGING)? };
    for i in 0..n {
        let l = &b[i * CHANGING..(i + 1) * CHANGING];
        let (at, count) = vector(l, CHANGING_VALUES, 32, MAX_TEXTS)?;
        lists.push(List { name: string(read, l)?, values: strings(read, at, count)? });
    }
    let mut commands = BTreeMap::new();
    for node in nodes {
        let name = string(read, &node.body[NAME..])?;
        if name.is_empty() || string(read, &node.body[KEY..])? != name {
            return None;
        }
        let description = string(read, &node.body[TEXT..])?;
        let (from, count) = vector(&node.body, VARIANTS, VARIANT, MAX_VARIANTS)?;
        let b = if count == 0 { vec![] } else { bytes(read, from, count * VARIANT)? };
        let variants: Option<Vec<_>> =
            (0..count).map(|i| params(read, &b[i * VARIANT..(i + 1) * VARIANT], fixed, lists.len())).collect();
        // Nur glauben, wenn die Liste den Befehl selbst nennt.
        let symbol = small(&node.body, ALIASES)?;
        let list = (symbol != u32::MAX && symbol & 0x20_0000 != 0).then_some((symbol & 0xFFFF) as usize);
        let list = list.filter(|l| *l < fixed).and_then(|l| lists.get(l)).filter(|l| l.values.contains(&name));
        let aliases = list.map_or(vec![], |l| l.values.iter().filter(|v| **v != name).cloned().collect());
        commands.insert(name, Command { description, variants: variants?, aliases });
    }
    Some(Commands { commands, lists })
}

fn params(read: Read, v: &[u8], fixed: usize, all: usize) -> Option<Vec<Param>> {
    let (from, n) = vector(v, VARIANT_PARAMS, PARAM, MAX_PARAMS)?;
    let b = if n == 0 { vec![] } else { bytes(read, from, n * PARAM)? };
    (0..n)
        .map(|i| {
            let p = &b[i * PARAM..(i + 1) * PARAM];
            let (symbol, sort) = (small(p, 0x40)?, small(p, 0x54)?);
            let index = (symbol & 0xFFFF) as usize;
            let values = match sort {
                // Aufzählung: Kennzeichen 0x200000, darunter die Nummer der Liste.
                1 if symbol & 0x20_0000 != 0 && index < fixed => Values::List(index),
                2 if symbol & 0x400_0000 != 0 && index < all.saturating_sub(fixed) => Values::Changing(fixed + index),
                4 => Values::Chain,
                _ => Values::Free,
            };
            let optional = match p[0x60] {
                0 => false,
                1 => true,
                _ => return None,
            };
            Some(Param { name: string(read, &p[0x18..])?, optional, values, kind: num(p, 8)? as u64 })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nachgebauter Speicher ab Adresse 0x20000.
    struct Memory(Vec<u8>);

    const BASE: usize = 0x20000;

    impl Memory {
        fn put(&mut self, at: usize, b: &[u8]) {
            self.0[at - BASE..at - BASE + b.len()].copy_from_slice(b);
        }

        fn word(&mut self, at: usize, v: usize) {
            self.put(at, &v.to_le_bytes());
        }

        fn list(&mut self, at: usize, from: usize, bytes: usize) {
            self.word(at, from);
            self.word(at + 8, from + bytes);
            self.word(at + 16, from + bytes);
        }
    }

    const HEAD: usize = 0x20100;
    const HELP: usize = 0x20200;
    const ENCHANT: usize = 0x20300;
    const MAP: usize = 0x21000;

    fn game() -> Memory {
        let mut m = Memory(vec![0; 0x10000]);
        m.word(HEAD + 8, HELP);
        m.put(HEAD + 0x18, &[1, 1]);
        for (at, name, left) in [(HELP, "help", ENCHANT), (ENCHANT, "enchant", HEAD)] {
            m.word(at, left);
            m.word(at + 8, if at == HELP { HEAD } else { HELP });
            m.word(at + 16, HEAD);
            m.put(at + KEY, &short(name));
            m.put(at + NAME, &short(name));
            // Langer Text liegt außerhalb.
            let text = format!("commands.{name}.description");
            m.put(at + 0xD0, text.as_bytes());
            m.word(at + TEXT, at + 0xD0);
            m.word(at + TEXT + 16, text.len());
            m.word(at + TEXT + 24, 63);
        }
        // help: eine leere Variante, enchant: player, enchantmentName, [level].
        m.list(HELP + VARIANTS, 0x22000, VARIANT);
        m.list(ENCHANT + VARIANTS, 0x22100, VARIANT);
        m.list(0x22100 + VARIANT_PARAMS, 0x22200, PARAM * 3);
        let params = [
            ("player", u32::MAX, 0u32, 0u8, 7usize),
            ("enchantment", 0x30_0000, 1, 0, 0),
            ("level", 0x410_0000, 2, 1, 0),
        ];
        for (i, (name, symbol, sort, optional, kind)) in params.into_iter().enumerate() {
            let at = 0x22200 + i * PARAM;
            m.word(at + 8, kind);
            m.put(at + 0x18, &short(name));
            m.put(at + 0x40, &symbol.to_le_bytes());
            m.put(at + 0x54, &sort.to_le_bytes());
            m.put(at + 0x60, &[optional]);
        }
        m.word(MAP, HEAD);
        m.word(MAP + 8, 2);
        // Werte, feste Listen, veränderliche Listen.
        for i in 0..400 {
            m.put(0x23000 + i * 32, &short(&format!("wert{i}")));
        }
        m.list(MAP - 0x80, 0x23000, 400 * 32);
        for i in 0..25 {
            let at = 0x27000 + i * LIST;
            m.put(at, &short(&format!("Liste{i}")));
            m.list(at + LIST_VALUES, 0x28000, 32);
        }
        let long = b"helpCommandAliases";
        m.put(0x28100, long);
        m.word(0x27000 + LIST, 0x28100);
        m.word(0x27000 + LIST + 16, long.len());
        m.word(0x27000 + LIST + 24, 31);
        m.word(0x28000, 5);
        m.word(0x28010, 399);
        m.list(MAP - 0x60, 0x27000, 25 * LIST);
        m.put(0x28200, &short("Stufen"));
        m.list(0x28200 + CHANGING_VALUES, 0x28300, 64);
        m.put(0x28300, &short("eins"));
        m.put(0x28320, &short("zwei"));
        m.list(MAP + 0x40, 0x28200, CHANGING);
        m
    }

    fn with<R>(m: &Memory, f: impl FnOnce(Read) -> R) -> R {
        f(&|at, out| {
            let Some(o) = at.checked_sub(BASE) else { return false };
            let Some(b) = m.0.get(o..o + out.len()) else { return false };
            out.copy_from_slice(b);
            true
        })
    }

    #[test]
    fn liest_befehle_parameter_und_werte() {
        let m = game();
        with(&m, |read| {
            assert_eq!(head_of(read, HELP), Some(HEAD));
            assert_eq!(head_of(read, ENCHANT), Some(HEAD));
            assert_eq!(client_side(read, &[HEAD]), Some(HEAD));
            assert_eq!(needed(read, &walk(read, HEAD).unwrap()), Some(1));
            // Mehr Listen nötig, als es gibt: nicht die richtige Stelle.
            assert_eq!(around(read, MAP, 26), None);
            let p = around(read, MAP, 1).unwrap();
            let c = load_with(read, &p).unwrap();
            assert_eq!(c.commands.keys().collect::<Vec<_>>(), ["enchant", "help"]);
            let e = &c.commands["enchant"];
            assert_eq!(e.description, "commands.enchant.description");
            let [v] = &e.variants[..] else { panic!() };
            assert_eq!(
                v.iter().map(|p| (p.name.as_str(), p.optional, p.kind)).collect::<Vec<_>>(),
                [("player", false, 7), ("enchantment", false, 0), ("level", true, 0)]
            );
            assert_eq!(
                v.iter().map(|p| &p.values).collect::<Vec<_>>(),
                [&Values::Free, &Values::List(0), &Values::Changing(25)]
            );
            assert_eq!(c.lists[1].name, "helpCommandAliases");
            assert_eq!(c.lists[0].values, ["wert5", "wert399"]);
            assert_eq!(c.lists[25].values, ["eins", "zwei"]);
            assert_eq!(c.commands["help"].variants, [vec![]]);
            assert!(c.commands["help"].aliases.is_empty());
        });
        // Andere Namen über den Verweis; eine Liste ohne den Befehl selbst zählt nicht.
        let mut m = game();
        m.put(HELP + ALIASES, &0x30_0000u32.to_le_bytes());
        m.put(ENCHANT + ALIASES, &0x30_0001u32.to_le_bytes());
        m.put(0x23000 + 5 * 32, &short("help"));
        m.put(0x23000 + 399 * 32, &short("hilfe"));
        with(&m, |read| {
            let c = load_with(read, &around(read, MAP, 1).unwrap()).unwrap();
            assert_eq!(c.commands["help"].aliases, ["hilfe"]);
            assert!(c.commands["enchant"].aliases.is_empty());
        });
    }

    #[test]
    fn serverseite_und_kaputtes_gilt_nicht() {
        // Überall Ausführfunktionen: Serverseite.
        let mut m = game();
        m.word(0x22000 + VARIANT_RUN, 0x7FF6_0000_0000);
        m.word(0x22100 + VARIANT_RUN, 0x7FF6_0000_0000);
        assert_eq!(with(&m, |r| client_side(r, &[HEAD])), None);
        // Wie auf einem kleinen Server: fast alles gehört dem Client, ein Befehl hat keine Varianten.
        m.word(HELP + VARIANTS + 8, 0x22000);
        assert_eq!(with(&m, |r| client_side(r, &[HEAD])), Some(HEAD));
        // Zwei Bäume, die beide so aussehen: lieber keiner.
        assert_eq!(with(&m, |r| client_side(r, &[HEAD, HEAD])), None);
        // Kreis im Baum, falsche Anzahl, Wert außerhalb, unsinniges Kennzeichen.
        let breaks: [&dyn Fn(&mut Memory); 5] = [
            &|m| m.word(ENCHANT, HELP),
            &|m| m.word(MAP + 8, 3),
            &|m| m.word(0x28010, 400),
            &|m| m.put(0x22200 + 0x60, &[7]),
            &|m| m.put(ENCHANT + NAME, &short("anders")),
        ];
        for (i, f) in breaks.iter().enumerate() {
            let mut m = game();
            f(&mut m);
            with(&m, |read| {
                let c = around(read, MAP, 1).and_then(|p| load_with(read, &p));
                assert_eq!(c, None, "{i}");
            });
        }
        // Server mit wenigen eigenen Werten und freiem Text bei `help`.
        let mut m = game();
        m.list(MAP - 0x80, 0x23000, 6 * 32);
        m.word(0x28010, 0);
        m.put(HELP + 0xD0, "§aHilfe von unserem Server!".as_bytes());
        with(&m, |read| {
            assert_eq!(head_of(read, HELP), Some(HEAD));
            let p = around(read, MAP, 1).unwrap();
            assert_eq!(load_with(read, &p).unwrap().lists[0].values, ["wert5", "wert0"]);
        });
        // Veränderliche Listen nicht eindeutig: nur diese fallen aus.
        let mut m = game();
        m.list(MAP + 0x80, 0x28200, CHANGING);
        with(&m, |read| {
            let p = around(read, MAP, 1).unwrap();
            assert_eq!(p.changing, None);
            let c = load_with(read, &p).unwrap();
            assert_eq!(c.commands["enchant"].variants[0][2].values, Values::Free);
        });
        // Zwei mögliche Wertelisten: lieber keine.
        let mut m = game();
        m.list(MAP - 0x200, 0x23000, 400 * 32);
        assert_eq!(with(&m, |r| around(r, MAP, 1)), None);
    }
}
