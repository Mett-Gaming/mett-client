//! Liest eigenes Inventar, eigene Rüstung und eigene Effekte aus dem laufenden Spiel. Liest nur, nur solange eines der
//! Module an ist, und speichert nichts. Läuft in einem eigenen Thread.
use gamemem::Reader;
use gamemem::inventory::{self, Effect, Kinds, Own, Places, Watch};
use gamemem::uitree::{self, Layout, Spot};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FRESH: Duration = Duration::from_millis(250);
const FIRST: Duration = Duration::from_secs(2);
const SLOWEST: Duration = Duration::from_secs(60);
/// So oft wird nach der Rüstung gesehen, solange keine da ist.
const WORN: Duration = Duration::from_secs(5);
/// Ein erster Effekt soll nicht zu lange auf sich warten lassen.
const EFFECTS: Duration = Duration::from_secs(20);
/// Nach einem Menü nicht gleich wieder alles durchsuchen.
const REST: Duration = Duration::from_secs(10);
/// Minecrafts Schnellleiste.
const BAR: &str = "desktop_hotbar";
/// Menüs verdecken sie nur.
const GONE: Duration = Duration::from_secs(3);
const RUNNING: &str = "läuft";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Want {
    pub pid: u32,
    pub playing: bool,
    /// Inventar und Rüstung, Effekte.
    pub items: bool,
    pub effects: bool,
    /// Lage der Schnellleiste.
    pub bar: bool,
}

#[derive(Clone, Debug, Default)]
pub struct View {
    pub own: Option<Own>,
    /// Schnellleiste: links und oben in Anteilen der Bildhöhe, dazu die Einheit der Oberfläche.
    pub bar: Option<(f32, f32, f32)>,
    pub effects: Option<Vec<Effect>>,
}

#[derive(Default)]
struct Shared {
    states: [&'static str; 2],
    view: View,
}

pub struct GearLink {
    jobs: Sender<Option<Option<Want>>>,
    shared: Arc<Mutex<Shared>>,
    last: Option<Want>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl GearLink {
    pub fn start() -> Self {
        let (jobs, inbox) = channel();
        let shared = Arc::new(Mutex::new(Shared { states: ["aus"; 2], view: View::default() }));
        let s = shared.clone();
        let thread = std::thread::spawn(move || Worker::new(s).run(inbox));
        Self { jobs, shared, last: None, thread: Some(thread) }
    }

    pub fn want(&mut self, want: Option<Want>) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Some(want));
        }
    }

    /// Gegenstände, Effekte.
    pub fn states(&self) -> [&'static str; 2] {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).states
    }

    pub fn view(&self) -> View {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).view.clone()
    }
}

impl Drop for GearLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(None);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Wann wieder gesucht wird.
struct Pace {
    next: Instant,
    wait: Duration,
    last: Option<Instant>,
}

impl Pace {
    fn new() -> Self {
        Self { next: Instant::now(), wait: FIRST, last: None }
    }

    fn due(&self) -> bool {
        Instant::now() >= self.next
    }

    fn done(&mut self) {
        self.last = Some(Instant::now());
        self.next = Instant::now() + self.wait;
        self.wait = (self.wait * 3 / 2).min(SLOWEST);
    }

    fn soon(&mut self) {
        self.wait = FIRST;
        self.next = self.last.map_or(Instant::now(), |t| t + REST);
    }
}

struct Worker {
    shared: Arc<Mutex<Shared>>,
    want: Option<Want>,
    reader: Option<Reader>,
    kinds: Option<Kinds>,
    places: Places,
    watch: Watch,
    lists: Vec<u64>,
    layout: Option<Layout>,
    bar: Option<Spot>,
    /// Die Leiste war zuletzt zu sehen, oder es wurde nicht gespielt.
    seen: Instant,
    bars: Pace,
    items: Pace,
    effects: Pace,
    /// Letzte Suche nach der Rüstung allein.
    worn: Instant,
}

