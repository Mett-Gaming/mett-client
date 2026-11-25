//! Zahlen-Einstellungen des laufenden Spiels (nur lesen). Einstieg sind ihre Namen; welches Feld der Wert ist, ergibt sich
//! aus dem Abgleich vieler Einstellungen mit der Einstellungsdatei.
use crate::Reader;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub const SPACING: &str = "chat_line_spacing";
pub const OPACITY: &str = "gfx_chat_background_opacity";
/// So weit vor dem Namen darf die Beschreibung anfangen.
const REACH: u64 = 0x800;
/// Felder einer Einstellung hinter Tabelle und Beschreibung.
const FIELDS: Range<u64> = 0x10..0x30;
const ENOUGH: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub table: u64,
    /// Abstand des Namens vom Anfang der Beschreibung.
    pub name: u64,
    /// Wo der Wert steht.
    pub value: u64,
    /// (Einstellung, Beschreibung)
    pub spacing: (u64, u64),
    pub opacity: (u64, u64),
}

/// Woran die Suche scheitert, nur Anzahlen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trace {
    /// Fundstellen der zwei Namen, Texte darauf, mögliche Einstellungen, gemeinsame Arten.
    pub names: usize,
    pub texts: usize,
    pub options: usize,
    pub kinds: usize,
    /// Eindeutig belegte Arten, darunter die am besten passende: wie viele Einstellungen bekannt sind und wie viele
    /// beim besten und zweitbesten Feld zur Datei passen.
    pub single: usize,
    pub known: usize,
    pub best: usize,
    pub second: usize,
}

type Read<'a> = &'a (dyn Fn(u64, &mut [u8]) -> bool + Sync);

fn word(read: Read, at: u64) -> Option<u64> {
    let mut b = [0u8; 8];
    read(at, &mut b).then(|| u64::from_le_bytes(b))
}

/// Text an dieser Stelle, kurz im Objekt selbst oder lang über seinen Zeiger.
fn text(read: Read, at: u64) -> Option<String> {
    let mut b = [0u8; 32];
    if !read(at, &mut b) {
        return None;
    }
    let n = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap_or_default());
    let (len, cap) = (n(16), n(24));
    if len == 0 || len > 64 || cap < len || !(15..=4096).contains(&cap) {
        return None;
    }
    let mut t = vec![0u8; len as usize];
    if cap == 15 {
        t.copy_from_slice(&b[..len as usize]);
    } else if !read(n(0), &mut t) {
        return None;
    }
    String::from_utf8(t).ok().filter(|t| t.bytes().all(|c| c.is_ascii_graphic()))
}

/// Zeilen `name:zahl` der Einstellungsdatei.
pub fn parse(file: &str) -> HashMap<String, f32> {
    let pair = |l: &str| l.trim().split_once(':').and_then(|(n, v)| Some((n.to_string(), v.parse().ok()?)));
    file.lines().filter_map(pair).collect()
}

/// Welches Feld stimmt mit der Datei überein?
fn value(found: &[(String, Vec<f32>)], file: &HashMap<String, f32>, trace: &mut Trace) -> Option<u64> {
    let known: Vec<(&Vec<f32>, f32)> = found.iter().filter_map(|(n, f)| Some((f, *file.get(n)?))).collect();
    let mut hits: Vec<(usize, u64)> = FIELDS
        .step_by(4)
        .enumerate()
        .map(|(i, o)| (known.iter().filter(|(f, v)| f.get(i).is_some_and(|x| (x - v).abs() < 0.0005)).count(), o))
        .collect();
    hits.sort_unstable_by(|a, b| b.cmp(a));
    let (best, at) = *hits.first()?;
    // Der Anteil zählt: Falsche Abstände liefern oft viele bekannte Namen mit fremden Werten.
    let share = |known: usize, best: usize| (known >= ENOUGH, best * 1000 / known.max(1), known);
    if share(known.len(), best) >= share(trace.known, trace.best) {
        (trace.known, trace.best, trace.second) = (known.len(), best, hits.get(1).map_or(0, |s| s.0));
    }
    // Der Standardwert stimmt auch oft; der Wert muss klar besser passen.
    let clear = hits.get(1).is_none_or(|s| s.0 + 2 <= best);
    (known.len() >= ENOUGH && best * 10 >= known.len() * 9 && clear).then_some(at)
}

