//! AppleSkin: liest die eigenen Werte und die Lage der Hungerleiste aus dem laufenden Spiel. Liest nur. Läuft in einem
//! eigenen Thread.
use gamemem::Reader;
use gamemem::attributes::{self, Block, Names, Own, Values};
use gamemem::uitree::{self, Layout, Spot};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Werte und rechte obere Ecke der Hungerleiste, in Anteilen der Bildhöhe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub values: Values,
    pub right: f32,
    pub top: f32,
    /// Einheit der Oberfläche.
    pub unit: f32,
}

const BAR: &str = "hunger_rend";
const FRESH: Duration = Duration::from_millis(100);
const FIRST: Duration = Duration::from_secs(1);
const SLOWEST: Duration = Duration::from_secs(60);
/// Menüs verdecken die Leiste nur; erst wenn sie beim Spielen so lange fehlt, gilt sie als weg.
const GONE: Duration = Duration::from_secs(3);
/// Nach einem Menü nicht gleich wieder alles durchsuchen.
const REST: Duration = Duration::from_secs(15);
/// So oft werden schon gefundene Blöcke verglichen; das ist billig.
const LOOK: Duration = Duration::from_millis(500);

pub struct AppleLink {
    jobs: Sender<Option<Option<(u32, bool)>>>,
    shared: Arc<Mutex<(&'static str, Option<View>)>>,
    last: Option<(u32, bool)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AppleLink {
    pub fn start() -> Self {
        let (jobs, inbox) = channel();
        let shared = Arc::new(Mutex::new(("aus", None)));
        let s = shared.clone();
        let thread = std::thread::spawn(move || Worker::new(s).run(inbox));
        Self { jobs, shared, last: None, thread: Some(thread) }
    }

    /// Prozess und ob gerade gespielt wird.
    pub fn want(&mut self, want: Option<(u32, bool)>) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Some(want));
        }
    }

    pub fn state(&self) -> &'static str {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).0
    }

    pub fn view(&self) -> Option<View> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).1
    }
}

impl Drop for AppleLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(None);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Game {
    pid: u32,
    reader: Reader,
    names: Option<Names>,
    layout: Option<Layout>,
}

struct Worker {
    shared: Arc<Mutex<(&'static str, Option<View>)>>,
    want: Option<(u32, bool)>,
    game: Option<Game>,
    /// Ergebnis der letzten Suche.
    blocks: Vec<Block>,
    looked: Instant,
    /// Letzte Suche.
    searched: Option<Instant>,
    /// Die Leiste war zuletzt zu sehen, oder es wurde nicht gespielt.
    seen: Instant,
    own: Option<Own>,
    bar: Option<Spot>,
    next: Instant,
    wait: Duration,
}

impl Worker {
    fn new(shared: Arc<Mutex<(&'static str, Option<View>)>>) -> Self {
        Self {
            shared,
            want: None,
            game: None,
            blocks: vec![],
            looked: Instant::now(),
            searched: None,
            seen: Instant::now(),
            own: None,
            bar: None,
            next: Instant::now(),
            wait: FIRST,
        }
    }

    fn run(mut self, inbox: Receiver<Option<Option<(u32, bool)>>>) {
        loop {
            match inbox.recv_timeout(FRESH) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Some(w)) => {
                    if w.map(|w| w.0) != self.want.map(|w| w.0) {
                        (self.game, self.own, self.bar) = (None, None, None);
                        self.blocks.clear();
                        self.searched = None;
                    }
                    if w.is_some_and(|w| w.1) && !self.want.is_some_and(|w| w.1) {
                        self.wait = FIRST;
                        self.next = self.searched.map_or(Instant::now(), |t| t + REST);
                    }
                    self.want = w;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = self.step();
            *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = now;
        }
    }

    fn step(&mut self) -> (&'static str, Option<View>) {
        let Some((pid, playing)) = self.want else { return ("aus", None) };
        if self.game.is_none() {
            let Ok(reader) = Reader::open_for_scan(pid) else { return ("Spiel nicht lesbar", None) };
            self.game = Some(Game { pid, reader, names: None, layout: None });
        }
        let Some(g) = &mut self.game else { return ("aus", None) };
        let values = self.own.as_ref().and_then(|o| o.read(&g.reader));
        if values.is_none() {
            self.own = None;
        }
        if self.own.is_none() && !self.blocks.is_empty() && self.looked.elapsed() >= LOOK {
            self.looked = Instant::now();
            self.own = attributes::find(&g.reader, &self.blocks);
        }
        let bar = self.bar.zip(g.layout).and_then(|(b, l)| b.now(&g.reader, &l));
        if bar.is_some() || !playing {
            self.seen = Instant::now();
        } else if self.seen.elapsed() >= GONE {
            self.bar = None;
        }
        if (self.own.is_none() || self.bar.is_none()) && playing && Instant::now() >= self.next {
            if g.names.is_none()
                && let Ok((base, image)) = gamemem::image(g.pid)
            {
                g.names = attributes::names(&g.reader, base, &image);
            }
            if g.layout.is_none()
                && let Ok(range) = gamemem::image_range(g.pid)
            {
                g.layout = uitree::layout(&g.reader, &range);
            }
            if let Some(n) = g.names.filter(|_| self.own.is_none()) {
                self.blocks = attributes::blocks(&g.reader, &n);
                self.own = attributes::find(&g.reader, &self.blocks);
            }
            if let Some(l) = g.layout.filter(|_| self.bar.is_none()) {
                self.bar = uitree::spot(&g.reader, &l, BAR);
            }
            self.searched = Some(Instant::now());
            self.next = Instant::now() + self.wait;
            self.wait = (self.wait * 3 / 2).min(SLOWEST);
        }
        match (values, bar) {
            (Some(values), Some((x, y, height))) => {
                ("läuft", Some(View { values, right: x / height, top: y / height, unit: 1.0 / height }))
            }
            (None, _) if g.names.is_none() => ("eigene Werte werden gesucht", None),
            // Vorher sehen alle Werte aus wie bei anderen Spielern.
            (None, _) => ("wartet, bis sich Hunger oder Sättigung ändert", None),
            (_, None) => ("keine Hungerleiste zu sehen", None),
        }
    }
}
