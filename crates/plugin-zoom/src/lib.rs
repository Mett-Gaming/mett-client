//! Zoom: rechnet den Verlauf und setzt das Sichtfeld, eine benannte Einstellung des laufenden Spiels. Tasten und Maus
//! laufen im HUD-Plugin. Endet der Client oder das HUD-Plugin, kommt das Sichtfeld des Nutzers sofort zurück.
use hud::zoom::{Curve, Second, Settings, TICK};
use mett_plugin::{API, Host, Info};
use std::panic::catch_unwind;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::SeqCst};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::*;
use windows::core::{PCWSTR, s, w};

const NAME: &str = "gfx_field_of_view";

mod answer {
    pub const OK: u32 = 0;
    pub const BAD_ORDER: u32 = 2;
    pub const STALE: u32 = 3;
}

/// Auftrag: `client an tabelle name wert einstellung beschreibung|Zoom-Einstellungen`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Order {
    client: u32,
    on: bool,
    table: u64,
    name: u64,
    value: u64,
    at: u64,
    info: u64,
    settings: Settings,
}

impl Order {
    fn decode(s: &str) -> Option<Self> {
        let (head, rest) = s.split_once('|')?;
        let v: Vec<u64> = head.split(' ').map(|t| u64::from_str_radix(t, 16).ok()).collect::<Option<_>>()?;
        let [client, on, table, name, value, at, info] = v[..] else { return None };
        // Vor dem Wert liegen die zwei Grenzen.
        let value_fits = (0x18..0x30).contains(&value) && value.is_multiple_of(4);
        let ok = value_fits && on < 2 && name.is_multiple_of(8) && name < 0x800;
        let settings = Settings::decode(rest)?;
        ok.then_some(Self { client: u32::try_from(client).ok()?, on: on == 1, table, name, value, at, info, settings })
    }
}

static IMAGE: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static ORDER: Mutex<Option<Order>> = Mutex::new(None);
static KEEPER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);

// Plätze im HUD-Plugin, siehe dort.
const HELD: usize = 0;
const PRESSES: usize = 1;
const STEPS: usize = 2;
const FACTOR: usize = 3;
const FEWEST: usize = 4;
const MOST: usize = 5;
const ZOOMING: usize = 6;
const BEAT: usize = 7;
const SECOND: usize = 8;
const HIDE: usize = 9;
const CINEMATIC: usize = 10;
type Shared = [AtomicU32; 12];

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn usable(at: u64, size: u64) -> bool {
    let mut m = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(at as *const _), &mut m, size_of::<MEMORY_BASIC_INFORMATION>()) };
    let end = m.BaseAddress as u64 + m.RegionSize as u64;
    let open = n != 0 && m.State == MEM_COMMIT && m.Protect == PAGE_READWRITE;
    open && at >= 0x10000 && at.is_multiple_of(8) && at.checked_add(size).is_some_and(|e| e <= end)
}

unsafe fn word(at: u64) -> u64 {
    unsafe { *(at as *const u64) }
}

impl Order {
    /// Ist das noch genau diese Einstellung?
    fn fits(&self) -> bool {
        let Some(text) = self.info.checked_add(self.name) else { return false };
        if !usable(self.at, 0x40) || !usable(text, 32) {
            return false;
        }
        unsafe {
            let (start, len) = (word(text), word(text + 16));
            word(self.at) == self.table
                && word(self.at + 8) == self.info
                && len == NAME.len() as u64
                && usable(start & !7, len + 8)
                && std::slice::from_raw_parts(start as *const u8, NAME.len()) == NAME.as_bytes()
        }
    }

    /// Kleinster und größter Wert, dann der Wert selbst.
    fn fields(&self) -> &[AtomicU32; 3] {
        unsafe { &*((self.at + self.value - 8) as *const [AtomicU32; 3]) }
    }
}

/// Kleiner als jedes gezoomte Sichtfeld.
const LEAST: f32 = 0.01;

/// Sichtfeld des Nutzers und unseres. Minecraft begrenzt es beim Zeichnen mit der Untergrenze der Einstellung selbst,
/// deshalb wird sie währenddessen mit gesenkt.
#[derive(Default)]
struct View {
    user: f32,
    set: Option<f32>,
    /// Untergrenze des Spiels, solange unsere gilt.
    least: Option<f32>,
}

