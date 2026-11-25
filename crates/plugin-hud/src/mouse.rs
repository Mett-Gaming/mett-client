//! Sperre für GameInput: Solange etwas von uns offen ist, bekommt das Spiel eine stehende Maus und keine Tasten. Die Stellen kommen
//! aus einer eigenen GameInput-Schnittstelle, nicht aus Minecraft. Passt der Aufbau nicht, bleibt alles unverändert.
use hud::zoom::Smooth;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering::SeqCst};
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::core::{GUID, HRESULT, s, w};

/// Größe des Mauswerts dieser Schnittstelle: Tasten, Art, Bewegung x/y, absolut x/y, Rad x/y.
const SIZE: usize = 56;
const IGAMEINPUT: GUID = GUID::from_u128(0xbbaa66d2_837a_40f7_a303_917d500955f4);
/// Plätze in den Tabellen dieser Schnittstelle.
const CURRENT_READING: usize = 4;
const KEY_STATE: usize = 13;
const MOUSE_STATE: usize = 14;
const KIND_MOUSE: u32 = 0x20;

type Obj = *mut *const usize;
type Current = unsafe extern "system" fn(Obj, u32, *mut c_void, *mut Obj) -> HRESULT;
type State = unsafe extern "system" fn(Obj, *mut u8) -> bool;
type Keys = unsafe extern "system" fn(Obj, u32, *mut Key) -> u32;
type Release = unsafe extern "system" fn(Obj) -> u32;

struct Slot {
    at: AtomicUsize,
    old: AtomicUsize,
}

static READING: Slot = Slot { at: AtomicUsize::new(0), old: AtomicUsize::new(0) };
static STATE: Slot = Slot { at: AtomicUsize::new(0), old: AtomicUsize::new(0) };
static KEYS: Slot = Slot { at: AtomicUsize::new(0), old: AtomicUsize::new(0) };
static INSIDE: AtomicU32 = AtomicU32::new(0);
static OWN: AtomicUsize = AtomicUsize::new(0);
/// Jede Maus hat eigene Zähler.
static FILTERS: Mutex<BTreeMap<usize, Filter>> = Mutex::new(BTreeMap::new());
/// Beim Schließen noch gedrückte Tasten zählen erst nach dem Loslassen wieder.
static HELD: Mutex<[bool; 256]> = Mutex::new([false; 256]);
/// Bis dahin gelten die schließenden Tasten als gehalten: GameInput meldet sie manchmal erst nach dem Schließen.
static GRACE: AtomicU64 = AtomicU64::new(0);
const GRACE_MS: u64 = 250;
/// Seit wann (ms) Esc bei offener Eingabe gedrückt ist; 0 heißt nicht.
static ESCAPE: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// Letzter Messwert dieses Threads und das Gerät, zu dem er gehört.
    static LAST: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

/// Taste, wie GameInput sie meldet.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Key {
    scan: u32,
    code: u32,
    vk: u8,
    dead: bool,
}
/// Gemessener Aufbau: 0 offen, 1 passt, 2 passt nicht.
static LAYOUT: AtomicU32 = AtomicU32::new(0);
/// Zum Auslesen von außen: eingehängt, Aufbau, gesperrte Abfragen.
#[unsafe(no_mangle)]
pub static METT_MOUSE: [AtomicU32; 3] = [const { AtomicU32::new(0) }; 3];

/// Hält die Maus an und zieht die Bewegung aus dieser Zeit danach ab, damit nichts springt.
#[derive(Debug, PartialEq)]
struct Filter {
    offset: [i64; 4],
    frozen: Option<Frozen>,
    /// Beim Schließen noch gedrückte Tasten zählen erst nach dem Loslassen wieder.
    held: u32,
    /// Letzte echte Werte, Reste unter einem Punkt und Film-Kamera, für den Zoom.
    last: Option<[i64; 4]>,
    rest: [f64; 2],
    film: [Smooth; 2],
    at: Option<std::time::Instant>,
}

/// Was der Zoom mit der Maus macht.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Zoom {
    factor: f64,
    /// Prozent, 0 aus.
    cinematic: f64,
    wheel: bool,
}

#[derive(Debug, PartialEq)]
struct Frozen {
    moves: [i64; 4],
    absolute: [i64; 2],
}

/// Bewegung x/y und Rad x/y.
const MOVES: [usize; 4] = [8, 16, 40, 48];
const ABSOLUTE: [usize; 2] = [24, 32];

