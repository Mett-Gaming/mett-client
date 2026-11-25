//! Nur Entwicklung, wird nie ausgeliefert: zählt, welche leeren Rückrufe Minecraft an den Bausteinen der Chat-Anzeige
//! aufruft und wann. Verschiebt nichts. Ersetzt werden nur Funktionen, die nachweislich nichts tun.
use mett_plugin::{API, Host, Info};
use std::panic::catch_unwind;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering::SeqCst};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Performance::QueryPerformanceCounter;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

/// So viele Funktionen hat ein Baustein höchstens.
const SLOTS: usize = 16;
/// Kopierte Einträge der Funktionstabelle; was dahinter liegt, wird nie darüber aufgerufen.
const COPIED: usize = 32;
const PARTS: usize = 8;
const EVENTS: usize = 512;
/// Merker in `slot`: aus der Nachrichtenschleife, nicht aus einem Rückruf.
const TICK: u32 = 99;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Event {
    pub time: i64,
    pub part: u32,
    pub slot: u32,
    pub thread: u32,
    pub flags: u32,
    pub height: f32,
    pub place: f32,
    pub list_height: f32,
    pub done: f32,
}

#[repr(C)]
pub struct Log {
    pub counts: [[AtomicU32; SLOTS]; PARTS],
    pub replaced: [AtomicU32; PARTS],
    pub main_thread: AtomicU32,
    pub restored: AtomicU32,
    pub used: AtomicU32,
    pub pad: u32,
    pub events: [std::cell::UnsafeCell<Event>; EVENTS],
}

unsafe impl Sync for Log {}

#[unsafe(no_mangle)]
pub static METT_UIPROBE: Log = Log {
    counts: [const { [const { AtomicU32::new(0) }; SLOTS] }; PARTS],
    replaced: [const { AtomicU32::new(0) }; PARTS],
    main_thread: AtomicU32::new(0),
    restored: AtomicU32::new(0),
    used: AtomicU32::new(0),
    pad: 0,
    events: [const {
        std::cell::UnsafeCell::new(Event {
            time: 0,
            part: 0,
            slot: 0,
            thread: 0,
            flags: 0,
            height: 0.0,
            place: 0.0,
            list_height: 0.0,
            done: 0.0,
        })
    }; EVENTS],
};

/// Bausteine, deren Funktionstabelle gerade unsere ist.
struct Part {
    at: u64,
    original: u64,
    /// Kopie samt dem Wort davor. Bleibt bis zum Prozessende bestehen.
    copy: &'static [u64],
}

static IMAGE: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static PANEL: AtomicU64 = AtomicU64::new(0);
static LIST: AtomicU64 = AtomicU64::new(0);
/// Adressen der Bausteine, für die Rückrufe ohne Sperre.
static AT: [AtomicU64; PARTS] = [const { AtomicU64::new(0) }; PARTS];
static LAST: [AtomicU32; SLOTS + 1] = [const { AtomicU32::new(0) }; SLOTS + 1];
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);
static MESSAGE: AtomicU32 = AtomicU32::new(0);
static ORDER: Mutex<Option<[u64; 3]>> = Mutex::new(None);
static HELD: Mutex<Vec<Part>> = Mutex::new(vec![]);
static KEEPER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn protection(at: u64, size: u64) -> Option<PAGE_PROTECTION_FLAGS> {
    let mut m = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(at as *const _), &mut m, size_of::<MEMORY_BASIC_INFORMATION>()) };
    let end = m.BaseAddress as u64 + m.RegionSize as u64;
    (n != 0 && m.State == MEM_COMMIT && at >= 0x10000 && at.checked_add(size)? <= end).then_some(m.Protect)
}

fn writable(at: u64, size: u64) -> bool {
    at.is_multiple_of(8) && protection(at, size) == Some(PAGE_READWRITE)
}

fn readable(at: u64, size: u64) -> bool {
    let ok = [PAGE_READONLY, PAGE_READWRITE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_WRITECOPY];
    protection(at, size).is_some_and(|p| ok.contains(&p))
}

unsafe fn word(at: u64) -> u64 {
    unsafe { *(at as *const u64) }
}

unsafe fn float(at: u64) -> f32 {
    unsafe { *(at as *const f32) }
}

