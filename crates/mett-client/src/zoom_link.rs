//! Zoom: sucht das Sichtfeld unter den benannten Einstellungen des laufenden Spiels und gibt es samt Zoom-Einstellungen
//! ans Plugin. Läuft in einem eigenen Thread.
use crate::plugins::Plugins;
use crate::{focus, named};
use gamemem::Reader;
use gamemem::options::{self, Options};
use hud::zoom::Settings;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::s;

pub const PLUGIN: &str = "zoom";
pub const FOV: &str = "gfx_field_of_view";
const RETRY: Duration = Duration::from_secs(15);
const PATIENT: u32 = 4;
const RUNNING: &str = "läuft";

type Want = Option<(u32, Settings)>;

pub struct ZoomLink {
    jobs: Sender<Option<Want>>,
    state: Arc<Mutex<&'static str>>,
    last: Want,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ZoomLink {
    pub fn start(plugins: Plugins) -> Self {
        let (jobs, inbox) = channel();
        let state = Arc::new(Mutex::new("aus"));
        let s = state.clone();
        let thread = std::thread::spawn(move || Worker::new(s, plugins).run(inbox));
        Self { jobs, state, last: None, thread: Some(thread) }
    }

    pub fn want(&mut self, want: Want) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Some(want));
        }
    }

    pub fn state(&self) -> &'static str {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn on(&self) -> bool {
        self.state() == RUNNING
    }
}

impl Drop for ZoomLink {
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
    want: Want,
    /// Je Spielstart einmal gefunden: Einstellungen und das Sichtfeld.
    found: Option<(u32, Options, (u64, u64))>,
    /// Zuletzt angenommen.
    sent: Want,
    /// Sichtfeld des Nutzers: zuletzt gelesen, und zuletzt gemerkt.
    seen: Option<f32>,
    noted: Option<f32>,
    tries: u32,
    /// Woran es zuletzt lag.
    why: &'static str,
    next: Instant,
}

impl Worker {
    fn new(state: Arc<Mutex<&'static str>>, plugins: Plugins) -> Self {
        Self {
            state,
            plugins,
            want: None,
            found: None,
            sent: None,
            seen: None,
            noted: None,
            tries: 0,
            why: "",
            next: Instant::now(),
        }
    }

    fn run(mut self, inbox: Receiver<Option<Want>>) {
        loop {
            match inbox.recv_timeout(Duration::from_millis(500)) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Some(mut w)) => {
                    // Beim Ziehen an Reglern zählt nur der letzte Wunsch.
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
                    if w.map(|w| w.0) != self.want.map(|w| w.0) {
                        self.tries = 0;
                        self.next = Instant::now();
                    }
                    self.want = w;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.track();
            let state = self.step();
            *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
        }
        self.off();
    }

    fn step(&mut self) -> &'static str {
        if self.sent.map(|s| s.0) != self.want.map(|w| w.0) {
            self.off();
        }
        let Some((pid, settings)) = self.want else { return "aus" };
        if self.sent != self.want && (self.sent.is_some() || Instant::now() >= self.next) {
            self.next = Instant::now() + RETRY;
            match self.turn_on(pid, &settings) {
                Ok(()) => {
                    self.sent = self.want;
                    self.tries = 0;
                }
                Err(why) => {
                    self.sent = None;
                    self.tries += 1;
                    self.why = why;
                }
            }
        }
        match (self.sent, self.tries >= PATIENT) {
            (Some(_), _) => RUNNING,
            (None, true) => self.why,
            (None, false) => "wird gesucht",
        }
    }

    /// Das Sichtfeld des Nutzers kann sich im Spiel ändern. Solange die Untergrenze gesenkt ist, gehört der Wert uns.
    fn track(&mut self) {
        let (Some(_), Some((pid, o, place))) = (&self.sent, &self.found) else {
            self.seen = None;
            return;
        };
        let read = Reader::open(*pid).ok().and_then(|r| options::least_and_value(&r, o, *place, FOV));
        let now = read.filter(|(least, v)| *least >= 1.0 && v.is_finite()).map(|r| r.1);
        // Zweimal gleich, sonst könnte gerade ein Wechsel dazwischenliegen.
        if now.is_some() && now == self.seen && now != self.noted {
            focus::track_zoom(now.unwrap_or_default());
            self.noted = now;
        }
        self.seen = now;
    }

    fn turn_on(&mut self, pid: u32, s: &Settings) -> Result<(), &'static str> {
        let (o, fov) = named::place(pid, FOV)?;
        self.found = Some((pid, o, fov));
        // Erst merken: Minecraft könnte mitten im Zoom speichern.
        focus::remember_zoom().map_err(|_| "Merkzettel ließ sich nicht schreiben")?;
        self.plugins.load(pid, PLUGIN)?;
        match self.order(pid, true, s) {
            Some(0) => Ok(()),
            _ => {
                self.found = None;
                named::forget();
                Err("Plugin nimmt das Sichtfeld nicht an")
            }
        }
    }

    fn order(&self, pid: u32, on: bool, s: &Settings) -> Option<u32> {
        let (_, o, (at, info)) = self.found.as_ref()?;
        let client = std::process::id();
        let text = format!(
            "{client:x} {:x} {:x} {:x} {:x} {at:x} {info:x}|{}",
            on as u8,
            o.table,
            o.name,
            o.value,
            s.encode()
        );
        let dll = self.plugins.path(PLUGIN)?;
        mett_inject::call_export_text(pid, &dll, s!("mett_zoom_order"), &text).ok()
    }

    fn off(&mut self) {
        if let Some((pid, s)) = self.sent.take() {
            // Antwortet das Spiel nicht mehr, bleibt der Zettel zum Zurückstellen.
            if self.order(pid, false, &s) == Some(0) {
                focus::untrack_zoom();
            }
            self.noted = None;
        }
    }
}
