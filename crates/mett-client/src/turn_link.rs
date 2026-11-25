//! Blickrichtung: liest die eigene Richtung aus dem laufenden Spiel. Liest nur. Läuft in einem eigenen Thread.
use gamemem::Reader;
use gamemem::rotation::{self, Own, Types};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Beim Spielen etwa je Bild.
const FRESH: Duration = Duration::from_millis(15);
const IDLE: Duration = Duration::from_millis(250);
const FIRST: Duration = Duration::from_secs(1);
const SLOWEST: Duration = Duration::from_secs(30);

type Want = Option<(u32, bool)>;

pub struct TurnLink {
    jobs: Sender<Option<Want>>,
    shared: Arc<Mutex<(&'static str, Option<f32>)>>,
    last: Want,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TurnLink {
    pub fn start() -> Self {
        let (jobs, inbox) = channel();
        let shared = Arc::new(Mutex::new(("aus", None)));
        let s = shared.clone();
        let thread = std::thread::spawn(move || Worker::new(s).run(inbox));
        Self { jobs, shared, last: None, thread: Some(thread) }
    }

    /// Prozess und ob gerade gespielt wird.
    pub fn want(&mut self, want: Want) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Some(want));
        }
    }

    pub fn state(&self) -> &'static str {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).0
    }

    /// Grad wie auf einem Kompass: 0 ist Norden, 90 Osten.
    pub fn degrees(&self) -> Option<f32> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).1
    }
}

impl Drop for TurnLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(None);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Worker {
    shared: Arc<Mutex<(&'static str, Option<f32>)>>,
    want: Want,
    reader: Option<Reader>,
    types: Option<Types>,
    own: Option<Own>,
    next: Instant,
    wait: Duration,
}

impl Worker {
    fn new(shared: Arc<Mutex<(&'static str, Option<f32>)>>) -> Self {
        Self { shared, want: None, reader: None, types: None, own: None, next: Instant::now(), wait: FIRST }
    }

    fn run(mut self, inbox: Receiver<Option<Want>>) {
        loop {
            let busy = self.own.is_some() && self.want.is_some_and(|w| w.1);
            match inbox.recv_timeout(if busy { FRESH } else { IDLE }) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Some(w)) => {
                    if w.map(|w| w.0) != self.want.map(|w| w.0) {
                        (self.reader, self.types, self.own) = (None, None, None);
                        (self.next, self.wait) = (Instant::now(), FIRST);
                    }
                    self.want = w;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = self.step();
            *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = now;
        }
    }

    fn step(&mut self) -> (&'static str, Option<f32>) {
        let Some((pid, playing)) = self.want else { return ("aus", None) };
        if self.reader.is_none() {
            self.reader = Reader::open_for_scan(pid).ok();
        }
        let Some(r) = &self.reader else { return ("Spiel nicht lesbar", None) };
        if let Some(o) = &self.own {
            match o.read(r) {
                Some((_, yaw)) => return ("läuft", Some(compass(yaw))),
                // Welt gewechselt oder verlassen.
                None => (self.own, self.next, self.wait) = (None, Instant::now() + FIRST, FIRST),
            }
        }
        if playing && Instant::now() >= self.next {
            if self.types.is_none()
                && let Ok((base, image)) = gamemem::image(pid)
            {
                self.types = rotation::types(base, &image);
                if self.types.is_none() {
                    // Die Namen ändern sich im laufenden Spiel nicht mehr.
                    (self.next, self.wait) = (Instant::now() + SLOWEST, SLOWEST);
                    return ("passt nicht zu dieser Spielversion", None);
                }
            }
            self.own = self.types.as_ref().and_then(|t| rotation::find(r, t));
            self.next = Instant::now() + self.wait;
            self.wait = (self.wait * 2).min(SLOWEST);
        }
        if self.types.is_none() && self.wait == SLOWEST {
            return ("passt nicht zu dieser Spielversion", None);
        }
        ("wird in der Welt gesucht", None)
    }
}

/// Minecraft zählt von Süden aus über Westen.
fn compass(yaw: f32) -> f32 {
    (yaw + 180.0).rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn himmelsrichtungen() {
        for (yaw, want) in [(180.0, 0.0), (-180.0, 0.0), (-90.0, 90.0), (0.0, 180.0), (90.0, 270.0)] {
            assert_eq!(compass(yaw), want);
        }
    }
}