fn get(b: &[u8; SIZE], at: usize) -> i64 {
    i64::from_le_bytes(std::array::from_fn(|i| b[at + i]))
}

fn put(b: &mut [u8; SIZE], at: usize, v: i64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

impl Filter {
    const fn new() -> Self {
        Self { offset: [0; 4], frozen: None, held: 0, last: None, rest: [0.0; 2], film: [Smooth::NEW; 2], at: None }
    }

    /// Verkleinert die Bewegung seit dem letzten Mal. Der Unterschied wandert in den Abzug, so springt nie etwas.
    fn slow(&mut self, real: [i64; 4], z: Zoom) {
        let now = std::time::Instant::now();
        let seconds = self.at.replace(now).map_or(0.0, |t| (now - t).as_secs_f64().min(0.1));
        let Some(last) = self.last.replace(real) else { return };
        let moved: [i64; 4] = std::array::from_fn(|i| real[i].wrapping_sub(last[i]));
        if z.cinematic <= 0.0 {
            self.film = [Smooth::default(); 2];
        }
        for (i, moved) in moved.iter().take(2).enumerate() {
            if z.factor == 1.0 && z.cinematic <= 0.0 {
                self.rest[i] = 0.0;
                continue;
            }
            let mut want = *moved as f64 * z.factor;
            if z.cinematic > 0.0 {
                want = self.film[i].next(want, (seconds / (z.cinematic / 100.0)).min(1.0));
            }
            let want = want + self.rest[i];
            let whole = want.trunc();
            self.rest[i] = want - whole;
            self.offset[i] = self.offset[i].wrapping_add(moved.wrapping_sub(whole as i64));
        }
        if z.wheel {
            (2..4).for_each(|i| self.offset[i] = self.offset[i].wrapping_add(moved[i]));
        }
    }

    fn apply(&mut self, b: &mut [u8; SIZE], blocked: bool, zoom: Zoom) {
        let real = MOVES.map(|at| get(b, at));
        if blocked {
            self.last = Some(real);
        } else {
            self.slow(real, zoom);
        }
        let buttons = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let shown: [i64; 4] = std::array::from_fn(|i| real[i].wrapping_sub(self.offset[i]));
        let out = if blocked {
            let absolute = ABSOLUTE.map(|at| get(b, at));
            let f = self.frozen.get_or_insert(Frozen { moves: shown, absolute });
            ABSOLUTE.iter().zip(f.absolute).for_each(|(at, v)| put(b, *at, v));
            self.held |= buttons;
            b[..4].fill(0);
            f.moves
        } else {
            if let Some(f) = self.frozen.take() {
                self.offset = std::array::from_fn(|i| real[i].wrapping_sub(f.moves[i]));
            }
            self.held &= buttons;
            b[..4].copy_from_slice(&(buttons & !self.held).to_le_bytes());
            std::array::from_fn(|i| real[i].wrapping_sub(self.offset[i]))
        };
        MOVES.iter().zip(out).for_each(|(at, v)| put(b, *at, v));
    }
}

fn write(at: usize, v: usize) -> bool {
    unsafe {
        let mut old = PAGE_PROTECTION_FLAGS::default();
        if VirtualProtect(at as *const _, 8, PAGE_READWRITE, &mut old).is_err() {
            return false;
        }
        (*(at as *const AtomicUsize)).store(v, SeqCst);
        let _ = VirtualProtect(at as *const _, 8, old, &mut old);
        true
    }
}

/// Erst merken, dann tauschen. Nur einmal je Platz.
fn hook(slot: &Slot, at: usize, new: usize) -> bool {
    if slot.at.compare_exchange(0, at, SeqCst, SeqCst).is_err() {
        return false;
    }
    slot.old.store(unsafe { *(at as *const usize) }, SeqCst);
    let ok = write(at, new);
    if !ok {
        slot.at.store(0, SeqCst);
    }
    ok
}

unsafe extern "system" fn current(this: Obj, kind: u32, device: *mut c_void, out: *mut Obj) -> HRESULT {
    INSIDE.fetch_add(1, SeqCst);
    let r = unsafe {
        let old = std::mem::transmute::<usize, Current>(READING.old.load(SeqCst));
        let r = old(this, kind, device, out);
        if r.is_ok()
            && OWN.load(SeqCst) != 0
            && let Some(o) = out.as_ref()
            && !o.is_null()
        {
            LAST.set((*o as usize, device as usize));
            // Der erste Mauswert des Spiels verrät die zweite Tabelle.
            if kind == KIND_MOUSE && STATE.at.load(SeqCst) == 0 {
                let table = **o;
                let mouse = hook(&STATE, table.add(MOUSE_STATE) as usize, state as *const () as usize);
                let keys = hook(&KEYS, table.add(KEY_STATE) as usize, key_state as *const () as usize);
                METT_MOUSE[0].store(1 + (mouse && keys) as u32, SeqCst);
            }
        }
        r
    };
    INSIDE.fetch_sub(1, SeqCst);
    r
}

unsafe extern "system" fn state(this: Obj, out: *mut u8) -> bool {
    INSIDE.fetch_add(1, SeqCst);
    let old = unsafe { std::mem::transmute::<usize, State>(STATE.old.load(SeqCst)) };
    let r = unsafe { old(this, out) };
    if r && !out.is_null() && layout_fits(this, old) {
        let _ = std::panic::catch_unwind(|| {
            let blocked = crate::input::blocked();
            if blocked {
                METT_MOUSE[2].fetch_add(1, SeqCst);
            }
            let b = unsafe { &mut *out.cast::<[u8; SIZE]>() };
            let (reading, device) = LAST.get();
            // Ohne bekanntes Gerät bleibt der Wert, wie er ist.
            if reading == this as usize {
                let (factor, cinematic, wheel) = crate::zoom::mouse(crate::input::control());
                let zoom = Zoom { factor, cinematic, wheel };
                lock(&FILTERS).entry(device).or_insert_with(Filter::new).apply(b, blocked, zoom);
            }
        });
    }
    INSIDE.fetch_sub(1, SeqCst);
    r
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe extern "system" fn key_state(this: Obj, max: u32, out: *mut Key) -> u32 {
    INSIDE.fetch_add(1, SeqCst);
    let old = unsafe { std::mem::transmute::<usize, Keys>(KEYS.old.load(SeqCst)) };
    let n = unsafe { old(this, max, out) };
    let n = if out.is_null() || n == 0 || n > max {
        n
    } else {
        let keys = unsafe { std::slice::from_raw_parts_mut(out, n as usize) };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let blocked = crate::input::blocked();
            escape_fallback(keys, blocked);
            let now = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() };
            if blocked {
                GRACE.store(now + GRACE_MS, SeqCst);
            }
            let grace = now < GRACE.load(SeqCst);
            filter_keys(keys, &mut lock(&HELD), blocked, grace, &|vk| {
                crate::input::opens(vk).is_some() || crate::input::tab(vk) || crate::input::zoom_key(vk).is_some()
            })
        }))
        .unwrap_or(n as usize) as u32
    };
    INSIDE.fetch_sub(1, SeqCst);
    n
}

