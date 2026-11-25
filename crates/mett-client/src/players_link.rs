//! Spieler fürs Tab-Menü: sucht beim Spielen Minecrafts Spielerliste. Gelesen werden Namen und Gesichter nur,
//! solange die Taste gehalten wird; danach sind sie wieder weg.
use gamemem::{Reader, players};
use hud::tab_list::Player;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Want {
    pub pid: u32,
    pub playing: bool,
    pub held: bool,
}

const FRESH: Duration = Duration::from_millis(200);
const FIRST: Duration = Duration::from_secs(1);
const SLOWEST: Duration = Duration::from_secs(60);
/// Nach einem Menü nicht gleich wieder alles durchsuchen.
const REST: Duration = Duration::from_secs(15);
const PATIENT: u32 = 5;

struct Shared {
    state: &'static str,
    names: Vec<Player>,
}

pub struct PlayersLink {
    jobs: Sender<Option<Option<Want>>>,
    shared: Arc<Mutex<Shared>>,
    last: Option<Want>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PlayersLink {
    pub fn start() -> Self {
        let (jobs, inbox) = channel();
        let shared = Arc::new(Mutex::new(Shared { state: "aus", names: vec![] }));
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

    pub fn state(&self) -> &'static str {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).state
    }

    pub fn names(&self) -> Vec<Player> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).names.clone()
    }
}

impl Drop for PlayersLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(None);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Worker {
    shared: Arc<Mutex<Shared>>,
    want: Option<Want>,
    game: Option<(u32, Reader)>,
    /// Anfänge der Spielerlisten.
    heads: Vec<u64>,
    tries: u32,
    next: Instant,
    wait: Duration,
    /// Letzte Suche.
    searched: Option<Instant>,
}

impl Worker {
    fn new(shared: Arc<Mutex<Shared>>) -> Self {
        Self {
            shared,
            want: None,
            game: None,
            heads: vec![],
            tries: 0,
            next: Instant::now(),
            wait: FIRST,
            searched: None,
        }
    }

    fn run(mut self, inbox: Receiver<Option<Option<Want>>>) {
        loop {
            match inbox.recv_timeout(FRESH) {
                Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Some(w)) => self.change(w),
                Err(RecvTimeoutError::Timeout) => {}
            }
            let (state, names) = self.step();
            *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = Shared { state, names };
        }
    }

    fn change(&mut self, w: Option<Want>) {
        if w.map(|w| w.pid) != self.want.map(|w| w.pid) {
            self.game = None;
            self.heads.clear();
            self.searched = None;
            self.again();
        }
        // Taste gedrückt: gleich suchen.
        if w.is_some_and(|w| w.held) && !self.want.is_some_and(|w| w.held) {
            self.next = Instant::now();
        }
        // Zurück im Spiel: vielleicht eine andere Welt.
        if w.is_some_and(|w| w.playing) && !self.want.is_some_and(|w| w.playing) {
            self.again();
            // Wer die Taste hält, wartet nicht.
            if let Some(t) = self.searched.filter(|_| !w.is_some_and(|w| w.held)) {
                self.next = t + REST;
            }
        }
        self.want = w;
    }

    fn again(&mut self) {
        self.next = Instant::now();
        self.wait = FIRST;
        self.tries = 0;
    }

    fn step(&mut self) -> (&'static str, Vec<Player>) {
        let Some(w) = self.want else { return ("aus", vec![]) };
        if self.game.is_none() {
            self.game = Reader::open_for_scan(w.pid).ok().map(|r| (w.pid, r));
        }
        let Some((_, r)) = &self.game else { return ("Spiel nicht lesbar", vec![]) };
        let mut names = None;
        if w.held && !self.heads.is_empty() {
            names = players::players(r, &self.heads);
            if names.is_none() {
                // Andere Welt: neu suchen.
                self.heads.clear();
                self.again();
            }
        }
        if self.heads.is_empty() && (w.playing || w.held) && Instant::now() >= self.next {
            if let Some((_, r)) = &self.game {
                self.heads = players::heads(r);
                if w.held {
                    names = players::players(r, &self.heads);
                }
            }
            self.tries += 1;
            self.searched = Some(Instant::now());
            self.next = Instant::now() + self.wait;
            self.wait = (self.wait * 3 / 2).min(SLOWEST);
        }
        let state = match (self.heads.is_empty(), self.tries >= PATIENT) {
            (false, _) => "bereit",
            (true, true) => "Spielerliste nicht gefunden",
            (true, false) => "wird in der Welt gesucht",
        };
        (state, names.unwrap_or_default().into_iter().map(|p| Player { name: p.name, face: p.face }).collect())
    }
}