impl Worker {
    fn new(shared: Arc<Mutex<Shared>>) -> Self {
        Self {
            shared,
            want: None,
            reader: None,
            kinds: None,
            places: Places::default(),
            watch: Watch::default(),
            lists: vec![],
            layout: None,
            bar: None,
            seen: Instant::now(),
            bars: Pace::new(),
            items: Pace::new(),
            effects: Pace::new(),
            worn: Instant::now(),
        }
    }

    fn run(mut self, inbox: Receiver<Option<Option<Want>>>) {
        loop {
            match inbox.recv_timeout(FRESH) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Some(w)) => self.change(w),
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = self.step();
            *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = now;
        }
    }

    fn change(&mut self, w: Option<Want>) {
        if w.map(|w| w.pid) != self.want.map(|w| w.pid) {
            *self = Self::new(self.shared.clone());
        } else if w.is_some_and(|w| w.playing) && !self.want.is_some_and(|w| w.playing) {
            self.items.soon();
            self.effects.soon();
            self.bars.soon();
        }
        self.want = w;
    }

    fn step(&mut self) -> Shared {
        let off = |s| Shared { states: [s; 2], view: View::default() };
        let Some(w) = self.want else { return off("aus") };
        if self.reader.is_none() {
            self.reader = Reader::open_for_scan(w.pid).ok();
        }
        let Some(r) = &self.reader else { return off("Spiel nicht lesbar") };
        let mut out = off("aus");
        if w.items {
            if let Some(k) = &self.kinds {
                out.view.own = self.watch.read(r, k, &self.places);
            }
            if out.view.own.is_none() && w.playing && self.items.due() {
                if self.kinds.is_none()
                    && let Ok(image) = gamemem::image_range(w.pid)
                {
                    self.kinds = inventory::kinds(r, &image);
                }
                if let Some(k) = &self.kinds {
                    self.places = inventory::find(r, k);
                    self.watch = Watch::default();
                    out.view.own = self.watch.read(r, k, &self.places);
                }
                self.items.done();
            }
            // Ohne Rüstung könnte sie inzwischen woanders liegen, etwa nach einem Rundenwechsel.
            let bare = out.view.own.as_ref().is_some_and(|o| o.worn.is_empty());
            if bare
                && w.playing
                && self.worn.elapsed() >= WORN
                && let Some(k) = &self.kinds
            {
                inventory::find_worn(r, k, &mut self.places);
                self.worn = Instant::now();
            }
            out.states[0] = match (&out.view.own, &self.kinds) {
                (Some(_), _) => RUNNING,
                (None, Some(_)) => "eigenes Inventar nicht eindeutig gefunden",
                (None, None) => "wird gesucht",
            };
        }
        if w.bar {
            let now = self.bar.zip(self.layout).and_then(|(b, l)| b.now(r, &l));
            if now.is_some() || !w.playing {
                self.seen = Instant::now();
            } else if self.seen.elapsed() >= GONE {
                self.bar = None;
            }
            if self.bar.is_none() && w.playing && self.bars.due() {
                if self.layout.is_none()
                    && let Ok(image) = gamemem::image_range(w.pid)
                {
                    self.layout = uitree::layout(r, &image);
                }
                self.bar = self.layout.and_then(|l| uitree::spot(r, &l, BAR));
                self.bars.done();
            }
            out.view.bar = now.map(|(x, y, height)| (x / height, y / height, 1.0 / height));
        }
        if w.effects {
            out.view.effects = inventory::own_effects(r, &self.lists);
            // Bekannte Listen bleiben; nur ohne sie wird gesucht, und das selten.
            if out.view.effects.is_none() && w.playing && self.effects.due() {
                self.lists = inventory::effects(r);
                out.view.effects = inventory::own_effects(r, &self.lists);
                self.effects.done();
                self.effects.wait = self.effects.wait.min(EFFECTS);
            }
            out.states[1] = if out.view.effects.is_some() { RUNNING } else { "keine Effekte gefunden" };
        }
        out
    }
}
