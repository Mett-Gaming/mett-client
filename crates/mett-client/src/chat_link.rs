//! Verbindung zum Chat-Plugin: sucht die Sendestelle über Namen, lädt das Plugin und reicht abgeschickte Nachrichten
//! weiter. Läuft in einem eigenen Thread, damit die Anzeige nie wartet.
use crate::plugins::Plugins;
use gamemem::Reader;
use gamemem::model::{self, Found};
use mett_plugin::chat::{Action, Order, PLUGIN, answer};
use std::collections::{HashSet, VecDeque};
use std::ops::Range;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::s;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Off(&'static str),
    /// Spiel noch nicht ausgewertet.
    Searching,
    /// Senden ist möglich, die Stelle in der Welt wird noch gesucht.
    Prepared,
    Ready,
}

impl State {
    /// T gehört uns. Nachrichten warten notfalls kurz.
    pub fn usable(self) -> bool {
        matches!(self, State::Prepared | State::Ready)
    }

    pub fn text(self) -> &'static str {
        match self {
            State::Off(why) => why,
            State::Searching => "Minecraft wird ausgewertet",
            State::Prepared => "bereit",
            State::Ready => "läuft",
        }
    }
}

enum Job {
    /// Prozess und ob gerade gespielt wird; `None` schaltet ab.
    Want(Option<(u32, bool)>),
    Send(String),
    Stop,
}

struct Shared {
    state: State,
    /// Können auch Befehle gesendet werden?
    commands: bool,
    /// Meldungen für den Nutzer, wenn etwas nicht ankam.
    notes: Vec<&'static str>,
}

pub struct ChatLink {
    jobs: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
    last: Option<(u32, bool)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ChatLink {
    pub fn start(plugins: Plugins) -> Self {
        let (jobs, inbox) = channel();
        let shared = Arc::new(Mutex::new(Shared { state: State::Off("aus"), commands: false, notes: vec![] }));
        let s = shared.clone();
        let thread = std::thread::spawn(move || Worker::new(s, plugins).run(inbox));
        Self { jobs, shared, last: None, thread: Some(thread) }
    }

    pub fn want(&mut self, want: Option<(u32, bool)>) {
        if want != self.last {
            self.last = want;
            let _ = self.jobs.send(Job::Want(want));
        }
    }

    pub fn send(&self, text: String) {
        let _ = self.jobs.send(Job::Send(text));
    }

    pub fn state(&self) -> State {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).state
    }

    pub fn commands(&self) -> bool {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).commands
    }

    pub fn notes(&self) -> Vec<&'static str> {
        std::mem::take(&mut self.shared.lock().unwrap_or_else(|e| e.into_inner()).notes)
    }
}

