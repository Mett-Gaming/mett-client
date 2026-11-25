//! Launcher-Sitzung: Das Pack ist nur für die Dauer des Spiels da.
use crate::{focus, game, pack};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::Duration;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::*;
use windows::core::{PCWSTR, Result, w};

pub const INSTANCE: PCWSTR = w!("MettClient.Instanz");
const DETACHED: u32 = 0x8; // DETACHED_PROCESS

fn game_running() -> bool {
    // Im Zweifel ja: dann wird nichts unter dem laufenden Spiel weggeräumt.
    gamemem::find_processes(game::EXE).map_or(true, |p| !p.is_empty())
}

/// Vor dem Spielstart. Ein Fehler kostet nur den Knopf, nie das HUD.
pub fn begin() {
    if game_running() {
        return;
    }
    // Reste einer früheren Sitzung zurückstellen, sonst hielten wir unsere eigenen Werte für die des Nutzers.
    cleanup();
    // Erst der Aufräumer, dann die Änderungen: so bleibt nie etwas ohne Aufsicht liegen.
    if spawn("--watch-start").is_err() {
        return;
    }
    if pack::install().is_err() {
        let _ = pack::remove();
    }
    focus::force();
}

/// Reste einer abgebrochenen Sitzung entfernen.
pub fn clean_leftovers() {
    if !game_running() {
        cleanup();
    }
}

fn spawn(arg: &str) -> std::io::Result<()> {
    Command::new(std::env::current_exe()?).arg(arg).creation_flags(DETACHED).spawn()?.wait()?;
    Ok(())
}

/// Zwischenprozess: Der Aufräumer gehört danach nicht mehr zum Prozessbaum, den der Launcher beenden kann.
pub fn start_watcher() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("--watch").creation_flags(DETACHED).spawn();
    }
}

/// Wartet, bis weder Mett-Client noch Minecraft läuft, und räumt dann auf.
pub fn watch() -> Result<()> {
    unsafe {
        // Immer nur ein Aufräumer arbeitet, weitere warten.
        let m = CreateMutexW(None, false, w!("MettClient.Aufraeumen"))?;
        WaitForSingleObject(m, INFINITE);
        loop {
            std::thread::sleep(Duration::from_secs(2));
            if !client_running() && game_running() {
                focus::untrack_zoom();
            }
            if !client_running() && !game_running() {
                // Beim Beenden schreibt Minecraft noch Einstellungen; kurz warten und neu prüfen.
                std::thread::sleep(Duration::from_secs(3));
                if !client_running() && !game_running() {
                    cleanup();
                    break;
                }
            }
        }
        let _ = ReleaseMutex(m);
        CloseHandle(m)
    }
}

fn client_running() -> bool {
    unsafe {
        match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, INSTANCE) {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => false,
        }
    }
}

fn cleanup() {
    let _ = pack::remove();
    focus::restore();
}
