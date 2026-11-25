//! Chat-Plugin: sendet im Hauptthread des Spiels genau die Nachricht, die der Nutzer abgeschickt hat.
//! Verändert keinen Spielcode: Windows ruft uns beim Empfang einer eigenen Fensternachricht auf.
use mett_plugin::chat::{Action, Order, answer};
use mett_plugin::{API, Host, Info};
use std::panic::catch_unwind;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, AtomicU64, Ordering::SeqCst};
use windows::Win32::Foundation::*;
use windows::Win32::System::Diagnostics::Debug::RtlLookupFunctionEntry;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

type Keeper = (std::sync::mpsc::Sender<()>, std::thread::JoinHandle<()>);

static IMAGE: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static KEEPER: Mutex<Option<Keeper>> = Mutex::new(None);
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);
static MESSAGE: AtomicU32 = AtomicU32::new(0);
/// Immer nur ein Auftrag.
static BUSY: Mutex<()> = Mutex::new(());
static SLOT: Mutex<Slot> = Mutex::new(Slot::Empty);

enum Slot {
    Empty,
    Waiting(Order),
    Running,
    Done(u32),
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Zeichenkette im Aufbau von MSVC. Kurze Texte liegen darin, längere an der Adresse in den ersten acht Byte.
#[repr(C)]
struct StdString {
    data: [u8; 16],
    len: u64,
    cap: u64,
}

impl StdString {
    /// Gilt nur, solange `text` lebt. `text` endet mit einer Null.
    fn new(text: &[u8]) -> Self {
        let len = text.len() - 1;
        let mut data = [0; 16];
        if len < 16 {
            data[..len].copy_from_slice(&text[..len]);
            Self { data, len: len as u64, cap: 15 }
        } else {
            data[..8].copy_from_slice(&(text.as_ptr() as u64).to_le_bytes());
            Self { data, len: len as u64, cap: len as u64 }
        }
    }
}

/// Liest ein Wort, wenn der Speicher lesbar ist.
fn word(at: u64) -> Option<u64> {
    if at < 0x10000 || !at.is_multiple_of(8) {
        return None;
    }
    let mut m = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(at as *const _), &mut m, size_of::<MEMORY_BASIC_INFORMATION>()) };
    let readable = [PAGE_READWRITE, PAGE_READONLY, PAGE_EXECUTE_READ, PAGE_WRITECOPY].contains(&m.Protect);
    let fits = at + 8 <= m.BaseAddress as u64 + m.RegionSize as u64;
    (n != 0 && m.State == MEM_COMMIT && readable && fits).then(|| unsafe { *(at as *const u64) })
}

/// Beginnt genau hier eine Funktion des Spiels?
fn is_function(f: u64) -> bool {
    let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
    if f < base || f >= base + size {
        return false;
    }
    let mut found = 0;
    let e = unsafe { RtlLookupFunctionEntry(f, &mut found, None) };
    !e.is_null() && found == base && unsafe { (*e).BeginAddress } as u64 == f - base
}

/// Gezählter Zeiger, wie ihn die Befehlsfunktion bekommt. Sie gibt den Zähler am Ende selbst zurück.
#[repr(C)]
struct Owner {
    alive: u64,
    count: u64,
    client: u64,
}

/// `None`, wenn der Zähler nicht wie einer aussieht.
fn owner(model: u64) -> Option<Owner> {
    let o = Owner { alive: word(model + 0x48)?, count: word(model + 0x50)?, client: word(model + 0x58)? };
    let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
    let uses = word(o.count + 8)? as u32;
    (word(o.count)?.wrapping_sub(base) < size && (1..1_000_000).contains(&uses)).then_some(o)
}

