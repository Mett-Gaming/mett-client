//! Eingaben: sitzt vor der Fensterfunktion des Spiels. Solange etwas von uns offen ist, erreichen Maus und Tasten das
//! Spiel nicht. Üblicher Windows-Weg (Fensterfunktion ersetzen), kein veränderter Spielcode. Nichts wird gespeichert.
use crate::link;
use hudlink::{Event, control};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::SeqCst};
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static OLD: AtomicIsize = AtomicIsize::new(0);
static INSIDE: AtomicU32 = AtomicU32::new(0);
static CHAT_OPEN: AtomicBool = AtomicBool::new(false);
/// Zuletzt gemeldeter Stand, damit der Fenster-Thread bei Änderungen den Zeiger anpasst.
static LAST: AtomicBool = AtomicBool::new(false);
/// Es wird aus mehreren Threads gemeldet.
/// Zuletzt gesehenes Zeigerbild.
static SHAPE: AtomicU32 = AtomicU32::new(0);
static PUSH: Mutex<()> = Mutex::new(());
/// Vorheriger Zeiger: Grenze, wie oft wir ihn sichtbar gemacht haben, Bild.
static CURSOR: Mutex<Option<Cursor>> = Mutex::new(None);

struct Cursor {
    clip: Option<RECT>,
    shown: u32,
    /// Vorheriges Zeigerbild. Minecraft setzt es nach unserem nicht von selbst neu.
    image: isize,
}
static KEYS: Mutex<Keys> =
    Mutex::new(Keys { swallowed: [false; 256], buttons: 0, skip_char: false, tab: false, zoom: 0, high: None });

