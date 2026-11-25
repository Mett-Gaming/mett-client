//! Datenquellen. Eingaben werden nur gezählt, nie gespeichert oder protokolliert.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

mod chat;
mod commands;
mod etw;
mod frames;
mod hub;
mod media;
mod ping;
pub mod presence;
mod symbols;
mod system;

pub use chat::{Line, Lines};
pub use frames::Fps;
pub use hub::{Hub, Reading, Snapshot, Wanted};
pub use system::System;

const WINDOW: Duration = Duration::from_secs(1);

/// Klicks der letzten Sekunde.
#[derive(Default)]
pub struct CpsCounter {
    clicks: VecDeque<Instant>,
}

impl CpsCounter {
    pub fn click(&mut self, now: Instant) {
        self.prune(now);
        self.clicks.push_back(now);
    }

    pub fn value(&mut self, now: Instant) -> u32 {
        self.prune(now);
        self.clicks.len() as u32
    }

    pub fn clear(&mut self) {
        self.clicks.clear();
    }

    fn prune(&mut self, now: Instant) {
        while self.clicks.front().is_some_and(|t| now.saturating_duration_since(*t) >= WINDOW) {
            self.clicks.pop_front();
        }
    }
}

/// Stoppuhr: läuft, pausiert, zurücksetzen.
#[derive(Default)]
pub struct Stopwatch {
    done: Duration,
    since: Option<Instant>,
}

impl Stopwatch {
    pub fn toggle(&mut self, now: Instant) {
        match self.since.take() {
            Some(s) => self.done += now.saturating_duration_since(s),
            None => self.since = Some(now),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn value(&self, now: Instant) -> Duration {
        self.done + self.since.map_or(Duration::ZERO, |s| now.saturating_duration_since(s))
    }
}

/// `1:02:03` bzw. `02:03`.
pub fn clock_text(d: Duration) -> String {
    let s = d.as_secs();
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zaehlt_nur_die_letzte_sekunde() {
        let t = Instant::now();
        let mut c = CpsCounter::default();
        for ms in [0, 100, 900] {
            c.click(t + Duration::from_millis(ms));
        }
        assert_eq!(c.value(t + Duration::from_millis(999)), 3);
        assert_eq!(c.value(t + Duration::from_millis(1000)), 2);
        assert_eq!(c.value(t + Duration::from_millis(1900)), 0);
    }

    #[test]
    fn leeren() {
        let t = Instant::now();
        let mut c = CpsCounter::default();
        c.click(t);
        c.clear();
        assert_eq!(c.value(t), 0);
    }

    #[test]
    fn stoppuhr_pausiert_und_laeuft_weiter() {
        let t = Instant::now();
        let at = |s| t + Duration::from_secs(s);
        let mut w = Stopwatch::default();
        w.toggle(at(0));
        w.toggle(at(5));
        assert_eq!(w.value(at(100)), Duration::from_secs(5));
        w.toggle(at(100));
        assert_eq!(w.value(at(103)), Duration::from_secs(8));
        w.reset();
        assert_eq!(w.value(at(200)), Duration::ZERO);
    }

    #[test]
    fn zeittext() {
        assert_eq!(clock_text(Duration::from_secs(59)), "00:59");
        assert_eq!(clock_text(Duration::from_secs(3723)), "1:02:03");
    }
}
