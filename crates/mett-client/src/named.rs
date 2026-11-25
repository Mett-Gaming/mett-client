//! Benannte Zahlen-Einstellungen des laufenden Spiels, je Spielstart einmal gesucht und von allen gemeinsam benutzt.
use crate::focus;
use gamemem::Reader;
use gamemem::options::{self, Options};
use std::collections::HashMap;
use std::sync::Mutex;

struct Found {
    pid: u32,
    options: Options,
    places: HashMap<&'static str, (u64, u64)>,
}

static FOUND: Mutex<Option<Found>> = Mutex::new(None);

/// Wer zuerst kommt, sucht; alle anderen warten auf sein Ergebnis.
pub fn options(pid: u32) -> Result<Options, &'static str> {
    with(pid, |f| f.options.clone())
}

/// Einstellung und Beschreibung.
pub fn place(pid: u32, name: &'static str) -> Result<(Options, (u64, u64)), &'static str> {
    let reader = Reader::open_for_scan(pid).map_err(|_| "Spiel nicht lesbar")?;
    with(pid, |f| {
        if !f.places.contains_key(name)
            && let Some(p) = options::locate(&reader, &f.options, name)
        {
            f.places.insert(name, p);
        }
        f.places.get(name).map(|p| (f.options.clone(), *p))
    })?
    .ok_or("Einstellungen nicht eindeutig gefunden")
}

/// Passt etwas nicht mehr, wird neu gesucht.
pub fn forget() {
    *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn with<T>(pid: u32, f: impl FnOnce(&mut Found) -> T) -> Result<T, &'static str> {
    let mut found = FOUND.lock().unwrap_or_else(|e| e.into_inner());
    if found.as_ref().is_none_or(|f| f.pid != pid) {
        *found = None;
        let file = focus::options_file().and_then(|p| std::fs::read_to_string(p).ok());
        let file = options::parse(&file.ok_or("Einstellungsdatei nicht lesbar")?);
        let image = gamemem::image_range(pid).map_err(|_| "Spiel nicht lesbar")?;
        let reader = Reader::open_for_scan(pid).map_err(|_| "Spiel nicht lesbar")?;
        let options = options::find(&reader, &image, &file).ok_or("Einstellungen nicht eindeutig gefunden")?;
        *found = Some(Found { pid, options, places: HashMap::new() });
    }
    found.as_mut().map(f).ok_or("Einstellungen nicht eindeutig gefunden")
}