struct Keys {
    /// Von uns verschluckt gedrückt; nur deren Loslassen wird auch verschluckt.
    swallowed: [bool; 256],
    buttons: u8,
    /// Das Zeichen der Öffnen-Taste gehört nicht in den Text.
    skip_char: bool,
    /// Wir zeigen gerade das Tab-Menü.
    tab: bool,
    /// Gehaltene Zoom-Taste, 0 keine.
    zoom: usize,
    /// Erste Hälfte eines Zeichens außerhalb von 16 Bit.
    high: Option<u16>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sichtbares Spielfenster dieses Prozesses. Es muss genau eines geben.
fn game_window() -> Option<HWND> {
    let (mut after, mut found) = (None, None);
    unsafe {
        while let Ok(h) = FindWindowExW(None, after, w!("Bedrock"), None) {
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let ours = pid == GetCurrentProcessId() && IsWindowVisible(h).as_bool();
            if ours && found.replace(h).is_some() {
                return None;
            }
            after = Some(h);
        }
    }
    found
}

pub fn install() -> bool {
    let Some(h) = game_window() else { return false };
    CHAT_OPEN.store(false, SeqCst);
    LAST.store(false, SeqCst);
    unsafe {
        // Erst das Original merken, dann tauschen.
        let old = GetWindowLongPtrW(h, GWLP_WNDPROC);
        if old == 0 {
            return false;
        }
        OLD.store(old, SeqCst);
        WINDOW.store(h.0 as isize, SeqCst);
        SetWindowLongPtrW(h, GWLP_WNDPROC, wndproc as *const () as isize) != 0
    }
}

/// `false`: Jemand sitzt inzwischen vor uns oder ein Aufruf läuft noch. Dann muss die DLL geladen bleiben.
pub fn uninstall() -> bool {
    CHAT_OPEN.store(false, SeqCst);
    let window = WINDOW.load(SeqCst);
    if window == 0 {
        return true;
    }
    let h = HWND(window as *mut _);
    unsafe {
        // Noch in unserer Fensterfunktion: Zeiger zurückgeben.
        SendMessageTimeoutW(h, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 2000, None);
        WINDOW.store(0, SeqCst);
        if GetWindowLongPtrW(h, GWLP_WNDPROC) != wndproc as *const () as isize {
            // Fenster schon weg ist in Ordnung.
            return !IsWindow(Some(h)).as_bool();
        }
        SetWindowLongPtrW(h, GWLP_WNDPROC, OLD.load(SeqCst));
        // Der Fenster-Thread ist danach sicher nicht mehr in unserem Code.
        let done = SendMessageTimeoutW(h, WM_NULL, WPARAM(0), LPARAM(0), SMTO_BLOCK, 3000, None);
        let mine = GetWindowThreadProcessId(h, None) == windows::Win32::System::Threading::GetCurrentThreadId();
        (done.0 != 0 || mine) && INSIDE.load(SeqCst) == 0
    }
}

/// Gehört diese Taste gerade uns, um den Chat zu öffnen? `Some(true)`: Befehlstaste.
pub fn opens(vk: u8) -> Option<bool> {
    let hit = link(|l| {
        let c = l.control();
        if c & control::CHAT != 0 && l.is_key(vk, false) {
            Some(false)
        } else if c & control::COMMANDS != 0 && l.is_key(vk, true) {
            Some(true)
        } else {
            None
        }
    });
    // In Menüs und Textfeldern ist der Zeiger sichtbar; dort gehört die Taste dem Spiel.
    hit.flatten().filter(|_| !cursor_visible())
}

/// Gehört die Tab-Taste gerade uns? In Menüs und Textfeldern nicht.
pub fn tab(vk: u8) -> bool {
    VIRTUAL_KEY(vk as u16) == VK_TAB && link(|l| l.control() & control::TAB != 0).unwrap_or(false) && !cursor_visible()
}

/// Gehört diese Taste gerade dem Zoom?
pub fn zoom_key(vk: u8) -> Option<crate::zoom::Key> {
    link(|l| crate::zoom::key(vk, l.control(), l.zoom_keys())).flatten().filter(|_| !cursor_visible())
}

/// Wünsche des Clients.
pub fn control() -> u32 {
    link(|l| l.control()).unwrap_or(0)
}

#[cfg(not(test))]
fn cursor_visible() -> bool {
    let mut c = CURSORINFO { cbSize: size_of::<CURSORINFO>() as u32, ..Default::default() };
    // Im Zweifel ja: dann bleibt die Taste beim Spiel.
    unsafe { GetCursorInfo(&mut c) }.is_err() || c.flags.0 & CURSOR_SHOWING.0 != 0
}

#[cfg(test)]
fn cursor_visible() -> bool {
    false
}

/// Einziger Stand: Chat offen oder Menü vom Client gewünscht. Ohne Client nie.
pub fn blocked() -> bool {
    link(|l| l.alive() && (CHAT_OPEN.load(SeqCst) || l.control() & control::MENU != 0)).unwrap_or(false)
}

/// Zeigerbild, das der Client gerade wünscht.
fn shape() -> PCWSTR {
    match link(|l| l.control() >> control::SHAPE_SHIFT & control::SHAPE_MASK) {
        Some(1) => IDC_SIZEWE,
        Some(2) => IDC_SIZENS,
        Some(3) => IDC_SIZENWSE,
        Some(4) => IDC_SIZENESW,
        _ => IDC_ARROW,
    }
}

fn show_shape() {
    unsafe {
        if let Ok(c) = LoadCursorW(None, shape()) {
            SetCursor(Some(c));
        }
    }
}

/// Schließen von jedem Thread aus.
pub fn close() {
    if CHAT_OPEN.swap(false, SeqCst) {
        push(Event::Closed);
    }
    wake();
}

/// Der Zeiger gehört dem Fenster-Thread; er sieht bei der nächsten Nachricht nach.
fn wake() {
    let h = WINDOW.load(SeqCst);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

/// Einmal je Bild: auf Wünsche und Verschwinden des Clients reagieren.
pub fn tick() {
    if CHAT_OPEN.load(SeqCst) && !link(|l| l.alive()).unwrap_or(false) {
        CHAT_OPEN.store(false, SeqCst);
    }
    let now = blocked();
    // Wie in Java zählt die Taste nicht, solange etwas offen ist.
    if now || control() & control::ZOOM == 0 || cursor_visible() {
        crate::zoom::release();
    }
    let new = if now { link(|l| l.control() >> control::SHAPE_SHIFT & control::SHAPE_MASK).unwrap_or(0) } else { 0 };
    let turned = SHAPE.swap(new, SeqCst) != new;
    if LAST.swap(now, SeqCst) != now || turned {
        wake();
    }
}

/// Wie in Java ist der Mauszeiger bei offenem Chat frei. Nur im Fenster-Thread.
fn free_cursor() {
    let mut c = lock(&CURSOR);
    if c.is_some() {
        return;
    }
    unsafe {
        let mut clip = RECT::default();
        let clip = GetClipCursor(&mut clip).is_ok().then_some(clip);
        let _ = ClipCursor(None);
        let mut shown = 0;
        while ShowCursor(true) < 0 && shown < 32 {
            shown += 1;
        }
        let image = match LoadCursorW(None, IDC_ARROW) {
            Ok(arrow) => SetCursor(Some(arrow)),
            Err(_) => GetCursor(),
        };
        // Auch der letzte Aufruf zählt mit.
        *c = Some(Cursor { clip, shown: shown + 1, image: image.0 as isize });
    }
}

/// Alles wie vor dem Öffnen.
fn restore_cursor() {
    if let Some(Cursor { clip, shown, image }) = lock(&CURSOR).take() {
        unsafe {
            for _ in 0..shown {
                ShowCursor(false);
            }
            let image = HCURSOR(image as *mut _);
            SetCursor(if image.is_invalid() { None } else { Some(image) });
            let front = GetForegroundWindow().0 as isize == WINDOW.load(SeqCst);
            // Nach Alt+Tab gehört der Zeiger anderen Programmen.
            if front && let Some(r) = clip {
                let _ = ClipCursor(Some(&r));
            }
        }
    }
}

fn push(e: Event) {
    let _one = lock(&PUSH);
    link(|l| l.push(e));
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    INSIDE.fetch_add(1, SeqCst);
    let r = std::panic::catch_unwind(|| handle(m, wp, lp)).unwrap_or(false);
    let out = unsafe {
        if r {
            // WM_INPUT verlangt das Aufräumen durch Windows; alles andere ist erledigt.
            match m {
                WM_INPUT => DefWindowProcW(h, m, wp, lp),
                // Zeiger gesetzt.
                WM_SETCURSOR => LRESULT(1),
                _ => LRESULT(0),
            }
        } else {
            let old = std::mem::transmute::<isize, WNDPROC>(OLD.load(SeqCst));
            CallWindowProcW(old, h, m, wp, lp)
        }
    };
    INSIDE.fetch_sub(1, SeqCst);
    out
}

/// `true`: erreicht das Spiel nicht.
fn handle(m: u32, wp: WPARAM, lp: LPARAM) -> bool {
    let blocked = blocked();
    if blocked {
        free_cursor();
        // Auch ohne Mausbewegung, wenn der Client ein anderes Bild wünscht.
        if m == WM_NULL || m == WM_MOUSEMOVE {
            show_shape();
        }
    } else {
        restore_cursor();
    }
    let shift = || unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
    let mut k = lock(&KEYS);
    let vk = wp.0 & 0xff;
    match m {
        // Alt-Kürzel wie Alt+F4 bleiben beim Spiel, nur die Öffnen-Taste und Text zählen.
        WM_KEYDOWN => {
            if blocked {
                k.swallowed[vk] = true;
                k.skip_char = false;
                let down = |v: VIRTUAL_KEY| unsafe { GetKeyState(v.0 as i32) } < 0;
                // AltGr meldet Windows als Strg + Alt.
                let ctrl = down(VK_CONTROL) && !down(VK_MENU);
                push(Event::Key { vk: vk as u8, shift: shift(), ctrl });
                // Sofort schließen, nicht erst beim nächsten Bild.
                let closing = matches!(VIRTUAL_KEY(vk as u16), VK_ESCAPE | VK_RETURN);
                if closing && CHAT_OPEN.swap(false, SeqCst) && !self::blocked() {
                    restore_cursor();
                }
                return true;
            }
            // Verschluckt und noch gehalten: Wiederholungen gehen auch nach dem Schließen nicht ins Spiel.
            let repeat = lp.0 & (1 << 30) != 0;
            if k.swallowed[vk] && repeat {
                return true;
            }
            k.swallowed[vk] = false;
            if !repeat && tab(vk as u8) {
                k.swallowed[vk] = true;
                k.skip_char = true;
                k.tab = true;
                push(Event::Tab { down: true });
                return true;
            }
            if !repeat && let Some(z) = zoom_key(vk as u8) {
                k.swallowed[vk] = true;
                k.skip_char = true;
                if z == crate::zoom::Key::Zoom {
                    k.zoom = vk;
                }
                crate::zoom::press(z);
                return true;
            }
            // Auch mit gehaltener Strg-Taste (Sprinten). Alt-Kürzel kommen als andere Nachricht.
            if !repeat && let Some(command) = opens(vk as u8) {
                k.swallowed[vk] = true;
                k.skip_char = true;
                CHAT_OPEN.store(true, SeqCst);
                free_cursor();
                push(Event::Open { command });
                return true;
            }
            false
        }
        WM_KEYUP | WM_SYSKEYUP => {
            if VIRTUAL_KEY(vk as u16) == VK_TAB && std::mem::take(&mut k.tab) {
                push(Event::Tab { down: false });
            }
            if vk == k.zoom && vk != 0 {
                k.zoom = 0;
                crate::zoom::release();
            }
            std::mem::take(&mut k.swallowed[vk])
        }
        WM_CHAR => {
            if blocked && !k.skip_char {
                let unit = wp.0 as u16;
                let c = match (k.high.take(), unit) {
                    (_, 0xD800..=0xDBFF) => {
                        k.high = Some(unit);
                        None
                    }
                    (Some(a), _) => char::decode_utf16([a, unit]).next().and_then(Result::ok),
                    (None, _) => char::from_u32(unit as u32),
                };
                if let Some(c) = c.filter(|c| !c.is_control()) {
                    push(Event::Char(c));
                }
            }
            blocked
        }
        WM_DEADCHAR => blocked,
        WM_KILLFOCUS => {
            // Das Loslassen kommt nach Alt+Tab nicht mehr an.
            if std::mem::take(&mut k.tab) {
                push(Event::Tab { down: false });
            }
            k.zoom = 0;
            crate::zoom::release();
            drop(k);
            close();
            false
        }
        WM_SETCURSOR if blocked => {
            show_shape();
            true
        }
        // Rohe Maus: Damit dreht das Spiel die Kamera.
        WM_INPUT => blocked,
        WM_MOUSEMOVE => {
            if blocked {
                push(Event::Move { x: lp.0 as i16, y: (lp.0 >> 16) as i16 });
            }
            blocked
        }
        WM_MOUSEWHEEL => {
            if !blocked && crate::zoom::mouse(control()).2 {
                crate::zoom::step(((wp.0 >> 16) as i16).signum() as i32);
                return true;
            }
            if blocked {
                push(Event::Wheel { notches: ((wp.0 >> 16) as i16).signum() as i8, shift: shift() });
            }
            blocked
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDBLCLK => {
            let bit = button(m);
            if blocked {
                if bit <= 2 {
                    push(Event::Button { which: bit - 1, down: true });
                }
                k.buttons |= bit;
            } else {
                k.buttons &= !bit;
            }
            blocked
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
            let bit = button(m);
            let ours = k.buttons & bit != 0;
            if ours && bit <= 2 {
                push(Event::Button { which: bit - 1, down: false });
            }
            k.buttons &= !bit;
            ours
        }
        _ => false,
    }
}

fn button(m: u32) -> u8 {
    match m {
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK => 1,
        WM_RBUTTONDOWN | WM_RBUTTONUP | WM_RBUTTONDBLCLK => 2,
        WM_MBUTTONDOWN | WM_MBUTTONUP => 4,
        _ => 8,
    }
}
