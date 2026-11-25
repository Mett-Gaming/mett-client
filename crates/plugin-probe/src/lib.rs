//! Leeres Plugin zum Prüfen des Loaders. Verändert nichts am Spiel.
use mett_plugin::{API, Host, Info};

/// # Safety
/// Wird nur vom Kern mit gültigen Zeigern aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_query(host: *const Host, info: *mut Info) -> i32 {
    unsafe {
        *info = Info { size: size_of::<Info>() as u32, api: API, version: 1 };
        // Zum Prüfen der Ablehnung.
        if std::env::var_os("METT_PROBE_REFUSE").is_some() || (*host).api != API { 1 } else { 0 }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn mett_plugin_load(_: *const Host) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub extern "system" fn mett_plugin_unload() {}
