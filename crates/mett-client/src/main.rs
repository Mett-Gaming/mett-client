#![windows_subsystem = "windows"]
//! Mett-Client: zeigt HUD, Menü und Chat über ein Plugin direkt in Minecraft Bedrock.
//!
//! Läuft nur mit `--launcher` (setzt der Mett Launcher). Für die Entwicklung: `--dev` startet ohne Pack und Link,
//! `--exe <Name>` hängt sich an ein anderes Programm und startet nichts.
mod app;
mod apple_link;
mod chat_keys;
mod chat_link;
mod cross_link;
mod editor;
mod focus;
mod game;
mod marker;
mod menu;
mod pack;
mod panel_link;
mod players_link;
mod plugins;
mod screen;
mod session;
mod style_link;
mod window;
mod zoom_link;

use app::App;
use std::cell::RefCell;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, Result, w};

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Verschachtelte Aufrufe (z. B. während eines Systemdialogs) werden übersprungen statt abzustürzen.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok()?.as_mut().map(f))
}

fn message(text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    unsafe { MessageBoxW(None, &HSTRING::from(text), w!("Mett-Client"), style) }
}

/// Bedrock starten, falls nötig. `None` = Abbruch, sonst ob beim Warten ein Zeitlimit gilt.
/// Klappt das nach einem Update nicht mehr, bleibt der Start von Hand möglich.
fn start_game() -> Option<bool> {
    if !gamemem::find_processes(game::EXE).unwrap_or_default().is_empty() {
        return Some(true);
    }
    if !game::is_installed() {
        let text = "Minecraft Bedrock Edition wurde nicht gefunden.\n\n\
            Ja: Microsoft Store öffnen\n\
            Nein: warten, bis du Minecraft selbst startest\n\
            Abbrechen: Mett-Client beenden";
        return match message(text, MB_YESNOCANCEL | MB_ICONWARNING) {
            IDYES => {
                let uri = w!("ms-windows-store://search/?query=Minecraft%20Bedrock");
                unsafe { ShellExecuteW(None, w!("open"), uri, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
                None
            }
            IDNO => Some(false),
            _ => None,
        };
    }
    if game::launch().is_ok() {
        return Some(true);
    }
    let text = "Minecraft ließ sich nicht automatisch starten. Bitte starte es selbst, Mett-Client wartet.";
    (message(text, MB_OKCANCEL | MB_ICONWARNING) == IDOK).then_some(false)
}

/// Pfad nur, wenn die Datei in Ordnung ist: eine kaputte wird nie überschrieben.
fn load_config() -> (config::Config, Option<std::path::PathBuf>) {
    let path = config::default_path();
    match path.as_ref().map(|p| config::load(p)) {
        Some(Ok(c)) => (c, path),
        Some(Err(e)) => {
            let text = format!("Einstellungen nicht lesbar, es gelten Standardwerte.\n\n{e}");
            message(&text, MB_OK | MB_ICONWARNING);
            (config::Config::default(), None)
        }
        None => (config::Config::default(), None),
    }
}

/// Alles von Mett gibt es nur beim Start über den Mett Launcher.
#[derive(Debug, PartialEq)]
enum Start {
    Launcher,
    /// Entwicklung: HUD ohne Pack und Link, es wird nichts installiert.
    Dev,
    Denied,
}

impl Start {
    fn from(args: &[String]) -> Self {
        let has = |name| args.iter().any(|a| a == name);
        if has("--exe") || has("--dev") {
            Start::Dev
        } else if has("--launcher") {
            Start::Launcher
        } else {
            Start::Denied
        }
    }
}

fn run() -> Result<i32> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let args: Vec<String> = std::env::args().collect();
        let custom = args.iter().position(|a| a == "--exe").and_then(|i| args.get(i + 1)).cloned();

        match args.get(1).map(String::as_str) {
            Some("--watch-start") => {
                session::start_watcher();
                return Ok(0);
            }
            Some("--watch") => return session::watch().map(|_| 0),
            _ => {}
        }
        let _mutex = CreateMutexW(None, false, session::INSTANCE)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            message("Mett-Client läuft bereits.", MB_OK | MB_ICONINFORMATION);
            return Ok(0);
        }
        let start = Start::from(&args);
        let config = load_config();
        match start {
            Start::Launcher => session::begin(),
            Start::Denied => {
                session::clean_leftovers();
                message("Bitte starte Mett-Client über den Mett Launcher.", MB_OK | MB_ICONINFORMATION);
                return Ok(0);
            }
            Start::Dev => {}
        }
        let limited = if custom.is_some() { Some(false) } else { start_game() };
        let Some(limited) = limited else { return Ok(0) };
        let hwnd = window::create()?;
        // Ins Spiel geladen wird nur beim Start über den Launcher.
        let plugins = (start == Start::Launcher).then(plugins::Plugins::new);
        let app = App::new(hwnd, custom.unwrap_or(game::EXE.into()), limited, config, plugins.clone());
        APP.with(|a| *a.borrow_mut() = Some(app));
        window::start(hwnd)?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            DispatchMessageW(&msg);
        }
        window::stop(hwnd);
        with_app(|a| a.close_editor());
        let app = APP.with(|a| a.borrow_mut().take());
        let (timed_out, save_failed) = app.as_ref().map_or((false, false), |a| (a.timed_out, a.save_failed));
        // Erst die Verbindungen schließen, dann die Plugins aus dem Spiel nehmen.
        drop(app);
        if let Some(p) = plugins {
            p.stop();
        }
        if save_failed {
            message("Die Einstellungen konnten nicht gespeichert werden.", MB_OK | MB_ICONWARNING);
        }
        if timed_out {
            message("Minecraft ist nicht gestartet. Mett-Client wird beendet.", MB_OK | MB_ICONWARNING);
        }
        Ok(msg.wParam.0 as i32)
    }
}

fn main() {
    let code = run().unwrap_or_else(|e| {
        message(&format!("Mett-Client konnte nicht starten:\n{e}"), MB_OK | MB_ICONERROR);
        1
    });
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_launcher_startet_nichts() {
        let from = |a: &[&str]| Start::from(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(from(&["x.exe"]), Start::Denied);
        assert_eq!(from(&["x.exe", "--launcher"]), Start::Launcher);
        assert_eq!(from(&["x.exe", "--dev"]), Start::Dev);
        assert_eq!(from(&["x.exe", "--irgendwas"]), Start::Denied);
        // Entwicklung installiert nie etwas, auch nicht zusammen mit --launcher.
        assert_eq!(from(&["x.exe", "--launcher", "--dev"]), Start::Dev);
    }
}
