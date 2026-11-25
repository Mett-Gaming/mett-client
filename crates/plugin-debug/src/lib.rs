//! Nur für die Entwicklung, wird nie ausgeliefert. Beobachtet, in welchem Thread das Spiel seine Fensternachrichten
//! verarbeitet. Verändert keinen Spielcode: Windows ruft uns beim Abholen einer eigenen Nachricht auf.
use mett_plugin::{API, Host, Info};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering::SeqCst};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

/// Auftrag von außen: Schritt (1 bereit, 2 läuft, 3 fertig, 4 abgelehnt), Funktion, Modell, Rückgabe.
/// Adressen gelten nur für diesen Prozess und werden unmittelbar vorher ermittelt.
#[unsafe(no_mangle)]
pub static METT_ORDER: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static IMAGE: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
/// Höchstens eine Nachricht je Laden des Plugins.
static USED: AtomicBool = AtomicBool::new(false);
const TEXT: &[u8] = b"METTPLUGIN1";

/// Kurze Zeichenkette im Aufbau von MSVC.
#[repr(C)]
struct ShortString {
    text: [u8; 16],
    len: u64,
    cap: u64,
}

fn readable(at: u64, size: usize) -> bool {
    let mut m = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(at as *const _), &mut m, size_of::<MEMORY_BASIC_INFORMATION>()) };
    let end = m.BaseAddress as u64 + m.RegionSize as u64;
    n != 0
        && at.is_multiple_of(8)
        && m.State == MEM_COMMIT
        && matches!(m.Protect, PAGE_READWRITE | PAGE_READONLY | PAGE_EXECUTE_READ)
        && at + size as u64 <= end
}

/// Alles noch einmal prüfen, erst dann aufrufen.
unsafe fn run_order() {
    let step = &METT_ORDER[0];
    if step.compare_exchange(1, 2, SeqCst, SeqCst).is_err() {
        return;
    }
    let (f, model) = (METT_ORDER[1].load(SeqCst), METT_ORDER[2].load(SeqCst));
    let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
    let inside = |p: u64| p >= base && p < base + size;
    let ok = unsafe {
        let get = |p: u64| *(p as *const u64);
        !USED.swap(true, SeqCst)
            && GetCurrentThreadId() == METT_DEBUG[2].load(SeqCst)
            && inside(f)
            && readable(model, 0x60)
            && inside(get(model))
            && readable(get(model + 0x48), 8)
            && get(get(model + 0x48)) & 0xff == 1
            && readable(get(model + 0x58), 8)
            && inside(get(get(model + 0x58)))
    };
    if !ok {
        step.store(4, SeqCst);
        return;
    }
    let mut s = ShortString { text: [0; 16], len: TEXT.len() as u64, cap: 15 };
    s.text[..TEXT.len()].copy_from_slice(TEXT);
    type SendChat = unsafe extern "system" fn(u64, *const ShortString) -> u8;
    let send = unsafe { std::mem::transmute::<usize, SendChat>(f as usize) };
    METT_ORDER[3].store(unsafe { send(model, &s) } as u64, SeqCst);
    step.store(3, SeqCst);
}

type Keeper = (std::sync::mpsc::Sender<()>, std::thread::JoinHandle<()>);
static KEEPER: Mutex<Option<Keeper>> = Mutex::new(None);
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static MESSAGE: AtomicU32 = AtomicU32::new(0);
/// Zum Auslesen von außen: Aufrufe mit unserer Nachricht, Thread des Aufrufs, Thread des Fensters,
/// Rückrufe insgesamt, letzter Wert für „Nachricht entfernen“, angenommene eigene Nachrichten,
/// Nummer der Nachricht (kann sich von der in anderen Prozessen unterscheiden), Fenster.
#[unsafe(no_mangle)]
pub static METT_DEBUG: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];

/// Spielfenster des eigenen Prozesses und sein Thread.
fn game_window() -> Option<(HWND, u32)> {
    unsafe {
        let mut after = None;
        loop {
            let h = FindWindowExW(None, after, w!("Bedrock"), None).ok()?;
            let mut pid = 0;
            let thread = GetWindowThreadProcessId(h, Some(&mut pid));
            if pid == GetCurrentProcessId() {
                return Some((h, thread));
            }
            after = Some(h);
        }
    }
}