impl Drop for ChatLink {
    fn drop(&mut self) {
        let _ = self.jobs.send(Job::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Je Spielstart einmal aus dem laufenden Abbild gelesen.
struct Game {
    reader: Reader,
    range: Range<u64>,
    tables: HashSet<u64>,
    chat: u64,
    command: Option<u64>,
    offset: usize,
}

/// So lange darf eine abgeschickte Nachricht auf die Suche warten; danach wird sie nie nachgeholt.
const PATIENCE: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_millis(700);

struct Worker {
    shared: Arc<Mutex<Shared>>,
    plugins: Plugins,
    pid: Option<u32>,
    playing: bool,
    game: Option<Game>,
    /// Für diesen Spielstart endgültig aus.
    failed: Option<&'static str>,
    target: Option<Found>,
    /// Abgeschickt, aber die Stelle ist noch nicht gefunden.
    waiting: VecDeque<(String, Instant)>,
    /// Fehlversuche beim Auswerten.
    tries: u32,
    analyse_at: Instant,
    next: Instant,
    wait: Duration,
}

impl Worker {
    fn new(shared: Arc<Mutex<Shared>>, plugins: Plugins) -> Self {
        Self {
            shared,
            plugins,
            pid: None,
            playing: false,
            game: None,
            failed: None,
            target: None,
            waiting: VecDeque::new(),
            tries: 0,
            analyse_at: Instant::now(),
            next: Instant::now(),
            wait: Duration::from_secs(3),
        }
    }

    fn run(mut self, inbox: Receiver<Job>) {
        loop {
            // Wartet etwas oder fehlt die Stelle noch, öfter nachsehen.
            let busy = !self.waiting.is_empty() || (self.playing && self.target.is_none());
            match inbox.recv_timeout(Duration::from_millis(if busy { 100 } else { 300 })) {
                Ok(Job::Stop) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(Job::Want(w)) => self.want(w),
                Ok(Job::Send(t)) => self.send(t),
                Err(_) => {}
            }
            self.step();
            self.flush();
            self.publish();
        }
        self.target = None;
    }

    fn want(&mut self, w: Option<(u32, bool)>) {
        let pid = w.map(|w| w.0);
        if pid != self.pid {
            self.target = None;
            self.pid = pid;
            self.game = None;
            self.failed = None;
            self.waiting.clear();
            self.tries = 0;
            // Beim Start des Spiels ist sein Abbild noch nicht vollständig.
            self.analyse_at = Instant::now() + Duration::from_secs(4);
        }
        let playing = w.is_some_and(|w| w.1);
        if playing && !self.playing {
            self.wait = RETRY;
            self.next = Instant::now();
        }
        self.playing = playing;
    }

    fn step(&mut self) {
        let Some(pid) = self.pid.filter(|_| self.failed.is_none()) else { return };
        if self.game.is_none() {
            self.prepare(pid);
        }
        let Some(g) = &self.game else { return };
        if let Some(t) = &self.target {
            if t.alive(&g.reader, &g.range) {
                return;
            }
            // Andere Welt: neu suchen.
            self.target = None;
            self.wait = RETRY;
            self.next = Instant::now();
        }
        // Wartende Nachrichten suchen auch dann weiter, wenn gerade ein Fenster offen ist.
        if !(self.playing || !self.waiting.is_empty()) || Instant::now() < self.next {
            return;
        }
        if let Err(why) = self.find(pid) {
            self.failed = Some(why);
            self.target = None;
        }
        self.next = Instant::now() + self.wait;
        // Anfangs dicht hintereinander, dann seltener.
        self.wait = (self.wait * 5 / 4).min(Duration::from_secs(10));
    }

    /// Schon im Hauptmenü: alles, was nicht von einer Welt abhängt. So ist der Chat von Anfang an da.
    fn prepare(&mut self, pid: u32) {
        if Instant::now() < self.analyse_at {
            return;
        }
        match analyse(pid).and_then(|g| self.plugins.load(pid, PLUGIN).map(|_| g)) {
            Ok(g) => self.game = Some(g),
            Err(why) => {
                self.tries += 1;
                self.analyse_at = Instant::now() + Duration::from_secs(5);
                // Früh kann es am Laden liegen; danach passt die Spielversion nicht.
                if self.tries >= 6 {
                    self.failed = Some(why);
                }
            }
        }
    }

    /// `Err`: geht mit diesem Spielstart nicht.
    fn find(&mut self, pid: u32) -> Result<(), &'static str> {
        let Some(g) = &self.game else { return Ok(()) };
        let Some(found) = model::find(&g.reader, &g.tables, g.offset, &g.range) else { return Ok(()) };
        self.plugins.load(pid, PLUGIN)?;
        // Erst wenn das Plugin im Spiel dieselben Prüfungen besteht, wird gesendet.
        if self.order(found, Action::Check) == Some(answer::OK) {
            self.target = Some(found);
        }
        Ok(())
    }

    /// Wartende Nachrichten der Reihe nach senden oder nach der Frist aufgeben.
    fn flush(&mut self) {
        while let Some((_, at)) = self.waiting.front() {
            if at.elapsed() > PATIENCE || self.failed.is_some() {
                self.waiting.pop_front();
                self.note("Nicht gesendet: Minecraft war nicht rechtzeitig bereit");
            } else if self.target.is_some() {
                let Some((text, at)) = self.waiting.pop_front() else { return };
                self.deliver(text, at);
            } else {
                return;
            }
        }
    }

    fn order(&self, t: Found, action: Action) -> Option<u32> {
        let (g, pid) = (self.game.as_ref()?, self.pid?);
        let o = Order {
            function: if matches!(action, Action::Command(_)) { g.command? } else { g.chat },
            controller: t.controller,
            table: t.table,
            offset: t.offset,
            model: t.model,
            action,
        };
        let dll = self.plugins.path(PLUGIN)?;
        mett_inject::call_export_text(pid, &dll, s!("mett_chat_order"), &o.encode()).ok()
    }

    fn send(&mut self, text: String) {
        if text.starts_with('/') && self.game.as_ref().is_some_and(|g| g.command.is_none()) {
            return self.note("Nicht gesendet: Befehle gehen mit dieser Spielversion nicht");
        }
        // Über die Warteschlange, damit die Reihenfolge stimmt.
        self.waiting.push_back((text, Instant::now()));
        self.next = self.next.min(Instant::now());
    }

    fn deliver(&mut self, text: String, at: Instant) {
        let Some(t) = self.target else { return };
        let action = if text.starts_with('/') { Action::Command(text.clone()) } else { Action::Chat(text.clone()) };
        match self.order(t, action) {
            Some(answer::OK) => {}
            Some(answer::RESTRICTED) => self.note("Nicht gesendet: Minecraft meldet eine Einschränkung"),
            // Sicher nicht gesendet: neu suchen und innerhalb der Frist noch einmal.
            Some(answer::STALE) => {
                self.target = None;
                self.next = Instant::now();
                self.wait = RETRY;
                self.waiting.push_front((text, at));
            }
            Some(answer::NOT_REACHED) => self.note("Nicht gesendet: Minecraft reagiert gerade nicht"),
            Some(answer::BAD_ORDER) => self.note("Nicht gesendet: Text nicht erlaubt"),
            _ => self.note("Unklar, ob die Nachricht ankam"),
        }
    }

    fn note(&self, t: &'static str) {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).notes.push(t);
    }

    fn publish(&self) {
        let state = match (self.pid, self.failed, self.target) {
            (None, ..) => State::Off("aus"),
            (_, Some(why), _) => State::Off(why),
            (_, _, Some(_)) => State::Ready,
            _ if self.game.is_some() => State::Prepared,
            _ => State::Searching,
        };
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.state = state;
        s.commands = state.usable() && self.game.as_ref().is_some_and(|g| g.command.is_some());
    }
}

fn analyse(pid: u32) -> Result<Game, &'static str> {
    let (base, bytes) = gamemem::image(pid).map_err(|_| "Spiel nicht lesbar")?;
    let image = compat::image::Image::new(base, &bytes).ok_or("Spiel nicht lesbar")?;
    let send = compat::chat_send::find(&image).map_err(|m| m.text())?;
    let tables = compat::hud_model::tables(&image).ok_or("HUD-Controller nicht eindeutig gefunden")?;
    let reader = Reader::open_for_scan(pid).map_err(|_| "Spiel nicht lesbar")?;
    Ok(Game {
        reader,
        range: base..base + bytes.len() as u64,
        tables: tables.into_iter().map(|t| base + t as u64).collect(),
        chat: base + send.chat as u64,
        command: send.command.map(|c| base + c as u64),
        offset: send.model_offset,
    })
}
