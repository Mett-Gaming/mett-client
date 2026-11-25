//! Führt alle Quellen zusammen. Jede läuft in einem eigenen Thread und kann einzeln ausfallen.
use crate::chat::{Chat, Lines};
use crate::commands::Source;
use crate::{etw::FpsSession, frames::Fps, media, ping, symbols, system};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Reading<T> {
    /// Noch kein Wert.
    #[default]
    Pending,
    Ready(T),
    /// Geht hier nicht, mit kurzem Grund für die Anzeige.
    Off(&'static str),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Snapshot {
    pub system: Reading<system::System>,
    pub battery: Reading<u8>,
    pub music: Reading<String>,
    pub ping: Reading<u32>,
    pub fps: Reading<Fps>,
    pub chat: Reading<Lines>,
    pub commands: Option<Arc<hud::hints::Commands>>,
    pub symbols: Arc<hud::symbols::Symbols>,
}

/// Was gerade gebraucht wird. Nicht angezeigte Quellen arbeiten nicht.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wanted {
    pub system: bool,
    pub battery: bool,
    pub music: bool,
    pub ping: Option<String>,
    pub fps_pid: Option<u32>,
    pub chat_pid: Option<u32>,
    pub commands_pid: Option<u32>,
}

struct Shared {
    snapshot: Mutex<Snapshot>,
    system: AtomicBool,
    battery: AtomicBool,
    music: AtomicBool,
    ping: Mutex<Option<String>>,
    fps_pid: AtomicU32,
    chat_pid: AtomicU32,
    chat_expected: AtomicBool,
    commands_pid: AtomicU32,
    chat_opened: AtomicBool,
    symbols_opened: AtomicBool,
    stop: AtomicBool,
}

pub struct Hub {
    shared: Arc<Shared>,
    fps: Option<thread::JoinHandle<()>>,
}

/// Ein Absturz einer Quelle darf die anderen nicht blockieren.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Hub {
    pub fn start() -> Self {
        let shared = Arc::new(Shared {
            snapshot: Mutex::default(),
            system: AtomicBool::new(false),
            battery: AtomicBool::new(false),
            music: AtomicBool::new(false),
            ping: Mutex::new(None),
            fps_pid: AtomicU32::new(0),
            chat_pid: AtomicU32::new(0),
            chat_expected: AtomicBool::new(false),
            commands_pid: AtomicU32::new(0),
            chat_opened: AtomicBool::new(false),
            symbols_opened: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        });
        spawn(&shared, system_loop);
        spawn(&shared, music_loop);
        spawn(&shared, ping_loop);
        spawn(&shared, chat_loop);
        spawn(&shared, commands_loop);
        spawn(&shared, symbols_loop);
        let fps = spawn(&shared, fps_loop);
        Self { shared, fps }
    }

    pub fn want(&self, w: &Wanted) {
        self.shared.system.store(w.system, Relaxed);
        self.shared.battery.store(w.battery, Relaxed);
        self.shared.music.store(w.music, Relaxed);
        self.shared.fps_pid.store(w.fps_pid.unwrap_or(0), Relaxed);
        self.shared.chat_pid.store(w.chat_pid.unwrap_or(0), Relaxed);
        self.shared.commands_pid.store(w.commands_pid.unwrap_or(0), Relaxed);
        let mut ping = lock(&self.shared.ping);
        if *ping != w.ping {
            ping.clone_from(&w.ping);
            lock(&self.shared.snapshot).ping = Reading::Pending;
        }
    }

    /// Gleich erscheint eine Nachricht im Chat.
    pub fn expect_chat(&self) {
        self.shared.chat_expected.store(true, Relaxed);
    }

    /// Der Chat ging auf: Befehlsliste auffrischen.
    pub fn chat_opened(&self) {
        self.shared.chat_opened.store(true, Relaxed);
        self.shared.symbols_opened.store(true, Relaxed);
    }

    pub fn snapshot(&self) -> Snapshot {
        lock(&self.shared.snapshot).clone()
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.shared.stop.store(true, Relaxed);
        // Nur die FPS-Sitzung muss in Windows abgemeldet werden; die anderen enden mit dem Programm.
        if let Some(t) = self.fps.take() {
            let _ = t.join();
        }
    }
}

fn spawn(shared: &Arc<Shared>, f: fn(&Shared)) -> Option<thread::JoinHandle<()>> {
    let s = shared.clone();
    thread::Builder::new().spawn(move || f(&s)).ok()
}

fn pause(s: &Shared, ms: u64) -> bool {
    thread::sleep(Duration::from_millis(ms));
    !s.stop.load(Relaxed)
}

fn system_loop(s: &Shared) {
    let mut sampler = None;
    loop {
        if s.system.load(Relaxed) {
            let value = sampler.get_or_insert_with(system::Sampler::new).sample();
            lock(&s.snapshot).system = value.map_or(Reading::Pending, Reading::Ready);
        } else {
            sampler = None;
        }
        if s.battery.load(Relaxed) {
            lock(&s.snapshot).battery = system::battery().map_or(Reading::Off("Kein Akku"), Reading::Ready);
        }
        if !pause(s, 1000) {
            return;
        }
    }
}

