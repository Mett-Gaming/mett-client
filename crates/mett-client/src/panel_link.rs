//! Minecrafts eigene Chat-Anzeige verschieben: sucht sie über ihren Namen und reicht die gewünschte Lage ans Plugin
//! weiter. Läuft in einem eigenen Thread, damit die Anzeige nie wartet.
use crate::plugins::Plugins;
use gamemem::Reader;
use gamemem::uitree::{self, Layout, Panel};
use mett_plugin::panel::{Offsets, Order, PLUGIN, Wish, answer};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::s;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goal {
    /// Linker und unterer Rand sowie Bildschirmhöhe in Pixeln.
    Place {
        x: f32,
        bottom: f32,
        height: f32,
        /// So viele der neuesten Nachrichten.
        rows: u32,
        /// Ohne sanftes Ausblenden.
        sudden: bool,
        /// Breite und Höhe in Pixeln; gilt, wenn die gespeicherten Maße bekannt sind.
        size: (f32, f32),
        java: bool,
    },
    Hide,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Want {
    pub pid: u32,
    pub playing: bool,
    pub goal: Goal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Off(&'static str),
    Searching,
    Ready,
}

impl State {
    pub fn text(self) -> &'static str {
        match self {
            State::Off(why) => why,
            State::Searching => "wird in der Welt gesucht",
            State::Ready => "läuft",
        }
    }
}

/// Maße von Minecrafts Anzeige auf dem Bildschirm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Pixel je Einheit der Oberfläche.
    pub unit: f32,
    pub width: f32,
    /// Breite und Höhe lassen sich frei wählen.
    pub free: bool,
}

pub struct PanelLink {
    look: Arc<Mutex<Option<(f32, f32, bool)>>>,
    jobs: Sender<Option<Option<Want>>>,
    state: Arc<Mutex<State>>,
    last: Option<Want>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PanelLink {
    pub fn start(plugins: Plugins) -> Self {
        let (jobs, inbox) = channel();
        let state = Arc::new(Mutex::new(State::Off("aus")));
        let look = Arc::new(Mutex::new(None));
        let (s, l) = (state.clone(), look.clone());
        let thread = std::thread::spawn(move || Worker::new(s, l, plugins).run(inbox));
        Self { look, jobs, state, last: None, thread: Some(thread) }
    }

    pub fn want(&mut self, want: Option<Want>) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Some(want));
        }
    }

    /// Nur wenn die Anzeige uns gehört. `height` ist die Bildschirmhöhe in Pixeln.
    pub fn look(&self, height: f32) -> Option<Look> {
        let (ui, width, free) = (*self.look.lock().unwrap_or_else(|e| e.into_inner()))?;
        Some(Look { unit: height / ui, width: width * height / ui, free })
    }

    pub fn state(&self) -> State {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for PanelLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(None);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

const FIRST: Duration = Duration::from_millis(300);
const SLOWEST: Duration = Duration::from_secs(15);
/// So oft darf die Suche leer ausgehen, bevor im Menü der Grund steht. Gesucht wird weiter, nur seltener.
const PATIENT: u32 = 8;

struct Game {
    reader: Reader,
    image: std::ops::Range<u64>,
    layout: Option<Layout>,
}

struct Worker {
    /// Höhe der Oberfläche und Breite der Anzeige in ihren Einheiten.
    look: Arc<Mutex<Option<(f32, f32, bool)>>>,
    state: Arc<Mutex<State>>,
    plugins: Plugins,
    want: Option<Want>,
    game: Option<(u32, Game)>,
    panel: Option<Panel>,
    /// Zuletzt angenommener Auftrag.
    sent: Option<Order>,
    problem: Option<&'static str>,
    tries: u32,
    next: Instant,
    wait: Duration,
}

impl Worker {
    fn new(state: Arc<Mutex<State>>, look: Arc<Mutex<Option<(f32, f32, bool)>>>, plugins: Plugins) -> Self {
        let next = Instant::now();
        Self {
            look,
            state,
            plugins,
            want: None,
            game: None,
            panel: None,
            sent: None,
            problem: None,
            tries: 0,
            next,
            wait: FIRST,
        }
    }

    fn run(mut self, inbox: Receiver<Option<Option<Want>>>) {
        loop {
            match inbox.recv_timeout(Duration::from_millis(250)) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Some(mut w)) => {
                    // Beim Ziehen kommen viele Wünsche; nur der letzte zählt.
                    let mut stop = false;
                    for more in inbox.try_iter() {
                        match more {
                            Some(m) => w = m,
                            None => stop = true,
                        }
                    }
                    if stop {
                        break;
                    }
                    self.change(w);
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.step();
            let state = match (self.want, self.sent, self.problem) {
                (None, ..) => State::Off("aus"),
                (_, Some(_), _) => State::Ready,
                (_, _, Some(why)) if self.tries >= PATIENT => State::Off(why),
                _ => State::Searching,
            };
            let look = self.panel.filter(|_| state == State::Ready).map(|p| (p.screen.1, p.width, self.free()));
            *self.look.lock().unwrap_or_else(|e| e.into_inner()) = look;
            *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
        }
        self.release();
    }

    fn change(&mut self, w: Option<Want>) {
        if w.map(|w| w.pid) != self.want.map(|w| w.pid) {
            self.release();
            self.game = None;
            self.again();
        }
        // Zurück im Spiel: vielleicht eine andere Welt.
        if w.is_some_and(|w| w.playing) && !self.want.is_some_and(|w| w.playing) {
            self.next = Instant::now();
            self.wait = FIRST;
        }
        self.want = w;
    }

    fn again(&mut self) {
        self.next = Instant::now();
        self.wait = FIRST;
        self.tries = 0;
        self.problem = None;
    }

    /// Alles zurück an den Platz des Spiels.
    fn release(&mut self) {
        if let (Some(o), Some((pid, _))) = (self.sent.take(), &self.game) {
            let _ = self.order(*pid, &Order { wish: Wish::Release, ..o });
        }
        self.panel = None;
    }

    fn order(&self, pid: u32, o: &Order) -> Option<u32> {
        let dll = self.plugins.path(PLUGIN)?;
        mett_inject::call_export_text(pid, &dll, s!("mett_panel_order"), &o.encode()).ok()
    }

    fn step(&mut self) {
        let Some(w) = self.want else { return };
        if let Some((_, g)) = &self.game
            && let (Some(p), Some(l)) = (self.panel, g.layout)
        {
            match p.now(&g.reader, &l) {
                Some(p) => self.panel = Some(p),
                None => {
                    // Welt verlassen oder Oberfläche neu aufgebaut.
                    self.panel = None;
                    self.sent = None;
                    self.again();
                }
            }
        }
        if self.panel.is_none() && w.playing && Instant::now() >= self.next {
            if let Err(why) = self.find(w.pid) {
                self.problem = Some(why);
                self.tries += 1;
            }
            self.next = Instant::now() + self.wait;
            self.wait = (self.wait * 3 / 2).min(SLOWEST);
        }
        self.apply(w);
    }

    fn find(&mut self, pid: u32) -> Result<(), &'static str> {
        if self.game.is_none() {
            let image = gamemem::image_range(pid).map_err(|_| "Spiel nicht lesbar")?;
            let reader = Reader::open_for_scan(pid).map_err(|_| "Spiel nicht lesbar")?;
            self.game = Some((pid, Game { reader, image, layout: None }));
        }
        let Some((_, g)) = &mut self.game else { return Ok(()) };
        if g.layout.is_none() {
            g.layout = uitree::layout(&g.reader, &g.image);
        }
        let l = g.layout.ok_or("Aufbau der Oberfläche nicht eindeutig erkannt")?;
        let p = uitree::panel(&g.reader, &l).ok_or("Minecrafts Chat-Anzeige nicht eindeutig gefunden")?;
        self.plugins.load(pid, PLUGIN)?;
        self.panel = Some(p);
        Ok(())
    }

    fn free(&self) -> bool {
        self.game.as_ref().and_then(|g| g.1.layout).is_some_and(|l| l.parts.is_some() && l.sizes.is_some())
    }

    fn apply(&mut self, w: Want) {
        let Some((pid, g)) = &self.game else { return };
        let (Some(p), Some(l)) = (self.panel, g.layout) else { return };
        let Some(wish) = wish(w.goal, p.screen, self.free()) else { return };
        let o = Order {
            client: std::process::id(),
            at: p.at,
            parent: p.parent,
            table: l.table,
            offsets: Offsets {
                name: l.name,
                place: l.place,
                size: l.size,
                flags: l.flags,
                bit: l.bit as u64,
                parent: l.parent,
                children: l.children,
                half: l.half,
                alpha: l.alpha.unwrap_or(0),
                parts: l.parts.map_or(0, |p| p.list),
                owner: l.parts.map_or(0, |p| p.owner),
                delta: l.parts.map_or(0, |p| p.delta),
                layout: l.sizes.map_or(0, |s| s.table),
                width: l.sizes.map_or(0, |s| s.width),
                value: l.sizes.map_or(0, |s| s.value),
            },
            wish,
        };
        if self.sent == Some(o) {
            return;
        }
        match self.order(*pid, &o) {
            Some(answer::OK) => {
                self.sent = Some(o);
                self.problem = None;
                self.tries = 0;
            }
            a => {
                self.sent = None;
                self.panel = None;
                self.problem = Some(if a == Some(answer::STALE) {
                    "Chat-Anzeige hat gewechselt"
                } else {
                    "Plugin antwortet nicht"
                });
                self.tries += 1;
                self.next = Instant::now() + self.wait;
            }
        }
    }
}

