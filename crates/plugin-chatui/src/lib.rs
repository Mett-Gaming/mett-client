//! Verschiebt Minecrafts eigene Chat-Anzeige und stellt sie wieder zurück. Geschrieben wird nur im Hauptthread des
//! Spiels, dort lebt die Oberfläche. Endet der Client, kommt alles von selbst zurück.
mod cursor;
mod follow;
mod panel;
mod style;
mod values;
mod width;

use cursor::Cursor;
use mett_plugin::panel::{Order, Wish, answer};
use mett_plugin::{API, Host, Info};
use panel::Target;
use std::panic::catch_unwind;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering::SeqCst};
use style::Style;
use values::Value;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

static IMAGE: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);
static MESSAGE: AtomicU32 = AtomicU32::new(0);
static TARGET: Mutex<Option<Target>> = Mutex::new(None);
static STYLE: Mutex<Option<Style>> = Mutex::new(None);
static CURSOR: Mutex<Option<Cursor>> = Mutex::new(None);
static VALUES: Mutex<[Option<Value>; mett_plugin::values::ALLOWED.len()]> = Mutex::new([None]);
static KEEPER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ein Schritt im Hauptthread.
fn step(restore: bool) {
    style_step(restore);
    panel_step(restore);
    cursor_step(restore);
    value_step(restore);
}

fn value_step(restore: bool) {
    for slot in lock(&VALUES).iter_mut() {
        let Some(v) = slot else { continue };
        if restore {
            v.order.target = None;
        }
        if !v.step() || v.order.target.is_none() {
            *slot = None;
        }
    }
}

fn cursor_step(restore: bool) {
    let mut cursor = lock(&CURSOR);
    let Some(c) = &mut *cursor else { return };
    if restore {
        c.order.hide = false;
    }
    if !c.step() || !c.order.hide {
        *cursor = None;
    }
}

fn style_step(restore: bool) {
    let mut style = lock(&STYLE);
    let Some(s) = &mut *style else { return };
    if restore {
        s.order.on = false;
    }
    if !s.step() || !s.order.on {
        *style = None;
    }
}

fn panel_step(restore: bool) {
    let mut target = lock(&TARGET);
    let Some(t) = &mut *target else { return };
    if restore {
        t.order.wish = Wish::Release;
    }
    // Passt etwas nicht mehr, wird das Element nie wieder angefasst.
    let image = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
    if !t.step(image) || t.order.wish == Wish::Release {
        *target = None;
    }
}

/// Rückruf des Spiels. Ist das Ziel gerade gesperrt, erledigt es der nächste Schritt.
pub fn height_changed() {
    if unsafe { GetCurrentThreadId() } == THREAD.load(SeqCst)
        && let Ok(mut target) = TARGET.try_lock()
        && let Some(t) = &mut *target
    {
        t.height_changed();
    }
}