/// Behält nur Tasten, die das Spiel sehen darf, und liefert deren Zahl.
fn filter_keys(
    keys: &mut [Key],
    held: &mut [bool; 256],
    blocked: bool,
    grace: bool,
    opens: &dyn Fn(u8) -> bool,
) -> usize {
    if blocked {
        keys.iter().for_each(|k| held[k.vk as usize] = true);
        // Esc und Eingabe schließen.
        held[0x1B] = true;
        held[0x0D] = true;
        return 0;
    }
    // Was nicht mehr gedrückt ist, gehört wieder dem Spiel.
    for (vk, h) in held.iter_mut().enumerate() {
        *h &= grace || keys.iter().any(|k| k.vk as usize == vk);
    }
    let mut n = 0;
    for i in 0..keys.len() {
        // Die Öffnen-Taste gehört immer uns, sonst sähe das Spiel sie vor uns.
        if !held[keys[i].vk as usize] && !opens(keys[i].vk) {
            keys[n] = keys[i];
            n += 1;
        }
    }
    n
}

/// Notausgang: Kommt Esc nicht als Fensternachricht an, schließt es trotzdem.
fn escape_fallback(keys: &[Key], blocked: bool) {
    let down = blocked && keys.iter().any(|k| k.vk == 0x1B);
    let now = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }.max(1);
    if !down {
        ESCAPE.store(0, SeqCst);
    } else if ESCAPE.compare_exchange(0, now, SeqCst, SeqCst).is_err() && now - ESCAPE.load(SeqCst) > 150 {
        crate::input::close();
    }
}