/// Was die Funktion tut, wenn es sicher nichts ist.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Empty {
    /// `ret`
    Nothing,
    /// `xor al, al; ret`
    No,
}

fn empty(f: u64) -> Option<Empty> {
    let code = [PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_EXECUTE];
    if !protection(f, 16).is_some_and(|p| code.contains(&p) && p != PAGE_EXECUTE) {
        return None;
    }
    let b = unsafe { *(f as *const [u8; 3]) };
    match b {
        [0xC3, ..] | [0xC2, 0, 0] => Some(Empty::Nothing),
        [0x32 | 0x30, 0xC0, 0xC3] => Some(Empty::No),
        _ => None,
    }
}

fn note(part: u32, slot: u32) {
    let (p, l) = (PANEL.load(SeqCst), LIST.load(SeqCst));
    // Nur Werte festhalten, wenn sich die Höhe seit dem letzten gleichen Rückruf geändert hat.
    if !writable(p, 0xC0) || !writable(l, 0xC0) {
        return;
    }
    unsafe {
        let height = float(p + 0x5C);
        let seen = &LAST[(slot as usize).min(SLOTS)];
        if seen.swap(height.to_bits(), SeqCst) == height.to_bits() {
            return;
        }
        let i = METT_UIPROBE.used.fetch_add(1, SeqCst) as usize;
        if i >= EVENTS {
            return;
        }
        let mut time = 0;
        let _ = QueryPerformanceCounter(&mut time);
        METT_UIPROBE.events[i].get().write(Event {
            time,
            part,
            slot,
            thread: GetCurrentThreadId(),
            flags: *((p + 0x28) as *const u32),
            height,
            place: float(p + 0x54),
            list_height: float(l + 0x5C),
            done: float(p + 0x24),
        });
    }
}

fn called(this: u64, slot: usize) {
    let part = AT.iter().position(|a| a.load(SeqCst) == this);
    if let Some(part) = part {
        METT_UIPROBE.counts[part][slot].fetch_add(1, SeqCst);
        note(part as u32, slot as u32);
    }
}

unsafe extern "system" fn nothing<const S: usize>(this: u64) {
    let _ = catch_unwind(|| called(this, S));
}

unsafe extern "system" fn no<const S: usize>(this: u64) -> u64 {
    let _ = catch_unwind(|| called(this, S));
    0
}

macro_rules! table {
    ($f:ident) => {
        [
            $f::<0> as *const () as u64,
            $f::<1> as *const () as u64,
            $f::<2> as *const () as u64,
            $f::<3> as *const () as u64,
            $f::<4> as *const () as u64,
            $f::<5> as *const () as u64,
            $f::<6> as *const () as u64,
            $f::<7> as *const () as u64,
            $f::<8> as *const () as u64,
            $f::<9> as *const () as u64,
            $f::<10> as *const () as u64,
            $f::<11> as *const () as u64,
            $f::<12> as *const () as u64,
            $f::<13> as *const () as u64,
            $f::<14> as *const () as u64,
            $f::<15> as *const () as u64,
        ]
    };
}

/// Kopie der Tabelle mit unseren Zählern; der Aufräumer (0) bleibt immer der des Spiels.
fn copy(original: u64) -> Option<(&'static [u64], u32)> {
    if !readable(original - 8, (COPIED as u64 + 1) * 8) {
        return None;
    }
    let mut v: Vec<u64> = (0..=COPIED as u64).map(|i| unsafe { word(original - 8 + i * 8) }).collect();
    let (a, b): ([u64; SLOTS], [u64; SLOTS]) = (table!(nothing), table!(no));
    let mut replaced = 0;
    for s in 1..SLOTS {
        let new = match empty(v[s + 1]) {
            Some(Empty::Nothing) => a[s],
            Some(Empty::No) => b[s],
            None => continue,
        };
        v[s + 1] = new;
        replaced |= 1 << s;
    }
    Some((v.leak(), replaced))
}