impl View {
    /// `false`: Der Wert ist unbrauchbar.
    fn apply(&mut self, [low, high, field]: &[AtomicU32; 3], divisor: f64) -> bool {
        let get = |f: &AtomicU32| f32::from_bits(f.load(SeqCst));
        let now = get(field);
        let (least, most) = (get(low), get(high));
        let bounds = least.is_finite() && most.is_finite() && least > 0.0 && least < most && most <= 360.0;
        if !bounds || now > most {
            return false;
        }
        if !now.is_finite() || !(0.01..=360.0).contains(&now) {
            return false;
        }
        // Der Nutzer hat selbst umgestellt, oder wir fangen gerade an.
        if self.set != Some(now) {
            self.user = now;
        }
        if divisor <= 1.0 {
            if self.set.take().is_some_and(|s| s == now) {
                field.store(self.user.to_bits(), SeqCst);
            }
            // Erst der Wert, dann die Grenze.
            if let Some(old) = self.least.take().filter(|_| least == LEAST) {
                low.store(old.to_bits(), SeqCst);
            }
        } else {
            if least != LEAST {
                self.least = Some(least);
                low.store(LEAST.to_bits(), SeqCst);
            }
            let want = (self.user as f64 / divisor) as f32;
            field.store(want.to_bits(), SeqCst);
            self.set = Some(want);
        }
        true
    }
}

/// Alles, was sich zwischen zwei Takten merkt.
#[derive(Default)]
struct Brain {
    curve: Curve,
    second: Second,
    presses: u32,
    toggled: bool,
    second_presses: u32,
    second_on: bool,
    /// Zeit seit dem letzten Takt.
    part: f64,
    started: bool,
}

impl Brain {
    /// Ein Bild weiter; liefert den Teiler des Sichtfelds und ob der Haupt-Zoom an ist.
    fn frame(&mut self, s: &Settings, hud: &Shared, seconds: f64) -> (f64, bool) {
        let get = |i: usize| hud[i].load(SeqCst);
        if !std::mem::replace(&mut self.started, true) {
            (self.presses, self.second_presses) = (get(PRESSES), get(SECOND));
        }
        if (get(PRESSES).wrapping_sub(std::mem::replace(&mut self.presses, get(PRESSES)))) & 1 == 1 {
            self.toggled = !self.toggled;
        }
        if (get(SECOND).wrapping_sub(std::mem::replace(&mut self.second_presses, get(SECOND)))) & 1 == 1 {
            self.second_on = !self.second_on;
        }
        self.second_on &= s.second;
        self.toggled &= s.toggle;
        let zooming = if s.toggle { self.toggled } else { get(HELD) != 0 };
        if !zooming && !s.retain {
            hud[STEPS].store(0, SeqCst);
        }
        let steps = get(STEPS) as i32;
        // Nach einer Pause nicht alles nachholen.
        self.part += (seconds / TICK).min(4.0);
        while self.part >= 1.0 {
            self.part -= 1.0;
            self.curve.tick(s, zooming, steps);
            self.second.tick(s, self.second_on);
        }
        let main = self.curve.divisor(s, zooming, self.part);
        (main * self.second.divisor(s, self.part), zooming)
    }

    fn idle(&self) -> bool {
        self.curve.idle() && self.second.idle() && !self.second_on
    }
}

/// Eingaben im HUD-Plugin. Hält es geladen, solange wir darauf zugreifen.
struct Hud(HMODULE, *const Shared);

impl Hud {
    fn open() -> Option<Self> {
        unsafe {
            let mut m = HMODULE::default();
            GetModuleHandleExW(0, w!("hud.dll"), &mut m).ok()?;
            match GetProcAddress(m, s!("METT_ZOOM")) {
                Some(f) => Some(Self(m, f as *const Shared)),
                None => {
                    let _ = FreeLibrary(m);
                    None
                }
            }
        }
    }

    fn shared(&self) -> &Shared {
        unsafe { &*self.1 }
    }
}

impl Drop for Hud {
    fn drop(&mut self) {
        for i in [FACTOR, ZOOMING, BEAT, HIDE, CINEMATIC] {
            self.shared()[i].store(0, SeqCst);
        }
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}

fn publish(hud: &Shared, s: &Settings, divisor: f64, zooming: bool, second: bool) {
    let put = |i: usize, v: u32| hud[i].store(v, SeqCst);
    put(FACTOR, (s.mouse(divisor) as f32).to_bits());
    put(FEWEST, s.least_steps() as u32);
    put(MOST, s.steps as u32);
    put(ZOOMING, zooming as u32);
    put(HIDE, (second && s.second_hides_hud) as u32);
    put(CINEMATIC, if zooming || second { s.cinematic.max(if second { 100 } else { 0 }) as u32 } else { 0 });
    put(BEAT, (unsafe { GetTickCount64() } as u32).max(1));
}

struct Client(u32, HANDLE);

impl Client {
    fn alive(watched: &mut Option<Self>, pid: u32) -> bool {
        if watched.as_ref().is_none_or(|w| w.0 != pid) {
            *watched = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok().map(|h| Self(pid, h));
        }
        watched.as_ref().is_some_and(|w| unsafe { WaitForSingleObject(w.1, 0) } == WAIT_TIMEOUT)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.1);
        }
    }
}

