//! Schutz vor Bedrock-Updates.
//!
//! Jede versionsabhängige Funktion hat eigene Byte-Muster und wird bei jedem Spielstart einzeln geprüft.
//! Passt ein Muster nicht eindeutig, ist nur diese Funktion aus. Der externe Kern benutzt dieses Modul nicht.
pub mod chat_send;
pub mod hud_model;
pub mod image;

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Alle Stellen eindeutig gefunden, in Reihenfolge der Muster.
    Ready(Vec<usize>),
    Off(Reason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Eintrag im Profil ist fehlerhaft.
    BadEntry,
    /// Muster nicht gefunden: Das Update hat diese Stelle verändert.
    NotFound,
    /// Muster mehrfach gefunden: lieber aus als falsch.
    Ambiguous,
}

#[derive(Deserialize)]
struct Feature {
    patterns: Vec<String>,
}

/// Prüft alle Funktionen unabhängig voneinander. Nur ein komplett unlesbares Profil ist ein Fehler.
pub fn resolve(profile: &str, image: &[u8]) -> Result<BTreeMap<String, State>, serde_json::Error> {
    // Erst als lose Werte lesen, damit ein kaputter Eintrag die übrigen nicht mitreißt.
    let raw: BTreeMap<String, serde_json::Value> = serde_json::from_str(profile)?;
    Ok(raw.into_iter().map(|(name, value)| (name, check(value, image))).collect())
}

fn check(value: serde_json::Value, image: &[u8]) -> State {
    let Ok(feature) = serde_json::from_value::<Feature>(value) else { return State::Off(Reason::BadEntry) };
    if feature.patterns.is_empty() {
        return State::Off(Reason::BadEntry);
    }
    let mut found = vec![];
    for text in &feature.patterns {
        let Some(pattern) = parse(text) else { return State::Off(Reason::BadEntry) };
        match find_unique(&pattern, image) {
            Ok(at) => found.push(at),
            Err(r) => return State::Off(r),
        }
    }
    State::Ready(found)
}

/// Muster wie `48 8B ?? 05 06`; mindestens vier feste Bytes, damit nichts zufällig passt.
fn parse(text: &str) -> Option<Vec<Option<u8>>> {
    let byte = |t: &str| match t {
        "??" => Some(None),
        t if t.len() == 2 && t.bytes().all(|b| b.is_ascii_hexdigit()) => u8::from_str_radix(t, 16).ok().map(Some),
        _ => None,
    };
    let p: Vec<Option<u8>> = text.split_whitespace().map(byte).collect::<Option<_>>()?;
    (p.iter().flatten().count() >= 4).then_some(p)
}

fn find_unique(pattern: &[Option<u8>], image: &[u8]) -> Result<usize, Reason> {
    let mut hits = image
        .windows(pattern.len())
        .enumerate()
        .filter(|(_, w)| w.iter().zip(pattern).all(|(b, p)| p.is_none_or(|p| p == *b)))
        .map(|(i, _)| i);
    let first = hits.next().ok_or(Reason::NotFound)?;
    if hits.next().is_some() { Err(Reason::Ambiguous) } else { Ok(first) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE: &[u8] = &[0, 1, 2, 3, 4, 5, 9, 9, 9, 9, 9, 9, 0xAA, 0xBB, 0xCC, 0xDD];

    #[test]
    fn eine_kaputte_funktion_laesst_die_anderen_leben() {
        let profile = r#"{
            "zoom": {"patterns": ["01 02 ?? 04 05"]},
            "koordinaten": {"patterns": ["AA BB CC DD", "00 01 02 03"]},
            "fullbright": {"patterns": ["DE AD BE EF"]},
            "crosshair": {"patterns": ["09 09 09 09"]},
            "tippfehler": {"patterns": ["ZZ 01 02 03 04"]},
            "falsche-form": 7,
            "leer": {"patterns": []}
        }"#;
        let r = resolve(profile, IMAGE).unwrap();
        assert_eq!(r["zoom"], State::Ready(vec![1]));
        assert_eq!(r["koordinaten"], State::Ready(vec![12, 0]));
        assert_eq!(r["fullbright"], State::Off(Reason::NotFound));
        assert_eq!(r["crosshair"], State::Off(Reason::Ambiguous));
        for n in ["tippfehler", "falsche-form", "leer"] {
            assert_eq!(r[n], State::Off(Reason::BadEntry), "{n}");
        }
    }

    #[test]
    fn verschobener_code_wird_wiedergefunden() {
        let moved = [&[0x77; 50], IMAGE].concat();
        let r = resolve(r#"{"zoom": {"patterns": ["01 02 ?? 04 05"]}}"#, &moved).unwrap();
        assert_eq!(r["zoom"], State::Ready(vec![51]));
    }

    #[test]
    fn zu_unspezifische_muster_werden_abgelehnt() {
        assert!(parse("01 ?? ?? 02").is_none());
        assert!(parse("+1 02 03 04").is_none());
    }
}
