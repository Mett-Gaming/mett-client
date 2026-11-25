//! Chat wie in der Java Edition: sucht zwei benannte Einstellungen des laufenden Spiels und lässt das Plugin sie
//! umstellen. Läuft in einem eigenen Thread.
use crate::focus;
use crate::plugins::Plugins;
use gamemem::Reader;
use gamemem::options::{self, Options};
use mett_plugin::panel::{PLUGIN, answer};
use mett_plugin::style::Order;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::s;

const RETRY: Duration = Duration::from_secs(15);
const PATIENT: u32 = 4;

pub struct StyleLink {
    jobs: Sender<Option<Option<u32>>>,
    state: Arc<Mutex<&'static str>>,
    last: Option<u32>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl StyleLink {
    pub fn start(plugins: Plugins) -> Self {
        let (jobs, inbox) = channel();
        let state = Arc::new(Mutex::new("aus"));
        let s = state.clone();
        let thread = std::thread::spawn(move || Worker::new(s, plugins).run(inbox));
        Self { jobs, state, last: None, thread: Some(thread) }
    }

    /// Spiel, in dem der Stil gelten soll.
    pub fn want(&mut self, pid: Option<u32>) {
        if pid != self.last {
            self.last = pid;
            let _ = self.jobs.send(Some(pid));
        }
    }

    pub fn state(&self) -> &'static str {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn on(&self) -> bool {
        self.state() == RUNNING
    }
}

const RUNNING: &str = "läuft";

impl Drop for StyleLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(None);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Worker {
    state: Arc<Mutex<&'static str>>,
    plugins: Plugins,
    want: Option<u32>,
    /// Je Spielstart einmal gefunden.
    found: Option<(u32, Options)>,
    /// Spiel, in dem der Stil gerade gilt.
    on: Option<u32>,
    tries: u32,
    next: Instant,
}

impl Worker {
    fn new(state: Arc<Mutex<&'static str>>, plugins: Plugins) -> Self {
        Self { state, plugins, want: None, found: None, on: None, tries: 0, next: Instant::now() }
    }

    fn run(mut self, inbox: Receiver<Option<Option<u32>>>) {
        loop {
            match inbox.recv_timeout(Duration::from_millis(500)) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Some(w)) => {
                    self.want = w;
                    self.tries = 0;
                    self.next = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let state = self.step();
            *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
        }
        self.off();
    }

    fn step(&mut self) -> &'static str {
        if self.on.is_some() && self.on != self.want {
            self.off();
        }
        let Some(pid) = self.want else { return "aus" };
        if self.on.is_none() && Instant::now() >= self.next {
            self.next = Instant::now() + RETRY;
            match self.turn_on(pid) {
                Ok(()) => self.on = Some(pid),
                Err(why) => {
                    self.tries += 1;
                    if self.tries >= PATIENT {
                        return why;
                    }
                }
            }
        }
        match (self.on, self.tries >= PATIENT) {
            (Some(_), _) => RUNNING,
            (None, true) => "Chat-Einstellungen nicht eindeutig gefunden",
            (None, false) => "wird gesucht",
        }
    }

    fn turn_on(&mut self, pid: u32) -> Result<(), &'static str> {
        if self.found.as_ref().is_none_or(|f| f.0 != pid) {
            let file = focus::options_file().and_then(|p| std::fs::read_to_string(p).ok());
            let file = options::parse(&file.ok_or("Einstellungsdatei nicht lesbar")?);
            let image = gamemem::image_range(pid).map_err(|_| "Spiel nicht lesbar")?;
            let reader = Reader::open_for_scan(pid).map_err(|_| "Spiel nicht lesbar")?;
            let o = options::find(&reader, &image, &file).ok_or("Chat-Einstellungen nicht eindeutig gefunden")?;
            self.found = Some((pid, o));
        }
        // Erst merken: Minecraft speichert seine Einstellungen auch zwischendurch.
        focus::remember_style().map_err(|_| "Merkzettel ließ sich nicht schreiben")?;
        self.plugins.load(pid, PLUGIN)?;
        match self.order(pid, true) {
            Some(answer::OK) => Ok(()),
            _ => {
                // Beim nächsten Mal neu suchen.
                self.found = None;
                Err("Plugin nimmt die Einstellungen nicht an")
            }
        }
    }

    fn order(&self, pid: u32, on: bool) -> Option<u32> {
        let (_, f) = self.found.as_ref()?;
        let o = Order {
            client: std::process::id(),
            on,
            table: f.table,
            name: f.name,
            value: f.value,
            options: [f.spacing, f.opacity],
        };
        let dll = self.plugins.path(PLUGIN)?;
        mett_inject::call_export_text(pid, &dll, s!("mett_style_order"), &o.encode()).ok()
    }

    fn off(&mut self) {
        if let Some(pid) = self.on.take() {
            let _ = self.order(pid, false);
        }
    }
}
