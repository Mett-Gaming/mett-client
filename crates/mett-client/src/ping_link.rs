//! Findet die Adresse des Servers, mit dem das Spiel gerade verbunden ist. Liest nur. Läuft in einem eigenen Thread.
use gamemem::{Reader, server};
use std::net::Ipv4Addr;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const LOOK: Duration = Duration::from_secs(1);
const FIRST: Duration = Duration::from_secs(1);
const SLOWEST: Duration = Duration::from_secs(60);
/// Nach einem Menü nicht gleich wieder alles durchsuchen.
const REST: Duration = Duration::from_secs(10);

pub struct PingLink {
    jobs: Sender<Option<Option<(u32, bool)>>>,
    found: Arc<Mutex<Option<Ipv4Addr>>>,
    last: Option<(u32, bool)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PingLink {
    pub fn start() -> Self {
        let (jobs, inbox) = channel();
        let found = Arc::new(Mutex::new(None));
        let f = found.clone();
        let thread = std::thread::spawn(move || Worker::new(f).run(inbox));
        Self { jobs, found, last: None, thread: Some(thread) }
    }

    /// Prozess und ob gerade gespielt wird.
    pub fn want(&mut self, want: Option<(u32, bool)>) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Some(want));
        }
    }

    pub fn server(&self) -> Option<Ipv4Addr> {
        *self.found.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for PingLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(None);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Game {
    reader: Reader,
    /// Funktionstabellen der Verbindung; leer: in dieser Spielversion nicht gefunden.
    tables: Vec<u64>,
}

struct Worker {
    found: Arc<Mutex<Option<Ipv4Addr>>>,
    want: Option<(u32, bool)>,
    game: Option<Game>,
    objects: Vec<u64>,
    searched: Option<Instant>,
    next: Instant,
    wait: Duration,
}

impl Worker {
    fn new(found: Arc<Mutex<Option<Ipv4Addr>>>) -> Self {
        let next = Instant::now();
        Self { found, want: None, game: None, objects: vec![], searched: None, next, wait: FIRST }
    }

    fn run(mut self, inbox: Receiver<Option<Option<(u32, bool)>>>) {
        loop {
            match inbox.recv_timeout(LOOK) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Some(w)) => self.change(w),
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = self.step();
            *self.found.lock().unwrap_or_else(|e| e.into_inner()) = now;
        }
    }

    fn change(&mut self, w: Option<(u32, bool)>) {
        if w.map(|w| w.0) != self.want.map(|w| w.0) {
            self.game = None;
            self.objects.clear();
            (self.next, self.wait, self.searched) = (Instant::now(), FIRST, None);
        } else if w.is_some_and(|w| w.1) && !self.want.is_some_and(|w| w.1) {
            self.wait = FIRST;
            self.next = self.searched.map_or(Instant::now(), |t| t + REST);
        }
        self.want = w;
    }

    fn step(&mut self) -> Option<Ipv4Addr> {
        let (pid, playing) = self.want?;
        if self.game.is_none() && playing {
            self.game = prepare(pid);
        }
        let g = self.game.as_ref()?;
        let now = server::address(&g.reader, &g.tables, &self.objects);
        if now.is_some() || !playing || g.tables.is_empty() || Instant::now() < self.next {
            return now;
        }
        self.objects = server::find(&g.reader, &g.tables);
        self.searched = Some(Instant::now());
        self.next = Instant::now() + self.wait;
        self.wait = (self.wait * 3 / 2).min(SLOWEST);
        server::address(&g.reader, &g.tables, &self.objects)
    }
}

fn prepare(pid: u32) -> Option<Game> {
    let reader = Reader::open_for_scan(pid).ok()?;
    let (base, bytes) = gamemem::image(pid).ok()?;
    let image = compat::image::Image::new(base, &bytes)?;
    let tables = image.vtables(server::CLASS).into_iter().map(|t| base + t as u64).collect();
    Some(Game { reader, tables })
}
