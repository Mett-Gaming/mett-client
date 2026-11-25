//! Kern im Spielprozess: lädt eingeschaltete Plugins und nimmt sie wieder zurück. Selbst ohne Hooks und Threads.
//! Beim Laden der DLL passiert nichts; erst `mett_core_start` arbeitet.
mod start;

use std::panic::catch_unwind;

/// Anzahl geladener Plugins oder `u32::MAX` bei einem Fehler.
///
/// # Safety
/// `enabled` zeigt auf nullterminierten UTF-16-Text.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_core_start(enabled: *const u16) -> u32 {
    catch_unwind(|| {
        if enabled.is_null() {
            return u32::MAX;
        }
        let text = unsafe { windows::core::PCWSTR(enabled).to_string() }.unwrap_or_default();
        start::start(&text).unwrap_or(u32::MAX)
    })
    .unwrap_or(u32::MAX)
}

#[unsafe(no_mangle)]
pub extern "system" fn mett_core_stop(_: *const u16) -> u32 {
    catch_unwind(start::stop).map_or(u32::MAX, |_| 0)
}
