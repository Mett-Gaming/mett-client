//! Blendet Minecrafts Fadenkreuz aus, solange das eigene gezeigt wird. Läuft in einem eigenen Thread.
use crate::plugins::Plugins;
use gamemem::Reader;
use gamemem::uitree::{self, Crosshair, Layout, Visible};
use mett_plugin::cross::Order;
use mett_plugin::panel::{PLUGIN, answer};
use std::ops::Range;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::s;

const LOOK: Duration = Duration::from_millis(250);
const FIRST: Duration = Duration::from_millis(500);
const SLOWEST: Duration = Duration::from_secs(60);
/// Nach einem Menü nicht gleich wieder alles durchsuchen.
const REST: Duration = Duration::from_secs(10);

pub struct CrossLink {
    jobs: Sender<Option<Option<(u32, bool)>>>,
    state: Arc<Mutex<&'static str>>,
    last: Option<(u32, bool)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CrossLink {
    pub fn start(plugins: Plugins) -> Self {
        let (jobs, inbox) = channel();
        let state = Arc::new(Mutex::new("aus"));
        let s = state.clone();
        let thread = std::thread::spawn(move || Worker::new(s, plugins).run(inbox));
        Self { jobs, state, last: None, thread: Some(thread) }
    }

    /// Spiel, in dem das Fadenkreuz weg soll, und ob gerade gespielt wird.
    pub fn want(&mut self, want: Option<(u32, bool)>) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Some(want));
        }
    }

    pub fn state(&self) -> &'static str {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for CrossLink {
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
    image: Range<u64>,
    layout: Option<Layout>,
    visible: Option<Visible>,
}

struct Worker {
    state: Arc<Mutex<&'static str>>,
    plugins: Plugins,
    want: Option<(u32, bool)>,
    game: Option<Game>,
    /// Gerade weggerückt.
    hidden: Option<Crosshair>,
    searched: Option<Instant>,
    next: Instant,
    wait: Duration,
    problem: &'static str,
}

impl Worker {
    fn new(state: Arc<Mutex<&'static str>>, plugins: Plugins) -> Self {
        let next = Instant::now();
        let problem = "wird gesucht";
        Self { state, plugins, want: None, game: None, hidden: None, searched: None, next, wait: FIRST, problem }
    }

    fn run(mut self, inbox: Receiver<Option<Option<(u32, bool)>>>) {
        loop {
            match inbox.recv_timeout(LOOK) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Some(w)) => self.change(w),
                Err(RecvTimeoutError::Timeout) => {}
            }
            let state = self.step();
            *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
        }
        self.release();
    }

    fn change(&mut self, w: Option<(u32, bool)>) {
        if w.map(|w| w.0) != self.want.map(|w| w.0) {
            self.release();
            if w.is_some_and(|w| self.game.as_ref().is_some_and(|g| g.pid != w.0)) {
                self.game = None;
            }
            (self.next, self.wait, self.problem) = (Instant::now(), FIRST, "wird gesucht");
        } else if w.is_some_and(|w| w.1) && !self.want.is_some_and(|w| w.1) {
            self.wait = FIRST;
            self.next = self.searched.map_or(Instant::now(), |t| t + REST);
        }
        self.want = w;
    }

    fn release(&mut self) {
        if let Some(c) = self.hidden.take() {
            let _ = self.order(&c, false);
        }
    }

    fn order(&self, c: &Crosshair, hide: bool) -> Option<u32> {
        let g = self.game.as_ref()?;
        let (l, v) = (g.layout?, g.visible?);
        let o = Order {
            client: std::process::id(),
            hide,
            at: c.at,
            parent: c.parent,
            table: l.table,
            name: l.name,
            up: l.parent,
            flags: l.flags,
            own: v.own,
            all: v.all,
        };
        let dll = self.plugins.path(PLUGIN)?;
        mett_inject::call_export_text(g.pid, &dll, s!("mett_cursor_order"), &o.encode()).ok()
    }

    fn step(&mut self) -> &'static str {
        let Some((pid, playing)) = self.want else { return "aus" };
        if let (Some(c), Some(g)) = (&self.hidden, &self.game)
            && !g.layout.is_some_and(|l| c.alive(&g.reader, &l))
        {
            // Welt verlassen oder Oberfläche neu aufgebaut.
            self.hidden = None;
            (self.next, self.wait) = (Instant::now(), FIRST);
        }
        if self.hidden.is_none() && playing && Instant::now() >= self.next {
            if let Err(why) = self.find(pid) {
                self.problem = why;
            }
            self.searched = Some(Instant::now());
            self.next = Instant::now() + self.wait;
            self.wait = (self.wait * 3 / 2).min(SLOWEST);
        }
        if self.hidden.is_some() { "ausgeblendet" } else { self.problem }
    }

    fn find(&mut self, pid: u32) -> Result<(), &'static str> {
        if self.game.is_none() {
            let image = gamemem::image_range(pid).map_err(|_| "Spiel nicht lesbar")?;
            let reader = Reader::open_for_scan(pid).map_err(|_| "Spiel nicht lesbar")?;
            self.game = Some(Game { pid, reader, image, layout: None, visible: None });
        }
        let Some(g) = &mut self.game else { return Ok(()) };
        if g.layout.is_none() {
            g.layout = uitree::layout(&g.reader, &g.image);
        }
        let l = g.layout.ok_or("Aufbau der Oberfläche nicht eindeutig erkannt")?;
        let c = uitree::crosshair(&g.reader, &l).ok_or("nicht eindeutig gefunden")?;
        if g.visible.is_none() {
            g.visible = uitree::visible(&g.reader, &l);
        }
        g.visible.ok_or("Merker „sichtbar“ nicht eindeutig erkannt")?;
        self.plugins.load(pid, PLUGIN)?;
        match self.order(&c, true) {
            Some(answer::OK) => {
                self.hidden = Some(c);
                Ok(())
            }
            _ => Err("Plugin nimmt das Fadenkreuz nicht an"),
        }
    }
}
