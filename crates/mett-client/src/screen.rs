//! Bildschirm und Eingabe im Spiel: lädt das HUD-Plugin, schickt Zeichenbefehle und holt Eingaben ab.
use crate::plugins::Plugins;
use hud::symbols::Symbols;
use hud::{Draw, wire};
use hudlink::{Client, Event};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Eingaben aus dem Spiel liegen bereit.
pub const WM_EVENTS: u32 = WM_APP + 3;
const PLUGIN: &str = "hud";

struct Shared {
    link: RwLock<Option<Client>>,
    note: Mutex<&'static str>,
    stop: AtomicBool,
}

pub struct Screen {
    plugins: Plugins,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    pid: Option<u32>,
    loading: Option<JoinHandle<()>>,
    next: Instant,
    tries: u32,
    last: Vec<u8>,
    symbols: Arc<Symbols>,
    seen: Option<(u32, Instant)>,
}

impl Screen {
    pub fn start(hwnd: HWND, plugins: Plugins) -> Self {
        let shared = Arc::new(Shared {
            link: RwLock::new(None),
            note: Mutex::new("wartet auf Minecraft"),
            stop: AtomicBool::new(false),
        });
        let (s, target) = (shared.clone(), hwnd.0 as isize);
        // Weckt den Fenster-Thread, sobald das Spiel Eingaben meldet.
        let worker = std::thread::spawn(move || {
            while !s.stop.load(SeqCst) {
                let woken = s.link.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(|l| l.wait(100));
                match woken {
                    Some(true) => unsafe {
                        let _ = PostMessageW(Some(HWND(target as *mut _)), WM_EVENTS, WPARAM(0), LPARAM(0));
                    },
                    Some(false) => {}
                    None => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        });
        Self {
            plugins,
            shared,
            worker: Some(worker),
            pid: None,
            loading: None,
            next: Instant::now(),
            tries: 0,
            last: vec![],
            symbols: Arc::default(),
            seen: None,
        }
    }

    fn link<R>(&self, f: impl FnOnce(&Client) -> R) -> Option<R> {
        self.shared.link.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(f)
    }

    pub fn note(&self) -> &'static str {
        *self.shared.note.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Einmal je Takt. `ready`: Das Spielfenster ist da und vorne.
    pub fn tick(&mut self, pid: Option<u32>, ready: bool, control: u32) {
        if pid != self.pid {
            self.pid = pid;
            self.tries = 0;
            self.next = Instant::now() + Duration::from_secs(3);
            *self.shared.link.write().unwrap_or_else(|e| e.into_inner()) = None;
        }
        if self.loading.as_ref().is_some_and(|l| l.is_finished()) {
            self.loading = None;
        }
        match self.link(|l| l.connected().then(|| l.beat(control))) {
            Some(Some(())) => return,
            Some(None) => *self.shared.link.write().unwrap_or_else(|e| e.into_inner()) = None,
            None => {}
        }
        let Some(pid) = pid.filter(|_| ready && self.loading.is_none() && self.tries < 3) else { return };
        if Instant::now() < self.next {
            return;
        }
        self.tries += 1;
        self.next = Instant::now() + Duration::from_secs(10);
        self.last.clear();
        self.symbols = Arc::default();
        let (plugins, shared) = (self.plugins.clone(), self.shared.clone());
        // Laden kann dauern; das Fenster bleibt dabei bedienbar.
        self.loading = Some(std::thread::spawn(move || {
            let r =
                plugins.load(pid, PLUGIN).and_then(|_| Client::open(pid).map_err(|_| "keine Verbindung zum Plugin"));
            let note = match r {
                Ok(l) => {
                    *shared.link.write().unwrap_or_else(|e| e.into_inner()) = Some(l);
                    "läuft"
                }
                Err(why) => why,
            };
            *shared.note.lock().unwrap_or_else(|e| e.into_inner()) = note;
        }));
    }

    /// Bildgröße im Spiel, sobald gezeichnet wird.
    pub fn size(&self) -> Option<(f32, f32)> {
        self.link(|l| l.screen()).flatten().map(|s| (s.0 as f32, s.1 as f32))
    }

    pub fn set_zoom_keys(&self, keys: [u8; 4]) {
        self.link(|l| l.set_zoom_keys(keys));
    }

    pub fn set_keys(&self, chat: &[u16], command: &[u16]) {
        self.link(|l| l.set_keys(chat, command));
    }

    /// Schickt nur, was sich geändert hat.
    pub fn show(&mut self, scene: &[Draw]) {
        let bytes = wire::encode(scene);
        if bytes != self.last && self.link(|l| l.send(&bytes)) == Some(true) {
            self.last = bytes;
        }
    }

    /// Symboltafeln des Servers ins Plugin legen, nur bei Änderungen.
    pub fn show_symbols(&mut self, new: &Arc<Symbols>) {
        if *new == self.symbols {
            return;
        }
        let sent = self.link(|l| {
            let mut sheets = new.0.values();
            (0..Client::SHEETS).all(|slot| match sheets.next() {
                Some(s) => l.send_sheet(slot, s.id, s.side, &s.pixels),
                None => l.send_sheet(slot, 0, 0, &[]),
            })
        });
        if sent == Some(true) {
            self.symbols = new.clone();
        }
    }

    pub fn events(&self) -> Vec<Event> {
        self.link(|l| l.events()).unwrap_or_default()
    }

    /// Bilder je Sekunde, aus den gezählten Bildern. Einmal je Sekunde neu.
    pub fn frames(&mut self) -> Option<u32> {
        let count = self.link(|l| l.counts().0)?;
        let now = Instant::now();
        match self.seen {
            Some((before, at)) if now - at >= Duration::from_secs(1) => {
                self.seen = Some((count, now));
                Some((count.wrapping_sub(before) as f32 / (now - at).as_secs_f32()).round() as u32)
            }
            Some(_) => None,
            None => {
                self.seen = Some((count, now));
                None
            }
        }
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        self.shared.stop.store(true, SeqCst);
        for t in [self.worker.take(), self.loading.take()].into_iter().flatten() {
            let _ = t.join();
        }
        *self.shared.link.write().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