/// Alles wird im Hauptthread unmittelbar vor dem Aufruf geprüft.
fn fits(o: &Order) -> bool {
    let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
    let inside = |p: Option<u64>| p.is_some_and(|p| p >= base && p < base + size);
    let main = unsafe { GetCurrentThreadId() } == THREAD.load(SeqCst);
    main && is_function(o.function)
        && inside(Some(o.table))
        && o.offset < 0x10000
        && word(o.controller) == Some(o.table)
        && word(o.controller + o.offset) == Some(o.model)
        && inside(word(o.model))
        && word(o.model + 0x48).and_then(word).is_some_and(|v| v & 0xff == 1)
        && word(o.model + 0x58).is_some_and(|c| inside(word(c)))
}

fn run() {
    let order = {
        let mut slot = lock(&SLOT);
        match std::mem::replace(&mut *slot, Slot::Running) {
            Slot::Waiting(o) => o,
            other => {
                *slot = other;
                return;
            }
        }
    };
    *lock(&SLOT) = Slot::Done(if fits(&order) { send(&order) } else { answer::STALE });
}

fn send(order: &Order) -> u32 {
    let (Action::Chat(text) | Action::Command(text)) = &order.action else { return answer::OK };
    let bytes: Vec<u8> = text.bytes().chain([0]).collect();
    let s = StdString::new(&bytes);
    let f = order.function as usize;
    let restricted = if matches!(order.action, Action::Chat(_)) {
        type SendChat = unsafe extern "system" fn(u64, *const StdString) -> u8;
        unsafe { std::mem::transmute::<usize, SendChat>(f)(order.model, &s) }
    } else {
        let Some(mut o) = owner(order.model) else { return answer::STALE };
        type SendCommand = unsafe extern "system" fn(*const StdString, *mut Owner) -> u8;
        unsafe {
            AtomicU32::from_ptr((o.count + 8) as *mut u32).fetch_add(1, SeqCst);
            std::mem::transmute::<usize, SendCommand>(f)(&s, &mut o)
        }
    };
    if restricted == 0 { answer::OK } else { answer::RESTRICTED }
}