fn music_loop(s: &Shared) {
    media::init();
    loop {
        if s.music.load(Relaxed) {
            let value = match media::current() {
                Ok(Some(text)) => Reading::Ready(text),
                Ok(None) => Reading::Off("Keine Musik"),
                Err(_) => Reading::Off("Musik nicht lesbar"),
            };
            lock(&s.snapshot).music = value;
        }
        if !pause(s, 2000) {
            return;
        }
    }
}

fn ping_loop(s: &Shared) {
    loop {
        let host = lock(&s.ping).clone();
        if let Some(host) = host {
            let value = match ping::measure(&host) {
                Some(ms) => Reading::Ready(ms),
                None if host.trim().is_empty() => Reading::Off("Kein Server"),
                None => Reading::Off("Keine Antwort"),
            };
            // Nur übernehmen, wenn inzwischen nicht ein anderer Server eingetragen wurde.
            if lock(&s.ping).as_deref() == Some(host.as_str()) {
                lock(&s.snapshot).ping = value;
            }
        }
        if !pause(s, 2000) {
            return;
        }
    }
}

fn chat_loop(s: &Shared) {
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST};
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
    }
    let mut chat: Option<Chat> = None;
    loop {
        let pid = s.chat_pid.load(Relaxed);
        if chat.as_ref().is_some_and(|c| c.pid() != pid) {
            chat = None;
            lock(&s.snapshot).chat = Reading::Pending;
        }
        if pid != 0 && chat.is_none() {
            match Chat::open(pid) {
                Ok(c) => chat = Some(c),
                Err(e) => lock(&s.snapshot).chat = Reading::Off(e),
            }
        }
        if let Some(c) = &mut chat {
            if s.chat_expected.swap(false, Relaxed) {
                c.expect();
            }
            let value = match c.poll() {
                Ok(Some(lines)) => Reading::Ready(lines),
                Ok(None) => Reading::Pending,
                Err(e) => Reading::Off(e),
            };
            // Nach einem Fehler bis zur nächsten Suche stehen lassen.
            let mut snap = lock(&s.snapshot);
            if !(value == Reading::Pending && matches!(snap.chat, Reading::Off(_))) {
                snap.chat = value;
            }
        }
        if !pause(s, if chat.is_some() { 100 } else { 1000 }) {
            return;
        }
    }
}

fn commands_loop(s: &Shared) {
    let mut source: Option<Source> = None;
    loop {
        let pid = s.commands_pid.load(Relaxed);
        if source.as_ref().is_some_and(|c| c.pid() != pid) {
            source = None;
            lock(&s.snapshot).commands = None;
        }
        if pid != 0 && source.is_none() {
            source = Source::open(pid);
        }
        if let Some(c) = &mut source {
            let value = c.poll(s.chat_opened.swap(false, Relaxed));
            lock(&s.snapshot).commands = value;
        }
        if !pause(s, if source.is_some() { 100 } else { 1000 }) {
            return;
        }
    }
}

/// Schrifttafeln gehören zum Chat. Ohne Befehlsliste ist keine Welt offen.
fn symbols_loop(s: &Shared) {
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST};
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
    }
    let mut source: Option<symbols::Source> = None;
    let mut away = 0u32;
    loop {
        let pid = s.commands_pid.load(Relaxed);
        if source.as_ref().is_some_and(|c| c.pid() != pid) {
            source = None;
            lock(&s.snapshot).symbols = Arc::default();
        }
        if pid != 0 && source.is_none() {
            source = symbols::Source::open(pid);
        }
        if let Some(c) = &mut source {
            // Beim Wechsel zwischen Unterservern fehlt die Liste kurz.
            away = if lock(&s.snapshot).commands.is_some() { 0 } else { away.saturating_add(1) };
            let value = c.poll(away < 50, s.symbols_opened.swap(false, Relaxed));
            lock(&s.snapshot).symbols = value;
        }
        if !pause(s, if source.is_some() { 100 } else { 1000 }) {
            return;
        }
    }
}

fn fps_loop(s: &Shared) {
    let mut session: Option<FpsSession> = None;
    let mut denied = false;
    loop {
        let pid = s.fps_pid.load(Relaxed);
        if session.as_ref().is_some_and(|x| x.pid() != pid) {
            session = None;
        }
        if pid == 0 {
            denied = false;
        } else if session.is_none() && !denied {
            match FpsSession::start(pid) {
                Ok(x) => session = Some(x),
                Err(e) => {
                    denied = true;
                    lock(&s.snapshot).fps = Reading::Off(e);
                }
            }
        }
        if let Some(x) = &session {
            lock(&s.snapshot).fps = x.fps().map_or(Reading::Pending, Reading::Ready);
        }
        if !pause(s, 250) {
            return;
        }
    }
}