/// Einmal messen, wie viel die Funktion schreibt. Nur die bekannte Größe wird verändert.
fn layout_fits(this: Obj, old: State) -> bool {
    if LAYOUT.load(SeqCst) == 0 {
        let (mut a, mut b) = ([0x11u8; 256], [0xEEu8; 256]);
        let ok = unsafe { old(this, a.as_mut_ptr()) && old(this, b.as_mut_ptr()) };
        // Geschriebene Bytes stimmen überein, unberührte nicht.
        let fits = ok && (0..256).all(|i| (a[i] == b[i]) == (i < SIZE));
        LAYOUT.store(if fits { 1 } else { 2 }, SeqCst);
        METT_MOUSE[1].store(LAYOUT.load(SeqCst), SeqCst);
    }
    LAYOUT.load(SeqCst) == 1
}

/// `false`: ohne Maussperre weiter.
pub fn install() -> bool {
    unsafe {
        // Nur, wenn das Spiel GameInput selbst schon geladen hat.
        let Ok(dll) = GetModuleHandleW(w!("GameInputRedist.dll")) else { return false };
        let Some(f) = GetProcAddress(dll, s!("GameInputInitialize")) else { return false };
        type Init = unsafe extern "system" fn(*const GUID, *mut Obj) -> HRESULT;
        let init = std::mem::transmute::<unsafe extern "system" fn() -> isize, Init>(f);
        let mut input: Obj = std::ptr::null_mut();
        if init(&IGAMEINPUT, &mut input).is_err() || input.is_null() {
            return false;
        }
        lock(&FILTERS).clear();
        lock(&HELD).fill(false);
        GRACE.store(0, SeqCst);
        LAYOUT.store(0, SeqCst);
        OWN.store(input as usize, SeqCst);
        if !hook(&READING, (*input).add(CURRENT_READING) as usize, current as *const () as usize) {
            release();
            return false;
        }
        METT_MOUSE[0].store(1, SeqCst);
        true
    }
}

fn release() {
    let own = OWN.swap(0, SeqCst) as Obj;
    if !own.is_null() {
        unsafe { std::mem::transmute::<usize, Release>(*(*own).add(2))(own) };
    }
}