unsafe extern "system" fn on_message(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let m = &*(lp.0 as *const CWPSTRUCT);
            let ours = m.message == MESSAGE.load(SeqCst) && m.message != 0 && m.hwnd.0 as isize == WINDOW.load(SeqCst);
            if ours && GetCurrentThreadId() == THREAD.load(SeqCst) {
                let restore = m.wParam.0 == 1;
                let _ = catch_unwind(|| step(restore));
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

/// Einen Schritt auslösen und abwarten. Nie mit gesperrtem Ziel aufrufen.
fn tell(restore: bool) {
    let window = HWND(WINDOW.load(SeqCst) as *mut _);
    let (m, w) = (MESSAGE.load(SeqCst), WPARAM(restore as usize));
    unsafe {
        SendMessageTimeoutW(window, m, w, LPARAM(0), SMTO_ABORTIFHUNG, 1000, None);
    }
}

fn accept(o: Order) -> u32 {
    let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
    if !(base..base + size).contains(&o.table) {
        return answer::BAD_ORDER;
    }
    {
        let mut target = lock(&TARGET);
        match &mut *target {
            Some(t) if (t.order.at, t.order.parent) == (o.at, o.parent) => t.order = o,
            // Erst muss das alte Element zurückgestellt sein.
            Some(t) => {
                t.order.wish = Wish::Release;
                return answer::STALE;
            }
            None if o.wish == Wish::Release => return answer::OK,
            None => *target = Some(Target::new(o)),
        }
    }
    tell(false);
    if lock(&TARGET).is_some() || o.wish == Wish::Release { answer::OK } else { answer::STALE }
}

/// # Safety
/// `text` zeigt auf nullterminierten UTF-16-Text.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_panel_order(text: *const u16) -> u32 {
    catch_unwind(|| {
        if text.is_null() || STOP.load(SeqCst) {
            return answer::BAD_ORDER;
        }
        let text = unsafe { PCWSTR(text).to_string() }.unwrap_or_default();
        Order::decode(&text).map_or(answer::BAD_ORDER, accept)
    })
    .unwrap_or(answer::BAD_ORDER)
}

/// # Safety
/// `text` zeigt auf nullterminierten UTF-16-Text.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_style_order(text: *const u16) -> u32 {
    catch_unwind(|| {
        if text.is_null() || STOP.load(SeqCst) {
            return answer::BAD_ORDER;
        }
        let text = unsafe { PCWSTR(text).to_string() }.unwrap_or_default();
        let Some(o) = mett_plugin::style::Order::decode(&text) else { return answer::BAD_ORDER };
        let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
        if !(base..base + size).contains(&o.table) {
            return answer::BAD_ORDER;
        }
        {
            let mut style = lock(&STYLE);
            match &mut *style {
                // Der gemerkte Wert des Nutzers bleibt.
                Some(s) if s.order.options == o.options => s.order = o,
                Some(_) => return answer::STALE,
                None if !o.on => return answer::OK,
                None => *style = Some(Style::new(o)),
            }
        }
        tell(false);
        if lock(&STYLE).is_some() == o.on { answer::OK } else { answer::STALE }
    })
    .unwrap_or(answer::BAD_ORDER)
}

/// # Safety
/// `text` zeigt auf nullterminierten UTF-16-Text.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_cursor_order(text: *const u16) -> u32 {
    catch_unwind(|| {
        if text.is_null() || STOP.load(SeqCst) {
            return answer::BAD_ORDER;
        }
        let text = unsafe { PCWSTR(text).to_string() }.unwrap_or_default();
        let Some(o) = mett_plugin::cross::Order::decode(&text) else { return answer::BAD_ORDER };
        let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
        if !(base..base + size).contains(&o.table) {
            return answer::BAD_ORDER;
        }
        {
            let mut cursor = lock(&CURSOR);
            match &mut *cursor {
                Some(c) if (c.order.at, c.order.parent) == (o.at, o.parent) => c.order = o,
                // Erst muss das alte Element zurückgestellt sein.
                Some(c) => {
                    c.order.hide = false;
                    return answer::STALE;
                }
                None if !o.hide => return answer::OK,
                None => *cursor = Some(Cursor::new(o)),
            }
        }
        tell(false);
        if lock(&CURSOR).is_some() == o.hide { answer::OK } else { answer::STALE }
    })
    .unwrap_or(answer::BAD_ORDER)
}

/// # Safety
/// `text` zeigt auf nullterminierten UTF-16-Text.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_value_order(text: *const u16) -> u32 {
    catch_unwind(|| {
        if text.is_null() || STOP.load(SeqCst) {
            return answer::BAD_ORDER;
        }
        let text = unsafe { PCWSTR(text).to_string() }.unwrap_or_default();
        let Some(o) = mett_plugin::values::Order::decode(&text) else { return answer::BAD_ORDER };
        let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
        if !(base..base + size).contains(&o.table) {
            return answer::BAD_ORDER;
        }
        {
            let slot = &mut lock(&VALUES)[o.which];
            match slot {
                // Der gemerkte Wert des Nutzers bleibt.
                Some(v) if (v.order.at, v.order.info) == (o.at, o.info) => v.order = o,
                Some(v) => {
                    v.order.target = None;
                    return answer::STALE;
                }
                None if o.target.is_none() => return answer::OK,
                None => *slot = Some(Value::new(o)),
            }
        }
        tell(false);
        if lock(&VALUES)[o.which].is_some() == o.target.is_some() { answer::OK } else { answer::STALE }
    })
    .unwrap_or(answer::BAD_ORDER)
}

