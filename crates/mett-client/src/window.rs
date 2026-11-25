//! Unsichtbares Fenster für Nachrichten und das Tray-Symbol. Über dem Spiel liegt nichts.
use crate::{App, with_app};
use std::path::Path;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, Result, w};

use windows::Win32::System::{DataExchange::*, Memory::*, Ole::CF_UNICODETEXT};

const TICK_MS: u32 = 50;
const WM_TRAY: u32 = WM_APP + 1;
const CMD_TOGGLE: usize = 1;
const CMD_QUIT: usize = 2;
pub fn create() -> Result<HWND> {
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("MettClient");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let ex = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
        let title = w!("Mett-Client");
        CreateWindowExW(ex, class, title, WS_POPUP, 0, 0, 1, 1, None, None, Some(instance), None)
    }
}

/// Eingaben, Takt und Tray einschalten.
pub fn start(hwnd: HWND) -> Result<()> {
    unsafe {
        let device =
            |usage| RAWINPUTDEVICE { usUsagePage: 1, usUsage: usage, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
        RegisterRawInputDevices(&[device(2), device(6)], size_of::<RAWINPUTDEVICE>() as u32)?;
        let event = EVENT_SYSTEM_FOREGROUND;
        SetWinEventHook(event, event, None, Some(foreground_changed), 0, 0, WINEVENT_OUTOFCONTEXT);
        SetTimer(Some(hwnd), 1, TICK_MS, None);
    }
    tray(hwnd, NIM_ADD);
    Ok(())
}

pub fn stop(hwnd: HWND) {
    tray(hwnd, NIM_DELETE);
    unsafe {
        let _ = KillTimer(Some(hwnd), 1);
    }
}

pub fn clipboard() -> Option<String> {
    unsafe {
        OpenClipboard(None).ok()?;
        let text = GetClipboardData(CF_UNICODETEXT.0 as u32).ok().and_then(|h| {
            let h = HGLOBAL(h.0);
            let p = GlobalLock(h).cast::<u16>();
            if p.is_null() {
                return None;
            }
            let room = GlobalSize(h) / 2;
            let s = std::slice::from_raw_parts(p, room);
            let t = String::from_utf16_lossy(s.split(|c| *c == 0).next().unwrap_or_default());
            let _ = GlobalUnlock(h);
            Some(t)
        });
        let _ = CloseClipboard();
        text
    }
}

pub fn set_clipboard(hwnd: HWND, text: &str) -> Result<()> {
    let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe {
        OpenClipboard(Some(hwnd))?;
        let done = (|| {
            EmptyClipboard()?;
            let h = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2)?;
            let p = GlobalLock(h).cast::<u16>();
            if p.is_null() {
                return Err(windows::core::Error::from_thread());
            }
            p.copy_from_nonoverlapping(text.as_ptr(), text.len());
            let _ = GlobalUnlock(h);
            // Ab hier gehört der Speicher Windows.
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(h.0))).map(|_| ())
        })();
        let _ = CloseClipboard();
        done
    }
}

/// Ordner zum Teilen von Profilen neben der Einstellungsdatei.
pub fn open_folder(config: Option<&Path>) {
    let Some(dir) = config.and_then(|p| p.parent()).map(|p| p.join("Profile")) else { return };
    let _ = std::fs::create_dir_all(&dir);
    let dir = HSTRING::from(dir.as_path());
    unsafe { ShellExecuteW(None, w!("open"), &dir, None, None, SW_SHOWNORMAL) };
}

fn read_input(lparam: LPARAM) -> Option<RAWINPUT> {
    let mut raw = RAWINPUT::default();
    let mut size = size_of::<RAWINPUT>() as u32;
    let header = size_of::<RAWINPUTHEADER>() as u32;
    let handle = HRAWINPUT(lparam.0 as *mut _);
    let n = unsafe { GetRawInputData(handle, RID_INPUT, Some((&raw mut raw).cast()), &mut size, header) };
    (n != u32::MAX && n > 0).then_some(raw)
}

/// Stand der Anzeige am Tray-Symbol.
pub fn update_tray(hwnd: HWND) {
    // Nicht aus dem Programmzustand heraus aufrufen: Der Text kommt von dort.
    tray(hwnd, NIM_MODIFY);
}

fn tray(hwnd: HWND, action: NOTIFY_ICON_MESSAGE) {
    let mut d = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default(),
        ..Default::default()
    };
    let tip = with_app(|a| a.status()).unwrap_or_else(|| "Mett-Client".into());
    let room = d.szTip.len() - 1;
    for (to, from) in d.szTip.iter_mut().zip(tip.encode_utf16().take(room)) {
        *to = from;
    }
    unsafe {
        let _ = Shell_NotifyIconW(action, &d);
    }
}

fn tray_menu(hwnd: HWND) -> Result<()> {
    unsafe {
        let on = with_app(|a| a.hud_on).unwrap_or(true);
        let menu = CreatePopupMenu()?;
        let check = if on { MF_CHECKED } else { MF_UNCHECKED };
        AppendMenuW(menu, MF_STRING | check, CMD_TOGGLE, &HSTRING::from(hud::lang::t("HUD anzeigen")))?;
        AppendMenuW(menu, MF_STRING, CMD_QUIT, &HSTRING::from(hud::lang::t("Mett-Client beenden")))?;
        let mut p = POINT::default();
        GetCursorPos(&mut p)?;
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, p.x, p.y, None, hwnd, None);
        DestroyMenu(menu)?;
        match cmd.0 as usize {
            CMD_TOGGLE => {
                with_app(|a| a.hud_on = !a.hud_on);
            }
            CMD_QUIT => PostQuitMessage(0),
            _ => {}
        }
    }
    Ok(())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            with_app(App::tick);
            if with_app(App::status_changed) == Some(true) {
                update_tray(hwnd);
            }
        }
        WM_INPUT => {
            if let Some(raw) = read_input(lp) {
                with_app(|a| a.input(&raw));
            }
            return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
        }
        crate::screen::WM_EVENTS => {
            with_app(App::events);
        }
        WM_TRAY => {
            if matches!(lp.0 as u32, WM_RBUTTONUP | WM_LBUTTONUP) {
                let _ = tray_menu(hwnd);
            }
        }
        WM_CLOSE => unsafe { PostQuitMessage(0) },
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}

/// Läuft im eigenen Programm (out-of-context), es landet kein Code im Spiel.
unsafe extern "system" fn foreground_changed(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    with_app(App::tick);
}