/// Pixel in Einheiten der Oberfläche.
fn wish(goal: Goal, screen: (f32, f32), free: bool) -> Option<Wish> {
    match goal {
        Goal::Hide => Some(Wish::Hide),
        Goal::Place { height, .. } if height < 1.0 => None,
        Goal::Place { x, bottom, height, rows, sudden, size, java } => {
            let f = screen.1 / height;
            let size = if free { (size.0 * f, size.1 * f) } else { (0.0, 0.0) };
            Some(Wish::Place { x: x * f, bottom: bottom * f, rows, sudden, size, java })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_werden_umgerechnet() {
        let g = Goal::Place {
            x: 144.0,
            bottom: 1320.0,
            height: 1440.0,
            rows: 7,
            sudden: true,
            size: (900.0, 300.0),
            java: true,
        };
        assert_eq!(
            wish(g, (854.0, 480.0), true),
            Some(Wish::Place { x: 48.0, bottom: 440.0, rows: 7, sudden: true, size: (300.0, 100.0), java: true })
        );
        let none =
            Goal::Place { x: 1.0, bottom: 1.0, height: 0.0, rows: 7, sudden: true, size: (1.0, 1.0), java: false };
        assert_eq!(wish(none, (854.0, 480.0), true), None);
        assert_eq!(wish(Goal::Hide, (0.0, 0.0), false), Some(Wish::Hide));
    }

    #[test]
    fn ohne_wunsch_passiert_nichts() {
        let mut l = PanelLink::start(Plugins::new());
        assert_eq!(l.state(), State::Off("aus"));
        l.want(Some(Want { pid: u32::MAX, playing: true, goal: Goal::Hide }));
        l.want(None);
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(l.state(), State::Off("aus"));
    }
}