unsafe extern "system" fn on_message(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        METT_DEBUG[3].fetch_add(1, SeqCst);
        if code >= 0 {
            let m = &*(lp.0 as *const CWPSTRUCT);
            if m.message == MESSAGE.load(SeqCst) && m.message != 0 && m.hwnd.0 as isize == WINDOW.load(SeqCst) {
                METT_DEBUG[0].fetch_add(1, SeqCst);
                METT_DEBUG[1].store(GetCurrentThreadId(), SeqCst);
                run_order();
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

/// # Safety
/// Wird nur vom Kern mit gültigen Zeigern aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_query(host: *const Host, info: *mut Info) -> i32 {
    unsafe {
        *info = Info { size: size_of::<Info>() as u32, api: API, version: 1 };
        // Nur im Spiel und nur, wenn das Fenster eindeutig da ist.
        if (*host).api == API && game_window().is_some() { 0 } else { 1 }
    }
}

/// # Safety
/// Wird nur vom Kern mit gültigem Zeiger aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_load(host: *const Host) -> i32 {
    unsafe {
        IMAGE[0].store((*host).base as u64, SeqCst);
        IMAGE[1].store((*host).image_size as u64, SeqCst);
        let Some((window, thread)) = game_window() else { return 1 };
        MESSAGE.store(RegisterWindowMessageW(w!("MettDebug")), SeqCst);
        WINDOW.store(window.0 as isize, SeqCst);
        // Windows entfernt einen Hook, sobald der Thread endet, der ihn eingerichtet hat. Deshalb hält ihn ein eigener.
        let (done, ready) = std::sync::mpsc::channel();
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let keeper = std::thread::spawn(move || {
            // Eigener Prozess: ohne Modul, sonst ruft Windows uns nicht auf.
            match SetWindowsHookExW(WH_CALLWNDPROC, Some(on_message), None, thread) {
                Ok(h) => {
                    let _ = done.send(true);
                    let _ = wait.recv();
                    let _ = UnhookWindowsHookEx(h);
                }
                Err(_) => {
                    let _ = done.send(false);
                }
            }
        });
        if ready.recv() != Ok(true) {
            let _ = keeper.join();
            return 3;
        }
        *KEEPER.lock().unwrap_or_else(|e| e.into_inner()) = Some((stop, keeper));
        METT_DEBUG[6].store(MESSAGE.load(SeqCst), SeqCst);
        METT_DEBUG[7].store(window.0 as u32, SeqCst);
        METT_DEBUG[2].store(thread, SeqCst);
        for _ in 0..3 {
            // Gesendet statt eingereiht: Beim Spielen holt Minecraft eingereihte Nachrichten dieser Art nicht ab.
            let sent =
                SendMessageTimeoutW(window, MESSAGE.load(SeqCst), WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 1000, None);
            if sent.0 != 0 {
                METT_DEBUG[5].fetch_add(1, SeqCst);
            }
        }
        0
    }
}

/// Stößt einen bereitliegenden Auftrag im Hauptthread an. Von einem anderen Prozess gesendet kam er dort nicht an.
#[unsafe(no_mangle)]
pub extern "system" fn mett_debug_go(_: *const u16) -> u32 {
    let window = HWND(WINDOW.load(SeqCst) as *mut _);
    let message = MESSAGE.load(SeqCst);
    let sent = unsafe { SendMessageTimeoutW(window, message, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 3000, None) };
    (sent.0 != 0) as u32
}

#[unsafe(no_mangle)]
pub extern "system" fn mett_plugin_unload() {
    unsafe {
        if let Some((stop, keeper)) = KEEPER.lock().unwrap_or_else(|e| e.into_inner()).take() {
            drop(stop);
            let _ = keeper.join();
        }
        // Warten, bis der Hauptthread sicher nicht mehr in unserem Code ist. Sonst bleibt die DLL geladen.
        let window = HWND(WINDOW.load(SeqCst) as *mut _);
        let done = SendMessageTimeoutW(window, WM_NULL, WPARAM(0), LPARAM(0), SMTO_BLOCK, 3000, None);
        if done.0 == 0 {
            let mut pinned = HMODULE::default();
            let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
            let _ = GetModuleHandleExW(flags, PCWSTR(on_message as *const () as *const u16), &mut pinned);
        }
    }
}