/// Bausteine eines Elements: jeder nennt das Element, zu dem er gehört.
fn parts(control: u64) -> Vec<u64> {
    let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
    if !writable(control, 0x180) {
        return vec![];
    }
    let (begin, end) = unsafe { (word(control + 0xC8), word(control + 0xD0)) };
    let Some(bytes) = end.checked_sub(begin).filter(|s| *s > 0 && *s <= 64 && s % 8 == 0) else { return vec![] };
    if !writable(begin, bytes) {
        return vec![];
    }
    let list: Vec<u64> = (0..bytes / 8).map(|i| unsafe { word(begin + i * 8) }).collect();
    let fits = |c: &u64| {
        writable(*c, 16) && unsafe { (base..base + size).contains(&word(*c)) && word(*c + 8) == control + 0x10 }
    };
    if list.iter().all(fits) { list } else { vec![] }
}

fn attach([panel, list, table]: [u64; 3]) {
    let held = &mut *lock(&HELD);
    let ours = |c: u64| writable(c, 0x180) && unsafe { word(c) == table };
    if !held.is_empty() || !ours(panel) || !ours(list) || unsafe { word(list + 0xA0) } != panel {
        return;
    }
    PANEL.store(panel, SeqCst);
    LIST.store(list, SeqCst);
    for at in parts(panel).into_iter().chain(parts(list)).take(PARTS) {
        let original = unsafe { word(at) };
        let Some((copy, replaced)) = copy(original) else { continue };
        let i = held.len();
        AT[i].store(at, SeqCst);
        METT_UIPROBE.replaced[i].store(replaced, SeqCst);
        unsafe { (at as *mut u64).write_volatile(copy.as_ptr() as u64 + 8) };
        held.push(Part { at, original, copy });
    }
}

/// Nur, wo noch unsere Tabelle steht.
fn detach() {
    for p in lock(&HELD).drain(..) {
        unsafe {
            if writable(p.at, 8) && word(p.at) == p.copy.as_ptr() as u64 + 8 {
                (p.at as *mut u64).write_volatile(p.original);
                METT_UIPROBE.restored.fetch_add(1, SeqCst);
            }
        }
    }
    AT.iter().for_each(|a| a.store(0, SeqCst));
}