pub fn find(r: &Reader, image: &Range<u64>, file: &HashMap<String, f32>) -> Option<Options> {
    find_traced(r, image, file, &mut Trace::default())
}

pub fn find_traced(r: &Reader, image: &Range<u64>, file: &HashMap<String, f32>, trace: &mut Trace) -> Option<Options> {
    *trace = Trace::default();
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    // 1. Die Namen selbst.
    let names = [SPACING, OPACITY];
    let spots = r.scan(&|at, b| {
        let mut out = vec![];
        for (k, n) in names.iter().enumerate() {
            let n = n.as_bytes();
            for i in (0..b.len().saturating_sub(n.len() + 1)).step_by(8) {
                if b[i] == n[0] && b[i..].starts_with(n) && b[i + n.len()] == 0 {
                    out.push(((at + i) as u64, k));
                }
            }
        }
        out
    });
    let spots: HashMap<u64, usize> = spots.into_iter().collect();
    trace.names = spots.len();
    // 2. Texte, die darauf zeigen und so lang sind.
    let texts = r.scan(&|at, b| {
        let words = b.as_chunks::<8>().0;
        let hit = |(i, w): (usize, &[u8; 8])| {
            let k = *spots.get(&u64::from_le_bytes(*w))?;
            let len = u64::from_le_bytes(*words.get(i + 2)?);
            (len == names[k].len() as u64).then_some(((at + i * 8) as u64, k))
        };
        words.iter().enumerate().filter_map(hit).collect()
    });
    trace.texts = texts.len();
    // 3. Einstellungen: Tabelle des Programms, dann ein Zeiger kurz vor den Text.
    let options = r.scan(&|at, b| {
        let mut out = vec![];
        for (i, w) in b.as_chunks::<8>().0.windows(2).enumerate() {
            let (table, info) = (u64::from_le_bytes(w[0]), u64::from_le_bytes(w[1]));
            if !image.contains(&table) {
                continue;
            }
            for (t, k) in &texts {
                if (1..REACH).contains(&t.wrapping_sub(info)) {
                    out.push((*k, table, t - info, (at + i * 8) as u64, info));
                }
            }
        }
        out
    });
    trace.options = options.len();
    // Beide Namen müssen dieselbe Art und denselben Abstand ergeben. Beschreibungen liegen hintereinander, deshalb passt
    // auch der Abstand zur nächsten; nur beim richtigen stimmen Namen und Werte mit der Datei überein.
    let kinds = |k: usize| options.iter().filter(|o| o.0 == k).map(|o| (o.1, o.2)).collect::<HashSet<_>>();
    let mut good = vec![];
    for (table, name) in kinds(0).intersection(&kinds(1)).copied() {
        trace.kinds += 1;
        let one = |k: usize| {
            let mut it = options.iter().filter(|o| o.0 == k && (o.1, o.2) == (table, name));
            let first = it.next()?;
            it.next().is_none().then_some((first.3, first.4))
        };
        let (Some(spacing), Some(opacity)) = (one(0), one(1)) else { continue };
        trace.single += 1;
        if let Some(value) = value(&all(r, read, table, name), file, trace) {
            good.push(Options { table, name, value, spacing, opacity });
        }
    }
    if good.len() == 1 { good.pop() } else { None }
}

/// Namen und Felder aller Einstellungen dieser Art.
fn all(r: &Reader, read: Read, table: u64, name: u64) -> Vec<(String, Vec<f32>)> {
    r.scan(&|at, b| {
        let words = b.as_chunks::<8>().0.iter().enumerate();
        let hits = words.filter(|(_, w)| u64::from_le_bytes(**w) == table);
        hits.filter_map(|(i, _)| {
            let at = (at + i * 8) as u64;
            let n = text(read, word(read, at + 8)?.checked_add(name)?)?;
            let mut f = vec![0u8; (FIELDS.end - FIELDS.start) as usize];
            read(at + FIELDS.start, &mut f)
                .then(|| (n, f.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect::<Vec<f32>>()))
        })
        .collect()
    })
}

/// Alle Einstellungen dieser Art mit ihren Feldern (nur Entwicklung).
pub fn list(r: &Reader, o: &Options) -> Vec<(String, Vec<f32>)> {
    all(r, &|at, out| r.read(at as usize, out).is_ok(), o.table, o.name)
}