unsafe extern "system" fn on_message(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let m = &*(lp.0 as *const CWPSTRUCT);
            if m.message == MESSAGE.load(SeqCst) && m.message != 0 && m.hwnd.0 as isize == WINDOW.load(SeqCst) {
                let _ = catch_unwind(run);
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

/// Auftrag ausführen und das Ergebnis melden. Eigener Thread, nie der des Spiels.
///
/// # Safety
/// `text` zeigt auf nullterminierten UTF-16-Text.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_chat_order(text: *const u16) -> u32 {
    catch_unwind(|| {
        if text.is_null() {
            return answer::BAD_ORDER;
        }
        let text = unsafe { PCWSTR(text).to_string() }.unwrap_or_default();
        let Some(order) = Order::decode(&text) else { return answer::BAD_ORDER };
        let Ok(_busy) = BUSY.try_lock() else { return answer::NOT_REACHED };
        {
            let mut slot = lock(&SLOT);
            if matches!(*slot, Slot::Running) {
                return answer::NOT_REACHED;
            }
            *slot = Slot::Waiting(order);
        }
        let window = HWND(WINDOW.load(SeqCst) as *mut _);
        unsafe {
            SendMessageTimeoutW(window, MESSAGE.load(SeqCst), WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 3000, None)
        };
        let mut slot = lock(&SLOT);
        match *slot {
            Slot::Done(a) => {
                *slot = Slot::Empty;
                a
            }
            Slot::Running => answer::UNKNOWN,
            // Zurückziehen, damit nichts verspätet gesendet wird.
            _ => {
                *slot = Slot::Empty;
                answer::NOT_REACHED
            }
        }
    })
    .unwrap_or(answer::UNKNOWN)
}

/// Spielfenster des eigenen Prozesses und sein Thread. Es muss genau eines geben.
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
        MESSAGE.store(RegisterWindowMessageW(w!("MettChat")), SeqCst);
        // Windows entfernt einen Hook mit dem Ende des Threads, der ihn eingerichtet hat. Deshalb hält ihn ein eigener.
        let (done, ready) = std::sync::mpsc::channel();
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let keeper =
            std::thread::spawn(move || match SetWindowsHookExW(WH_CALLWNDPROC, Some(on_message), None, thread) {
                Ok(h) => {
                    let _ = done.send(true);
                    let _ = wait.recv();
                    let _ = UnhookWindowsHookEx(h);
                }
                Err(_) => {
                    let _ = done.send(false);
                }
            });
        if ready.recv() != Ok(true) {
            let _ = keeper.join();
            return 2;
        }
        *lock(&KEEPER) = Some((stop, keeper));
        0
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn mett_plugin_unload() {
    if let Some((stop, keeper)) = lock(&KEEPER).take() {
        drop(stop);
        let _ = keeper.join();
    }
    unsafe {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kurze_und_lange_texte() {
        let short = StdString::new(b"123456789012345\0");
        assert_eq!((short.len, short.cap, &short.data[..15]), (15, 15, &b"123456789012345"[..]));
        let text = "Grüße, das ist länger\0".as_bytes();
        let long = StdString::new(text);
        assert_eq!(long.len as usize, text.len() - 1);
        assert_eq!(u64::from_le_bytes(long.data[..8].try_into().unwrap()), text.as_ptr() as u64);
        assert!(long.cap >= 16);
    }

    #[test]
    fn unlesbares_wird_erkannt() {
        assert_eq!(word(8), None);
        assert_eq!(word(0x10001), None);
        let v = std::hint::black_box(Box::new(7u64));
        assert_eq!(word(&*v as *const u64 as u64), Some(7));
    }

    static GOT: Mutex<Vec<(u32, String)>> = Mutex::new(vec![]);

    /// Ersatz für die Sendefunktion des Spiels.
    #[inline(never)]
    unsafe extern "system" fn fake_send(_: u64, s: *const StdString) -> u8 {
        unsafe {
            let s = &*s;
            let at =
                if s.cap < 16 { s.data.as_ptr() } else { u64::from_le_bytes(s.data[..8].try_into().unwrap()) as _ };
            let text = std::str::from_utf8(std::slice::from_raw_parts(at, s.len as usize)).unwrap().to_owned();
            lock(&GOT).push((GetCurrentThreadId(), text.clone()));
            (text == "stumm") as u8
        }
    }

    /// Ersatz für die Befehlsfunktion: gibt den Zähler zurück wie das Original.
    #[inline(never)]
    unsafe extern "system" fn fake_command(s: *const StdString, o: *mut Owner) -> u8 {
        unsafe {
            let o = &*o;
            assert_eq!(*(o.alive as *const u64) & 0xff, 1);
            AtomicU32::from_ptr((o.count + 8) as *mut u32).fetch_sub(1, SeqCst);
            fake_send(o.client, s)
        }
    }

    fn call(t: &str) -> u32 {
        let t: Vec<u16> = t.encode_utf16().chain([0]).collect();
        unsafe { mett_chat_order(t.as_ptr()) }
    }

    /// Alles nacheinander in einem Test, weil das Plugin nur einmal im Prozess lebt.
    #[test]
    fn ganzer_weg_mit_nachgestelltem_spiel() {
        assert_eq!(call("quatsch"), answer::BAD_ORDER);
        // Kein Spielfenster: wird zurückgezogen.
        assert_eq!(call("1 2 3 4 5\nhallo"), answer::NOT_REACHED);
        assert!(matches!(*lock(&SLOT), Slot::Empty));

        let (ready, window) = std::sync::mpsc::channel();
        let game = std::thread::spawn(move || unsafe {
            unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
                unsafe { DefWindowProcW(h, m, w, l) }
            }
            let class = WNDCLASSW { lpfnWndProc: Some(proc), lpszClassName: w!("Bedrock"), ..Default::default() };
            RegisterClassW(&class);
            let h = CreateWindowExW(
                Default::default(),
                w!("Bedrock"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            );
            ready.send((h.unwrap().0 as isize, GetCurrentThreadId())).unwrap();
            let mut m = MSG::default();
            while GetMessageW(&mut m, None, 0, 0).0 > 0 {
                DispatchMessageW(&m);
            }
        });
        let (_, game_thread) = window.recv().unwrap();

        unsafe {
            let base = GetModuleHandleW(None).unwrap().0 as usize;
            let pe = base + *((base + 0x3c) as *const u32) as usize;
            let image_size = *((pe + 0x50) as *const u32) as usize;
            let host = Host { size: size_of::<Host>() as u32, api: API, game: [0; 4], base, image_size };
            let mut info = Info::default();
            assert_eq!(mett_plugin_query(&host, &mut info), 0);
            assert_eq!(mett_plugin_load(&host), 0);
        }
        // Tabellen liegen im Programm, Objekte auf dem Heap.
        static TABLE: AtomicU64 = AtomicU64::new(1);
        let table = TABLE.as_ptr() as u64;
        let mut alive = Box::new(1u64);
        let client = Box::new(table);
        let mut model = Box::new([0u64; 16]);
        model[0] = table;
        model[9] = &raw mut *alive as u64;
        // Zähler: Tabelle im Programm, danach 3 Benutzer.
        let mut count = Box::new([table, 3]);
        model[10] = &raw mut *count as u64;
        model[11] = &raw const *client as u64;
        let controller = Box::new([table, 0, &raw const *model as u64]);
        let mut o = Order {
            function: fake_send as *const () as u64,
            controller: &raw const *controller as u64,
            table,
            offset: 16,
            model: &raw const *model as u64,
            action: Action::Check,
        };
        // Prüfen sendet nichts.
        assert_eq!(call(&o.encode()), answer::OK);
        assert!(lock(&GOT).is_empty());
        let long = "Grüße 😀 ".repeat(30).trim().to_owned();
        for t in ["kurz", "genau 15 Zeiche", "jetzt 16 Zeichen", &long] {
            o.action = Action::Chat(t.into());
            assert_eq!(call(&o.encode()), answer::OK);
            assert_eq!(lock(&GOT).pop(), Some((game_thread, t.to_owned())));
        }
        o.action = Action::Chat("stumm".into());
        assert_eq!(call(&o.encode()), answer::RESTRICTED);
        lock(&GOT).clear();
        o.action = Action::Command("/help".into());
        let chat_function = std::mem::replace(&mut o.function, fake_command as *const () as u64);
        assert_eq!(call(&o.encode()), answer::OK);
        assert_eq!(lock(&GOT).pop(), Some((game_thread, "/help".to_owned())));
        assert_eq!(unsafe { std::ptr::read_volatile(&raw const count[1]) }, 3);
        // Ein unsinniger Zähler heißt: nicht senden.
        unsafe { std::ptr::write_volatile(&raw mut count[1], 0) };
        assert_eq!(call(&o.encode()), answer::STALE);
        unsafe { std::ptr::write_volatile(&raw mut count[1], 3) };
        o.function = chat_function;
        o.action = Action::Chat("x".into());
        // Jede Abweichung heißt: nicht senden.
        for bad in [
            Order { function: o.function + 1, ..o.clone() },
            Order { controller: 0x10000, ..o.clone() },
            Order { table: o.model, ..o.clone() },
            Order { offset: 8, ..o.clone() },
            Order { model: o.controller, ..o.clone() },
        ] {
            assert_eq!(call(&bad.encode()), answer::STALE);
        }
        unsafe { std::ptr::write_volatile(&raw mut *alive, 0) };
        assert_eq!(call(&o.encode()), answer::STALE);
        unsafe { std::ptr::write_volatile(&raw mut *alive, 1) };
        mett_plugin_unload();
        assert_eq!(call(&o.encode()), answer::NOT_REACHED);
        assert!(lock(&GOT).is_empty());
        unsafe { PostThreadMessageW(game_thread, WM_QUIT, WPARAM(0), LPARAM(0)).unwrap() };
        game.join().unwrap();
    }
}