unsafe extern "system" fn on_message(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let m = &*(lp.0 as *const CWPSTRUCT);
            let ours = m.message == MESSAGE.load(SeqCst) && m.message != 0 && m.hwnd.0 as isize == WINDOW.load(SeqCst);
            if ours && GetCurrentThreadId() == THREAD.load(SeqCst) {
                let stop = m.wParam.0 == 1;
                let _ = catch_unwind(|| {
                    if stop {
                        return detach();
                    }
                    if let Some(o) = lock(&ORDER).take() {
                        attach(o);
                    }
                    if PANEL.load(SeqCst) != 0 {
                        note(TICK, TICK);
                    }
                });
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

/// # Safety
/// `text` zeigt auf nullterminierten UTF-16-Text: Anzeige, Liste, Tabelle, hexadezimal.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_uiprobe_order(text: *const u16) -> u32 {
    if text.is_null() {
        return 1;
    }
    let text = unsafe { PCWSTR(text).to_string() }.unwrap_or_default();
    let v: Vec<u64> = text.split(' ').filter_map(|t| u64::from_str_radix(t, 16).ok()).collect();
    let Ok(o) = <[u64; 3]>::try_from(v) else { return 2 };
    *lock(&ORDER) = Some(o);
    0
}

fn tell(what: usize) {
    let window = HWND(WINDOW.load(SeqCst) as *mut _);
    unsafe {
        SendMessageTimeoutW(window, MESSAGE.load(SeqCst), WPARAM(what), LPARAM(0), SMTO_ABORTIFHUNG, 1000, None);
    }
}

fn game_window() -> Option<(HWND, u32)> {
    let mut pid = 0;
    unsafe {
        let h = FindWindowExW(None, None, w!("Bedrock"), None).ok()?;
        let thread = GetWindowThreadProcessId(h, Some(&mut pid));
        (pid == GetCurrentProcessId()).then_some((h, thread))
    }
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
        METT_UIPROBE.main_thread.store(thread, SeqCst);
        MESSAGE.store(RegisterWindowMessageW(w!("MettUiProbe")), SeqCst);
        // Unsere Zähler könnten noch aufgerufen werden: Die DLL bleibt bis zum Ende des Spiels geladen.
        let mut pinned = HMODULE::default();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
        if GetModuleHandleExW(flags, PCWSTR(on_message as *const () as *const u16), &mut pinned).is_err() {
            return 3;
        }
        STOP.store(false, SeqCst);
        let (done, ready) = std::sync::mpsc::channel();
        let keeper = std::thread::spawn(move || {
            let Ok(h) = SetWindowsHookExW(WH_CALLWNDPROC, Some(on_message), None, thread) else {
                let _ = done.send(false);
                return;
            };
            let _ = done.send(true);
            while !STOP.load(SeqCst) {
                tell(0);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            tell(1);
            let _ = UnhookWindowsHookEx(h);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn real(this: u64) -> u64 {
        this + 1
    }

    #[test]
    fn ersetzt_nur_leere_und_stellt_zurueck() {
        unsafe {
            let code = VirtualAlloc(None, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) as *mut u8;
            let put = |o: usize, b: &[u8]| std::ptr::copy_nonoverlapping(b.as_ptr(), code.add(o), b.len());
            put(0, &[0xC3]);
            put(16, &[0xC2, 0, 0]);
            put(32, &[0x32, 0xC0, 0xC3]);
            put(48, &[0x48, 0x8B, 0xC1, 0xC3]);
            put(64, &[0xC2, 8, 0]);
            let f = |o: usize| code as u64 + o as u64;
            assert_eq!(
                [0, 16, 32, 48, 64].map(|o| empty(f(o))),
                [Some(Empty::Nothing), Some(Empty::Nothing), Some(Empty::No), None, None]
            );
            assert_eq!(empty(0x10), None);

            let mem = VirtualAlloc(None, 0x10000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) as u64;
            let set = |o: u64, v: u64| ((mem + o) as *mut u64).write_volatile(v);
            // Tabelle bei 0x1000: Aufräumer, leer, nein, echt, Rest echt.
            let real = real as *const () as u64;
            (0..40u64).for_each(|i| set(0x1000 + i * 8, real));
            let table = mem + 0x1008;
            set(0x1008, f(0));
            set(0x1010, f(0));
            set(0x1018, f(32));
            // Anzeige bei 0x2000 und Liste bei 0x3000 mit je einem Baustein.
            for (c, list, part) in [(0x2000u64, 0x4000u64, 0x5000u64), (0x3000, 0x4100, 0x5100)] {
                set(c, table);
                set(c + 0xC8, mem + list);
                set(c + 0xD0, mem + list + 8);
                set(list, mem + part);
                set(part, table);
                set(part + 8, mem + c + 0x10);
            }
            IMAGE[0].store(mem, SeqCst);
            IMAGE[1].store(0x10000, SeqCst);
            // Falsche Eltern: nichts anfassen.
            attach([mem + 0x2000, mem + 0x3000, table]);
            assert_eq!(word(mem + 0x5000), table);
            set(0x3000 + 0xA0, mem + 0x2000);
            attach([mem + 0x2000, mem + 0x3000, table]);
            let new = word(mem + 0x5000);
            assert!(new != table && word(mem + 0x5100) != table);
            assert_eq!(METT_UIPROBE.replaced[0].load(SeqCst), 0b110);
            // Aufräumer und echte Funktionen sind die des Spiels, auch das Wort vor der Tabelle.
            assert_eq!((word(new), word(new + 24), word(new - 8)), (f(0), real, real));
            type F = unsafe extern "system" fn(u64) -> u64;
            let call = |slot: u64| std::mem::transmute::<u64, F>(word(new + slot * 8))(mem + 0x5000);
            call(1);
            call(1);
            assert_eq!((call(2), call(3)), (0, mem + 0x5001));
            let counts = |s: usize| METT_UIPROBE.counts[0][s].load(SeqCst);
            assert_eq!((counts(1), counts(2), counts(3)), (2, 1, 0));
            // Ein Baustein hat inzwischen eine andere Tabelle: Finger weg.
            set(0x5100, 0x1234);
            detach();
            assert_eq!((word(mem + 0x5000), word(mem + 0x5100)), (table, 0x1234));
            assert_eq!(METT_UIPROBE.restored.load(SeqCst), 1);
        }
    }
}
