//! Fullbright: sucht die benannten Einstellungen und lässt das Plugin sie umstellen. Läuft in einem
//! eigenen Thread.
use crate::plugins::Plugins;
use crate::{focus, named, zoom_link};
use mett_plugin::panel::{PLUGIN, answer};
use mett_plugin::values::{ALLOWED, Order};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::s;

const COUNT: usize = ALLOWED.len();
const RETRY: Duration = Duration::from_secs(5);
const PATIENT: u32 = 4;
const RUNNING: &str = "läuft";

/// Prozess und je Einstellung der gewünschte Wert.
pub type Want = (u32, [Option<f32>; COUNT]);

pub struct ValueLink {
    jobs: Sender<Option<Option<Want>>>,
    state: Arc<Mutex<[&'static str; COUNT]>>,
    last: Option<Want>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ValueLink {
    pub fn start(plugins: Plugins) -> Self {
        let (jobs, inbox) = channel();
        let state = Arc::new(Mutex::new(["aus"; COUNT]));
        let s = state.clone();
        let thread = std::thread::spawn(move || Worker::new(s, plugins).run(inbox));
        Self { jobs, state, last: None, thread: Some(thread) }
    }

    pub fn want(&mut self, want: Option<Want>) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Some(want));
        }
    }

    pub fn state(&self, which: usize) -> &'static str {
        self.state.lock().unwrap_or_else(|e| e.into_inner())[which]
    }
}

impl Drop for ValueLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(None);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Worker {
    state: Arc<Mutex<[&'static str; COUNT]>>,
    plugins: Plugins,
    want: Option<Want>,
    /// Spiel, in dem alles bereitliegt.
    ready: Option<u32>,
    /// Was im Spiel gerade gilt.
    sent: [Option<f32>; COUNT],
    tries: [u32; COUNT],
    /// Woran es zuletzt lag.
    why: [&'static str; COUNT],
    next: Instant,
}

impl Worker {
    fn new(state: Arc<Mutex<[&'static str; COUNT]>>, plugins: Plugins) -> Self {
        Self {
            state,
            plugins,
            want: None,
            ready: None,
            sent: [None; COUNT],
            tries: [0; COUNT],
            why: [""; COUNT],
            next: Instant::now(),
        }
    }

    fn run(mut self, inbox: Receiver<Option<Option<Want>>>) {
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
                        self.off();
                        self.tries = [0; COUNT];
                    }
                    self.want = w;
                    self.next = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let state = self.step();
            *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
        }
        self.off();
    }

    fn step(&mut self) -> [&'static str; COUNT] {
        let Some((pid, wish)) = self.want else {
            self.off();
            return ["aus"; COUNT];
        };
        let due = Instant::now() >= self.next;
        let mut failed = false;
        // Schon vor dem Einschalten alles bereitlegen, dann geht es sofort.
        if due && self.ready != Some(pid) {
            match self.prepare(pid) {
                Ok(()) => self.ready = Some(pid),
                Err(_) => failed = true,
            }
        }
        for (which, target) in wish.into_iter().enumerate() {
            if self.sent[which] == target || !due {
                continue;
            }
            match self.turn(pid, which, target) {
                Ok(()) => (self.sent[which], self.tries[which]) = (target, 0),
                Err(why) => {
                    failed = true;
                    self.why[which] = why;
                    self.tries[which] += 1;
                    self.sent[which] = None;
                }
            }
        }
        if failed {
            self.next = Instant::now() + RETRY;
        }
        std::array::from_fn(|i| match (wish[i], self.sent[i], self.tries[i] >= PATIENT) {
            (None, ..) => "aus",
            (Some(_), Some(_), _) => RUNNING,
            (Some(_), None, true) => self.why[i],
            (Some(_), None, false) => "wird gesucht",
        })
    }

    fn prepare(&mut self, pid: u32) -> Result<(), &'static str> {
        named::place(pid, ALLOWED[0].0)?;
        named::place(pid, zoom_link::FOV)?;
        self.plugins.load(pid, PLUGIN)?;
        self.plugins.load(pid, zoom_link::PLUGIN)
    }

    fn turn(&mut self, pid: u32, which: usize, target: Option<f32>) -> Result<(), &'static str> {
        if let Some(t) = target {
            // Erst merken: Minecraft speichert seine Einstellungen auch zwischendurch.
            focus::remember_value(ALLOWED[which].0, t).map_err(|_| "Merkzettel ließ sich nicht schreiben")?;
        }
        named::place(pid, ALLOWED[which].0)?;
        self.plugins.load(pid, PLUGIN)?;
        match self.order(pid, which, target) {
            Some(answer::OK) => Ok(()),
            _ => {
                named::forget();
                Err("Plugin nimmt die Einstellungen nicht an")
            }
        }
    }

    fn order(&self, pid: u32, which: usize, target: Option<f32>) -> Option<u32> {
        let (options, (at, info)) = named::place(pid, ALLOWED[which].0).ok()?;
        let o = Order {
            client: std::process::id(),
            which,
            target,
            table: options.table,
            name: options.name,
            value: options.value,
            at,
            info,
        };
        let dll = self.plugins.path(PLUGIN)?;
        mett_inject::call_export_text(pid, &dll, s!("mett_value_order"), &o.encode()).ok()
    }

    fn off(&mut self) {
        for which in 0..COUNT {
            if self.sent[which].take().is_some()
                && let Some((pid, _)) = self.want
            {
                let _ = self.order(pid, which, None);
            }
        }
    }
}
