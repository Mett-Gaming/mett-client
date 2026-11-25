//! Schnittstelle zwischen Loader, Kern und Plugins (nach dem Vorbild von SKSE: erst fragen, dann laden).

pub mod chat;
pub mod cross;
pub mod panel;
pub mod style;

/// Bei jeder unverträglichen Änderung erhöhen; Plugins mit anderem Stand werden nicht geladen.
pub const API: u32 = 1;
pub const MAX_PLUGINS: usize = 16;
pub const NAME_LEN: usize = 32;
pub const MAGIC: u32 = 0x4D45_5454;

/// Was der Kern dem Plugin mitgibt.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Host {
    pub size: u32,
    pub api: u32,
    /// Programmversion des Spiels.
    pub game: [u16; 4],
    pub base: usize,
    pub image_size: usize,
}

/// Was das Plugin über sich sagt.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Info {
    pub size: u32,
    pub api: u32,
    pub version: u32,
}

/// `mett_plugin_query`: darf nichts verändern. 0 heißt „passt zu diesem Spiel“.
pub type Query = unsafe extern "system" fn(*const Host, *mut Info) -> i32;
/// `mett_plugin_load`: 0 heißt geladen.
pub type Load = unsafe extern "system" fn(*const Host) -> i32;
/// `mett_plugin_unload`: muss alles zurücknehmen.
pub type Unload = unsafe extern "system" fn();

pub mod state {
    pub const LOADED: u32 = 1;
    pub const BAD_DLL: u32 = 2;
    pub const NO_EXPORTS: u32 = 3;
    pub const WRONG_API: u32 = 4;
    /// Das Plugin passt nach eigener Prüfung nicht zu dieser Spielversion.
    pub const REFUSED: u32 = 5;
    pub const LOAD_FAILED: u32 = 6;
    pub const BAD_NAME: u32 = 7;

    pub fn text(s: u32) -> &'static str {
        match s {
            LOADED => "läuft",
            BAD_DLL => "Datei nicht ladbar",
            NO_EXPORTS => "kein Mett-Plugin",
            WRONG_API => "falsche Plugin-Version",
            REFUSED => "passt nicht zu dieser Spielversion",
            LOAD_FAILED => "Start fehlgeschlagen",
            BAD_NAME => "ungültiger Name",
            _ => "unbekannt",
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub name: [u8; NAME_LEN],
    pub state: u32,
}

/// Gemeinsamer Speicher, über den der Loader den Stand liest.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub magic: u32,
    pub api: u32,
    pub count: u32,
    pub items: [Item; MAX_PLUGINS],
}

pub fn map_name(pid: u32) -> String {
    format!("MettCore.{pid}")
}

/// Nur einfache Namen, damit nie ein Pfad aus dem Plugin-Ordner herausführt.
pub fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() < NAME_LEN && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Urteil nach `query`, ohne Windows prüfbar.
pub fn judge(answer: i32, info: &Info) -> Result<(), u32> {
    if info.size as usize != size_of::<Info>() || info.api != API {
        return Err(state::WRONG_API);
    }
    if answer == 0 { Ok(()) } else { Err(state::REFUSED) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namen() {
        assert!(valid_name("chat") && valid_name("zoom-2"));
        for bad in ["", "..", "a/b", r"a\b", "C:x", "Chat", "x.dll", &"a".repeat(32)] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn urteil() {
        let good = Info { size: size_of::<Info>() as u32, api: API, version: 1 };
        assert_eq!(judge(0, &good), Ok(()));
        assert_eq!(judge(1, &good), Err(state::REFUSED));
        assert_eq!(judge(0, &Info { api: API + 1, ..good }), Err(state::WRONG_API));
        assert_eq!(judge(0, &Info::default()), Err(state::WRONG_API));
    }
}