/// Eine weitere Einstellung derselben Art: (Einstellung, Beschreibung). Es muss genau eine geben.
pub fn locate(r: &Reader, o: &Options, name: &str) -> Option<(u64, u64)> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let found = r.scan(&|at, b| {
        let words = b.as_chunks::<8>().0;
        let hit = |(i, w): (usize, &[[u8; 8]])| {
            let info = u64::from_le_bytes(w[1]);
            let ours = u64::from_le_bytes(w[0]) == o.table && text(read, info.checked_add(o.name)?)? == name;
            ours.then_some(((at + i * 8) as u64, info))
        };
        words.windows(2).enumerate().filter_map(hit).collect()
    });
    match found[..] {
        [one] => Some(one),
        _ => None,
    }
}

/// Aktueller Wert, wenn die Einstellung noch dieselbe ist.
pub fn get(r: &Reader, o: &Options, (at, info): (u64, u64), name: &str) -> Option<f32> {
    let read: Read = &|at, out| r.read(at as usize, out).is_ok();
    let same = word(read, at)? == o.table && word(read, at + 8)? == info && text(read, info + o.name)? == name;
    let mut b = [0u8; 4];
    (same && read(at + o.value, &mut b)).then(|| f32::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datei_wird_gelesen() {
        let f = parse("a:1\r\nb:0.5\r\nc:wort\r\nd\r\n key:2:3\r\n");
        assert_eq!((f.get("a"), f.get("b"), f.len()), (Some(&1.0), Some(&0.5), 2));
    }

    #[test]
    fn der_wert_passt_besser_als_der_standard() {
        let file: HashMap<String, f32> = (0..12).map(|i| (format!("o{i}"), i as f32)).collect();
        // Grenzen, Standard, Wert, Schritt.
        let row =
            |i: usize, standard: f32| (format!("o{i}"), vec![-9.0, 99.0, standard, i as f32, 0.001, 0.0, 0.0, 0.0]);
        let mixed: Vec<_> = (0..12).map(|i| row(i, if i < 4 { 7.5 } else { i as f32 })).collect();
        let value = |f: &[_], file: &_| super::value(f, file, &mut Trace::default());
        assert_eq!(value(&mixed, &file), Some(0x1C));
        // Alles auf Standard: nicht zu unterscheiden.
        let same: Vec<_> = (0..12).map(|i| row(i, i as f32)).collect();
        assert_eq!(value(&same, &file), None);
        // Die Spur zeigt, warum.
        let mut trace = Trace::default();
        super::value(&same, &file, &mut trace);
        assert_eq!((trace.known, trace.best, trace.second), (12, 12, 12));
        // Ein schlechter passender Kandidat verdrängt sie nicht, auch wenn er mehr Namen kennt.
        let more: Vec<_> = (0..40).map(|i| row(i % 12, 500.0)).map(|(n, f)| (n, vec![500.0; f.len()])).collect();
        super::value(&more, &file, &mut trace);
        assert_eq!((trace.known, trace.best), (12, 12));
        // Zu wenige bekannte Namen, oder die Datei passt nicht.
        assert_eq!(value(&mixed[..6], &file), None);
        let other: HashMap<String, f32> = (0..12).map(|i| (format!("o{i}"), 500.0 + i as f32)).collect();
        assert_eq!(value(&mixed, &other), None);
    }

    #[test]
    fn texte_kurz_und_lang() {
        let mut short = [0u8; 32];
        short[..3].copy_from_slice(b"abc");
        short[16] = 3;
        short[24] = 15;
        let long_text = *b"chat_line_spacing";
        let mut long = [0u8; 32];
        long[..8].copy_from_slice(&0x2000u64.to_le_bytes());
        long[16] = 17;
        long[24] = 31;
        let read = |at: u64, out: &mut [u8]| {
            let from: &[u8] = match at {
                0x1000 => &short,
                0x1100 => &long,
                0x2000 => &long_text,
                _ => return false,
            };
            out.copy_from_slice(&from[..out.len()]);
            true
        };
        assert_eq!(text(&read, 0x1000).as_deref(), Some("abc"));
        assert_eq!(text(&read, 0x1100).as_deref(), Some(SPACING));
        assert_eq!(text(&read, 0x3000), None);
    }
}