/// `false`: Jemand sitzt inzwischen vor uns oder ein Aufruf läuft noch. Dann muss die DLL geladen bleiben.
pub fn uninstall() -> bool {
    let mut clean = true;
    // Erst die Quelle schließen, damit sich nichts neu einhängt.
    let own = OWN.load(SeqCst);
    OWN.store(0, SeqCst);
    let ours = [current as *const () as usize, state as *const () as usize, key_state as *const () as usize];
    for (slot, mine) in [&READING, &STATE, &KEYS].into_iter().zip(ours) {
        let at = slot.at.load(SeqCst);
        if at == 0 {
            continue;
        }
        if unsafe { *(at as *const usize) } == mine && write(at, slot.old.load(SeqCst)) {
            slot.at.store(0, SeqCst);
        } else {
            clean = false;
        }
    }
    for _ in 0..100 {
        if INSIDE.load(SeqCst) == 0 {
            // Letzte Befehle hinter dem Zähler abwarten.
            std::thread::sleep(std::time::Duration::from_millis(50));
            OWN.store(own, SeqCst);
            release();
            return clean;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(buttons: u32, moves: [i64; 4], abs: [i64; 2]) -> [u8; SIZE] {
        let mut b = [0; SIZE];
        b[..4].copy_from_slice(&buttons.to_le_bytes());
        MOVES.iter().zip(moves).for_each(|(at, v)| put(&mut b, *at, v));
        ABSOLUTE.iter().zip(abs).for_each(|(at, v)| put(&mut b, *at, v));
        b
    }

    fn run(f: &mut Filter, blocked: bool, buttons: u32, moves: [i64; 4], abs: [i64; 2]) -> [u8; SIZE] {
        let mut b = value(buttons, moves, abs);
        f.apply(&mut b, blocked, Zoom { factor: 1.0, cinematic: 0.0, wheel: false });
        b
    }

    #[test]
    fn zoom_bremst_ohne_sprung() {
        let mut f = Filter::new();
        let go = |f: &mut Filter, x: i64, wheel: i64, z: Zoom| {
            let mut b = value(0, [x, -x, 0, wheel], [0; 2]);
            f.apply(&mut b, false, z);
            MOVES.map(|at| get(&b, at))
        };
        let off = Zoom { factor: 1.0, cinematic: 0.0, wheel: false };
        let on = Zoom { factor: 0.25, wheel: true, ..off };
        assert_eq!(go(&mut f, 100, 5, off), [100, -100, 0, 5]);
        // Kleine Schritte gehen nicht verloren, das Rad steht.
        for i in 1..=8 {
            go(&mut f, 100 + i, 5 + i, on);
        }
        assert_eq!(go(&mut f, 108, 13, on), [102, -102, 0, 5]);
        // Danach normal weiter, von dort aus.
        assert_eq!(go(&mut f, 118, 14, off), [112, -112, 0, 6]);
    }

    #[test]
    fn offen_steht_die_maus_und_danach_springt_nichts() {
        let mut f = Filter::new();
        // Ohne Sperre unverändert.
        assert_eq!(run(&mut f, false, 1, [10, 20, 0, 120], [5, 6]), value(1, [10, 20, 0, 120], [5, 6]));
        let still = value(0, [10, 20, 0, 120], [5, 6]);
        assert_eq!(run(&mut f, true, 0, [10, 20, 0, 120], [5, 6]), still);
        assert_eq!(run(&mut f, true, 2, [500, -90, 0, 360], [70, 80]), still);
        // Danach zählt nur, was nach dem Schließen passiert.
        assert_eq!(run(&mut f, false, 0, [500, -90, 0, 360], [70, 80]), value(0, [10, 20, 0, 120], [70, 80]));
        assert_eq!(run(&mut f, false, 0, [503, -88, 0, 480], [73, 82]), value(0, [13, 22, 0, 240], [73, 82]));
        // Zweite Sperre baut auf der ersten auf.
        assert_eq!(run(&mut f, true, 0, [600, 0, 0, 480], [0, 0]), value(0, [110, 110, 0, 240], [0, 0]));
        assert_eq!(run(&mut f, false, 0, [700, 5, 0, 480], [1, 1]), value(0, [110, 110, 0, 240], [1, 1]));
    }

    #[test]
    fn tasten_verschwinden_und_kommen_erst_nach_dem_loslassen_wieder() {
        let key = |vk| Key { vk, ..Default::default() };
        let mut held = [false; 256];
        let mut run = |vks: &[u8], blocked: bool, grace: bool| {
            let mut keys: Vec<Key> = vks.iter().map(|v| key(*v)).collect();
            let n = filter_keys(&mut keys, &mut held, blocked, grace, &|vk| vk == b'T');
            keys[..n].iter().map(|k| k.vk).collect::<Vec<_>>()
        };
        assert_eq!(run(b"WT", false, false), b"W");
        assert_eq!(run(b"WA", true, true), b"");
        // Esc meldet GameInput erst nach dem Schließen: trotzdem kein Pausenmenü.
        assert_eq!(run(b"D", false, true), b"D");
        assert_eq!(run(b"\x1bD", false, true), b"D");
        assert_eq!(run(b"\x1bD", false, false), b"D");
        assert_eq!(run(b"D", false, false), b"D");
        assert_eq!(run(b"\x1bW", false, false), b"\x1bW");
        assert_eq!(size_of::<Key>(), 12);
    }

    #[test]
    fn jede_maus_hat_eigene_zaehler() {
        let (mut a, mut b) = (Filter::new(), Filter::new());
        run(&mut a, true, 0, [9000, 9000, 0, 0], [0; 2]);
        run(&mut b, true, 0, [0; 4], [0; 2]);
        assert_eq!(get(&run(&mut a, false, 0, [9100, 9000, 0, 0], [0; 2]), 8), 9000);
        assert_eq!(get(&run(&mut b, false, 0, [0; 4], [0; 2]), 8), 0);
    }

    #[test]
    fn beim_schliessen_gehaltene_taste_schlaegt_nicht() {
        let mut f = Filter::new();
        run(&mut f, true, 1, [0; 4], [0; 2]);
        assert_eq!(run(&mut f, false, 1, [0; 4], [0; 2])[0], 0);
        assert_eq!(run(&mut f, false, 3, [0; 4], [0; 2])[0], 2);
        assert_eq!(run(&mut f, false, 0, [0; 4], [0; 2])[0], 0);
        assert_eq!(run(&mut f, false, 1, [0; 4], [0; 2])[0], 1);
    }
}