/// Genaue kurze Pausen; die üblichen sind mit 15 ms zu grob für ein weiches Bild.
struct Timer(Option<HANDLE>);

impl Timer {
    fn new() -> Self {
        let flags = CREATE_WAITABLE_TIMER_HIGH_RESOLUTION;
        Self(unsafe { CreateWaitableTimerExW(None, PCWSTR::null(), flags, TIMER_ALL_ACCESS.0) }.ok())
    }

    fn wait(&self, d: Duration) {
        unsafe {
            if let Some(h) = self.0
                && SetWaitableTimer(h, &-(d.as_micros() as i64 * 10), 0, None, None, false).is_ok()
            {
                WaitForSingleObject(h, 100);
                return;
            }
        }
        std::thread::sleep(d);
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(h) = self.0 {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

fn keep() {
    let (mut hud, mut client, mut brain, mut view) = (None, None, Brain::default(), View::default());
    let timer = Timer::new();
    let mut last = Instant::now();
    let mut held: Option<Order> = None;
    loop {
        let stop = STOP.load(SeqCst);
        let order = lock(&ORDER).filter(|o| o.on && !stop);
        let ready = order.filter(|o| Client::alive(&mut client, o.client) && o.fits());
        if hud.is_none() && ready.is_some() {
            hud = Hud::open();
        }
        let now = Instant::now();
        let seconds = (now - std::mem::replace(&mut last, now)).as_secs_f64();
        let mut busy = false;
        match (ready, &hud) {
            (Some(o), Some(h)) => {
                let (divisor, zooming) = brain.frame(&o.settings, h.shared(), seconds);
                publish(h.shared(), &o.settings, divisor, zooming, brain.second_on);
                if !view.apply(o.fields(), divisor) {
                    brain = Brain::default();
                }
                held = Some(o);
                busy = !brain.idle();
            }
            _ => {
                // Zurück auf das Sichtfeld des Nutzers, wenn es die Einstellung noch gibt.
                if let Some(o) = held.take().filter(Order::fits) {
                    view.apply(o.fields(), 1.0);
                }
                brain = Brain::default();
                hud = None;
            }
        }
        if stop {
            return;
        }
        timer.wait(Duration::from_micros(if busy { 1000 } else { 5000 }));
    }
}

/// # Safety
/// `text` zeigt auf nullterminierten UTF-16-Text.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_zoom_order(text: *const u16) -> u32 {
    catch_unwind(|| {
        if text.is_null() || STOP.load(SeqCst) {
            return answer::BAD_ORDER;
        }
        let text = unsafe { PCWSTR(text).to_string() }.unwrap_or_default();
        let Some(o) = Order::decode(&text) else { return answer::BAD_ORDER };
        let (base, size) = (IMAGE[0].load(SeqCst), IMAGE[1].load(SeqCst));
        if !(base..base + size).contains(&o.table) {
            return answer::BAD_ORDER;
        }
        if o.on && !o.fits() {
            return answer::STALE;
        }
        *lock(&ORDER) = Some(o);
        answer::OK
    })
    .unwrap_or(answer::BAD_ORDER)
}

/// # Safety
/// Wird nur vom Kern mit gültigen Zeigern aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_query(host: *const Host, info: *mut Info) -> i32 {
    unsafe {
        *info = Info { size: size_of::<Info>() as u32, api: API, version: 1 };
        if (*host).api == API { 0 } else { 1 }
    }
}

/// # Safety
/// Wird nur vom Kern mit gültigem Zeiger aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_load(host: *const Host) -> i32 {
    unsafe {
        IMAGE[0].store((*host).base as u64, SeqCst);
        IMAGE[1].store((*host).image_size as u64, SeqCst);
    }
    STOP.store(false, SeqCst);
    *lock(&ORDER) = None;
    *lock(&KEEPER) = Some(std::thread::spawn(keep));
    0
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

    fn shared() -> Shared {
        [const { AtomicU32::new(0) }; 12]
    }