/// Läuft der Client noch, der diesen Auftrag gab?
fn client_alive(watched: &mut Option<(u32, HANDLE)>, pid: u32) -> bool {
    unsafe {
        if watched.as_ref().is_none_or(|w| w.0 != pid) {
            if let Some((_, h)) = watched.take() {
                let _ = CloseHandle(h);
            }
            *watched = OpenProcess(PROCESS_SYNCHRONIZE, false, pid).ok().map(|h| (pid, h));
        }
        watched.as_ref().is_some_and(|w| WaitForSingleObject(w.1, 0) == WAIT_TIMEOUT)
    }
}

fn keep(hook: HHOOK) {
    let mut client = None;
    while !STOP.load(SeqCst) {
        let pid = lock(&TARGET).as_ref().map(|t| t.order.client);
        let pid = pid.or_else(|| lock(&STYLE).as_ref().map(|s| s.order.client));
        let pid = pid.or_else(|| lock(&CURSOR).as_ref().map(|c| c.order.client));
        let pid = pid.or_else(|| lock(&VALUES).iter().flatten().next().map(|v| v.order.client));
        if let Some(pid) = pid {
            tell(!client_alive(&mut client, pid));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    tell(true);
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
        if let Some((_, h)) = client {
            let _ = CloseHandle(h);
        }
    }
}

fn game_window() -> Option<(HWND, u32)> {
    let (mut after, mut found) = (None, None);
    unsafe {
        while let Ok(h) = FindWindowExW(None, after, w!("Bedrock"), None) {
            let mut pid = 0;
            let thread = GetWindowThreadProcessId(h, Some(&mut pid));
            if pid == GetCurrentProcessId() && found.replace((h, thread)).is_some() {
                return None;
            }
            after = Some(h);
        }
    }
    found
}

/// # Safety
/// Wird nur vom Kern mit gültigen Zeigern aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_query(host: *const Host, info: *mut Info) -> i32 {
    unsafe {
        *info = Info { size: size_of::<Info>() as u32, api: API, version: 1 };
        if (*host).api == API && game_window().is_some() { 0 } else { 1 }
    }
}

/// # Safety
/// Wird nur vom Kern mit gültigem Zeiger aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_load(host: *const Host) -> i32 {
    unsafe {
        let Some((window, thread)) = game_window() else { return 1 };
        IMAGE[0].store((*host).base as u64, SeqCst);
        IMAGE[1].store((*host).image_size as u64, SeqCst);
        WINDOW.store(window.0 as isize, SeqCst);
        THREAD.store(thread, SeqCst);
        MESSAGE.store(RegisterWindowMessageW(w!("MettChatUi")), SeqCst);
        STOP.store(false, SeqCst);
        let (done, ready) = std::sync::mpsc::channel();
        // Der Hook gehört dem Thread, der ihn einrichtet.
        let keeper =
            std::thread::spawn(move || match SetWindowsHookExW(WH_CALLWNDPROC, Some(on_message), None, thread) {
                Ok(h) => {
                    let _ = done.send(true);
                    keep(h);
                }
                Err(_) => {
                    let _ = done.send(false);
                }
            });
        if ready.recv() != Ok(true) {
            let _ = keeper.join();
            return 2;
        }
        *lock(&KEEPER) = Some(keeper);
        0
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn mett_plugin_unload() {
    STOP.store(true, SeqCst);
    if let Some(k) = lock(&KEEPER).take() {
        let _ = k.join();
    }
    unsafe {
        // Läuft noch ein Aufruf, bleibt die DLL stehen.
        let window = HWND(WINDOW.load(SeqCst) as *mut _);
        let done = SendMessageTimeoutW(window, WM_NULL, WPARAM(0), LPARAM(0), SMTO_BLOCK, 3000, None);
        // Ein Ziel, das nicht zurückgestellt wurde, kann noch Rückrufe haben.
        if done.0 == 0
            || lock(&TARGET).is_some()
            || lock(&STYLE).is_some()
            || lock(&CURSOR).is_some()
            || lock(&VALUES).iter().any(Option::is_some)
        {
            let mut pinned = HMODULE::default();
            let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
            let _ = GetModuleHandleExW(flags, PCWSTR(on_message as *const () as *const u16), &mut pinned);
        }
    }
}
