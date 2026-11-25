//! Findet die Funktionstabellen des HUD-Controllers über den Namen seines Konstruktors, der das gesuchte Modell nennt.
//! Damit lässt sich das lebende Modell im Speicher erkennen, ohne feste Adressen.
use crate::image::Image;
use std::collections::HashSet;
use std::ops::Range;

const CTOR: &str = "?0HudScreenController@@QEAA@V?$shared_ptr@VClientInstanceScreenModel@@";

/// Tabellen, die der Konstruktor setzt. `None`, wenn es nicht genau einen Konstruktor gibt.
pub fn tables(i: &Image) -> Option<Vec<usize>> {
    // Kleine Hilfsfunktionen im Konstruktor tragen seinen Namen; ihre laufende Nummer spielt keine Rolle.
    let types: HashSet<usize> =
        i.classes(CTOR).into_iter().filter(|c| c.0.starts_with(".?AV<lambda")).map(|c| c.1).collect();
    let asked: HashSet<usize> = i.lea_to(&types).into_iter().map(|(at, _)| at).collect();
    // Deren Tabellen: Die Typabfrage ist der vierte Eintrag.
    let lambda_tables: HashSet<usize> =
        i.pointers_to(&asked).into_iter().filter_map(|p| p.checked_sub(24)).filter(|t| i.rdata.contains(t)).collect();
    let helpers: HashSet<usize> =
        lambda_tables.iter().flat_map(|t| (0..6).filter_map(move |k| i.pointer(t + k * 8))).collect();
    let mut ctors: Vec<Range<usize>> = i
        .lea_to(&lambda_tables)
        .into_iter()
        .filter_map(|(at, _)| i.function_around(at))
        .filter(|f| !helpers.contains(&f.start))
        .collect();
    ctors.sort_by_key(|f| f.start);
    ctors.dedup();
    let [ctor] = &ctors[..] else { return None };
    let mut out: Vec<usize> = i
        .lea_targets(ctor)
        .into_iter()
        .filter(|t| t % 8 == 0 && is_table(i, *t) && !lambda_tables.contains(t))
        .collect();
    out.sort_unstable();
    out.dedup();
    (!out.is_empty()).then_some(out)
}

fn is_table(i: &Image, t: usize) -> bool {
    i.rdata.contains(&t) && (0..6).all(|k| i.pointer(t + k * 8).is_some_and(|p| i.text.contains(&p)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::tests::{BASE, blank, put};

    fn lea(b: &mut [u8], at: usize, to: usize) {
        put(b, at, &[0x48, 0x8D, 0x05]);
        put(b, at + 3, &((to as i64 - at as i64 - 7) as i32).to_le_bytes());
    }

    fn table(b: &mut [u8], at: usize, to: usize) {
        for k in 0..6 {
            put(b, at + k * 8, &(BASE + to as u64).to_le_bytes());
        }
    }

    fn game() -> Vec<u8> {
        let mut b = blank();
        let name = format!(".?AV<lambda_7>@?0??{CTOR}std@@_N@Z@\0");
        put(&mut b, 0x3110, name.as_bytes());
        // Typabfrage bei 0x1800, Kopierhilfe bei 0x1900, Konstruktor ab 0x1100.
        lea(&mut b, 0x1800, 0x3100);
        table(&mut b, 0x2100, 0x1900);
        put(&mut b, 0x2100 + 24, &(BASE + 0x1800).to_le_bytes());
        table(&mut b, 0x2400, 0x1A00);
        lea(&mut b, 0x1110, 0x2100);
        lea(&mut b, 0x1120, 0x2400);
        // Die Kopierhilfe benutzt die Lambda-Tabelle ebenfalls und darf nicht als Konstruktor gelten.
        lea(&mut b, 0x1910, 0x2100);
        for (k, (s, e)) in [(0x1100u32, 0x1200u32), (0x1800, 0x1840), (0x1900, 0x1940)].into_iter().enumerate() {
            put(&mut b, 0x4000 + k * 12, &s.to_le_bytes());
            put(&mut b, 0x4004 + k * 12, &e.to_le_bytes());
        }
        b
    }

    #[test]
    fn findet_die_tabelle_des_controllers() {
        assert_eq!(tables(&Image::new(BASE, &game()).unwrap()), Some(vec![0x2400]));
    }

    #[test]
    fn zwei_konstruktoren_oder_kein_name_schalten_ab() {
        let mut b = game();
        lea(&mut b, 0x1C10, 0x2100);
        put(&mut b, 0x4000 + 36, &0x1C00u32.to_le_bytes());
        put(&mut b, 0x4004 + 36, &0x1C40u32.to_le_bytes());
        assert_eq!(tables(&Image::new(BASE, &b).unwrap()), None);
        let mut b = game();
        b[0x3118] = b'X';
        assert_eq!(tables(&Image::new(BASE, &b).unwrap()), None);
    }
}