    #[test]
    fn sichtfeld_wird_geteilt_und_kommt_zurueck() {
        let f = [30f32, 110.0, 110.0].map(|v| AtomicU32::new(v.to_bits()));
        let get = || f32::from_bits(f[2].load(SeqCst));
        let low = || f32::from_bits(f[0].load(SeqCst));
        let mut v = View::default();
        assert!(v.apply(&f, 1.0) && get() == 110.0);
        assert!(low() == 30.0);
        assert!(v.apply(&f, 4.0) && get() == 27.5 && low() == LEAST);
        assert!(v.apply(&f, 2.0) && get() == 55.0);
        // Der Nutzer stellt mittendrin um.
        f[2].store(80f32.to_bits(), SeqCst);
        assert!(v.apply(&f, 2.0) && get() == 40.0);
        assert!(v.apply(&f, 1.0) && get() == 80.0 && low() == 30.0);
        // Und von vorn, auch viel weiter.
        assert!(v.apply(&f, 200.0) && get() == 0.4 && v.apply(&f, 1.0) && (get(), low()) == (80.0, 30.0));
        // Unsinn wird nicht angefasst.
        f[2].store(f32::NAN.to_bits(), SeqCst);
        assert!(!v.apply(&f, 4.0) && get().is_nan());
        f[2].store(80f32.to_bits(), SeqCst);
        for (i, bad) in [(0, 0.0f32), (0, 500.0), (1, f32::NAN), (1, 50.0)] {
            let keep = f[i].swap(bad.to_bits(), SeqCst);
            assert!(!View::default().apply(&f, 4.0) && get() == 80.0, "{i} {bad}");
            f[i].store(keep, SeqCst);
        }
    }

    #[test]
    fn halten_und_mausrad() {
        let (s, hud, mut b) = (Settings::default(), shared(), Brain::default());
        let run = |b: &mut Brain, seconds: f64| {
            let mut d = (1.0, false);
            (0..(seconds * 1000.0) as usize).for_each(|_| d = b.frame(&s, &hud, 0.001));
            d
        };
        assert_eq!(run(&mut b, 0.2), (1.0, false));
        hud[HELD].store(1, SeqCst);
        let d = run(&mut b, 1.2);
        assert!((d.0 - 4.0).abs() < 1e-6 && d.1 && !b.idle());
        hud[STEPS].store(2, SeqCst);
        assert!((run(&mut b, 3.0).0 - 9.0).abs() < 0.01);
        hud[HELD].store(0, SeqCst);
        let d = run(&mut b, 0.8);
        assert!((d.0 - 1.0).abs() < 1e-6 && !d.1 && b.idle(), "{d:?}");
        // Die Stufen sind danach wieder auf null.
        assert_eq!(hud[STEPS].load(SeqCst), 0);
    }

    #[test]
    fn umschalten_und_zweiter_zoom() {
        let s = Settings { toggle: true, ..Settings::default() };
        let (hud, mut b) = (shared(), Brain::default());
        hud[PRESSES].store(5, SeqCst);
        // Frühere Tastendrücke zählen nicht.
        assert!(!b.frame(&s, &hud, 0.001).1);
        hud[PRESSES].store(6, SeqCst);
        assert!(b.frame(&s, &hud, 0.001).1 && b.frame(&s, &hud, 0.001).1);
        hud[PRESSES].store(7, SeqCst);
        assert!(!b.frame(&s, &hud, 0.001).1);
        hud[SECOND].fetch_add(1, SeqCst);
        (0..2100).for_each(|_| {
            b.frame(&s, &hud, 0.005);
        });
        assert!((b.frame(&s, &hud, 0.0).0 - 4.0).abs() < 1e-6 && b.second_on);
        // Abgeschaltet gibt es ihn nicht.
        let off = Settings { second: false, ..s };
        b.frame(&off, &hud, 0.0);
        assert!(!b.second_on);
    }

    #[test]
    fn auftrag_wird_geprueft() {
        let rest = Settings::default().encode();
        let good = format!("7 1 100 188 18 1000 2000|{rest}");
        let o = Order::decode(&good).unwrap();
        assert!(o.on && (o.client, o.name, o.value, o.at) == (7, 0x188, 0x18, 0x1000));
        for bad in
            ["", "1|2", &good.replace(" 188 ", " 189 "), &good.replace(" 18 ", " 8 "), &good.replace("7 1 ", "7 2 ")]
        {
            assert_eq!(Order::decode(bad), None, "{bad}");
        }
        // Eine Adresse, an der nichts liegt, gilt nicht.
        assert!(!o.fits());
    }
}
