//! Lädt Plugins ins Spiel, nur beim Start über den Launcher, und nimmt am Ende alle zurück.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Fehlernummer von Windows beim letzten misslungenen Laden, für die Diagnose.
static LAST: Mutex<Option<(String, u32)>> = Mutex::new(None);

pub fn report() -> String {
    match &*LAST.lock().unwrap_or_else(|e| e.into_inner()) {
        None => "keiner".into(),
        Some((name, code)) => format!("{name} 0x{code:08X}"),
    }
}

#[derive(Clone)]
pub struct Plugins {
    dir: Option<PathBuf>,
    /// Spiel, in dem unser Kern liegt.
    loaded: Arc<Mutex<Option<u32>>>,
}

impl Plugins {
    pub fn new() -> Self {
        let dir = std::env::current_exe().ok().and_then(|p| Some(p.parent()?.to_path_buf()));
        Self { dir, loaded: Arc::default() }
    }

    pub fn path(&self, name: &str) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join("plugins").join(format!("{name}.dll")))
    }

    pub fn load(&self, pid: u32, name: &str) -> Result<(), &'static str> {
        let core = self.dir.as_ref().ok_or("eigener Ordner unbekannt")?.join("mett_core.dll");
        if !core.is_file() || !self.path(name).is_some_and(|p| p.is_file()) {
            return Err("Plugin-Dateien fehlen");
        }
        // Immer nur einer lädt.
        let mut loaded = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        *loaded = Some(pid);
        mett_inject::start(pid, &core, &[name]).map_err(|e| {
            *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((name.into(), e.code().0 as u32));
            "Plugin ließ sich nicht laden"
        })?;
        match mett_inject::status(pid).ok().and_then(|s| s.into_iter().find(|p| p.0 == name)) {
            Some((_, mett_plugin::state::LOADED)) => Ok(()),
            Some((_, s)) => Err(mett_plugin::state::text(s)),
            None => Err("Plugin nicht gestartet"),
        }
    }

    /// Plugins und Kern verlassen das Spiel. Ist es schon zu, gibt es nichts zu tun.
    pub fn stop(&self) {
        let pid = self.loaded.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let (Some(pid), Some(dir)) = (pid, &self.dir) {
            let _ = mett_inject::stop(pid, &dir.join("mett_core.dll"));
        }
    }
}
