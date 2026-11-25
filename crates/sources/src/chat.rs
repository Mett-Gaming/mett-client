//! Chat mitlesen: nur lesend, nichts wird gespeichert.
use gamemem::{Reader, chat};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// So viele Nachrichten hält auch die Java Edition vor.
const KEEP: usize = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Mit Minecrafts Farbzeichen.
    pub text: String,
    /// `None`: stand schon da, als wir die Liste fanden.
    pub at: Option<Instant>,
}

pub type Lines = Arc<Vec<Line>>;

/// Wie viele Nachrichten am Ende von `new` neu sind. Vorne darf das Spiel alte entfernt haben.
fn appended(old: &[chat::Message], new: &[chat::Message]) -> usize {
    (0..=old.len()).find(|k| new.starts_with(&old[*k..])).map_or(new.len(), |k| new.len() - (old.len() - k))
}

pub struct Chat {
    reader: Reader,
    pid: u32,
    list: Option<usize>,
    old: Vec<chat::Message>,
    lines: Lines,
    errors: u32,
    pace: Pace,
    /// Eine frühere Suche fand nichts: Was jetzt da steht, ist neu.
    was_empty: bool,
}

/// Wann gesucht wird: anfangs oft, dann seltener, nach dem Senden sofort.
struct Pace {
    next: Instant,
    wait: Duration,
}

impl Pace {
    const FIRST: Duration = Duration::from_secs(2);
    const MAX: Duration = Duration::from_secs(15);

    fn new(now: Instant) -> Self {
        Self { next: now, wait: Self::FIRST }
    }

    fn due(&self, now: Instant) -> bool {
        now >= self.next
    }

    /// Nach einer Suche. Danach seltener, damit das Spiel nichts davon merkt.
    fn searched(&mut self, now: Instant) {
        self.next = now + self.wait;
        self.wait = (self.wait * 2).min(Self::MAX);
    }

    /// Gleich kommt eine Nachricht: kurz warten, dann suchen und wieder öfter.
    fn hurry(&mut self, now: Instant) {
        self.next = self.next.min(now + Duration::from_millis(300));
        self.wait = Duration::from_secs(1);
    }
}

impl Chat {
    pub fn open(pid: u32) -> Result<Self, &'static str> {
        let reader = Reader::open_for_scan(pid).map_err(|_| "Chat nicht lesbar")?;
        Ok(Self {
            reader,
            pid,
            list: None,
            old: vec![],
            lines: Lines::default(),
            errors: 0,
            pace: Pace::new(Instant::now()),
            was_empty: false,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Es wurde gerade etwas gesendet.
    pub fn expect(&mut self) {
        if self.list.is_none() {
            self.pace.hurry(Instant::now());
        }
    }

    /// `Ok(None)`: Liste noch nicht gefunden (z. B. keine Welt offen oder noch keine Nachricht).
    pub fn poll(&mut self) -> Result<Option<Lines>, &'static str> {
        let Some(list) = self.list else { return self.find() };
        let read = |at: usize, out: &mut [u8]| self.reader.read(at, out).is_ok();
        let Some(new) = chat::messages(list, &read) else {
            // Während das Spiel gerade schreibt, darf ein Versuch scheitern.
            self.errors += 1;
            if self.errors > 20 {
                self.list = None;
                self.was_empty = false;
                self.pace = Pace::new(Instant::now() + Duration::from_secs(1));
            }
            return Ok(Some(self.lines.clone()));
        };
        self.errors = 0;
        if new != self.old {
            let n = appended(&self.old, &new);
            self.add(&new[new.len() - n..], Some(Instant::now()));
            self.old = new;
        }
        Ok(Some(self.lines.clone()))
    }

    fn find(&mut self) -> Result<Option<Lines>, &'static str> {
        if !self.pace.due(Instant::now()) {
            return Ok(None);
        }
        let read = |at: usize, out: &mut [u8]| self.reader.read(at, out).is_ok();
        let reader = &self.reader;
        let found = reader.scan(&|at, buf| chat::lists_in(at, buf, &|a, out: &mut [u8]| reader.read(a, out).is_ok()));
        self.pace.searched(Instant::now());
        match found[..] {
            [] => {
                self.was_empty = true;
                Ok(None)
            }
            [list] => {
                self.old = chat::messages(list, &read).unwrap_or_default();
                self.list = Some(list);
                self.errors = 0;
                self.lines = Lines::default();
                let old = self.old.clone();
                let new = self.was_empty.then(Instant::now);
                self.add(&old, new);
                Ok(Some(self.lines.clone()))
            }
            // Lieber nichts zeigen als das Falsche.
            _ => Err("Chat nicht eindeutig"),
        }
    }

    fn add(&mut self, new: &[chat::Message], at: Option<Instant>) {
        let lines = Arc::make_mut(&mut self.lines);
        lines.extend(new.iter().map(|m| Line { text: m.shown().into(), at }));
        let extra = lines.len().saturating_sub(KEEP);
        lines.drain(..extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(t: &str) -> chat::Message {
        chat::Message { kind: 1, author: "A".into(), text: t.into(), full: String::new() }
    }

    #[test]
    fn suche_wird_seltener_und_nach_dem_senden_sofort() {
        let t = Instant::now();
        let s = Duration::from_secs;
        let mut p = Pace::new(t);
        assert!(p.due(t));
        let mut at = t;
        for wait in [2, 4, 8, 15, 15] {
            p.searched(at);
            assert!(!p.due(at + s(wait) - Duration::from_millis(1)) && p.due(at + s(wait)), "{wait}");
            at += s(wait);
        }
        p.searched(at);
        p.hurry(at);
        assert!(!p.due(at) && p.due(at + Duration::from_millis(300)));
        // Noch nichts da: gleich noch einmal.
        p.searched(at + s(1));
        assert!(p.due(at + s(2)));
        // Eine nähere Suche wird nicht verschoben.
        let mut p = Pace::new(t);
        p.hurry(t);
        assert!(p.due(t));
    }

    #[test]
    fn neue_nachrichten_werden_erkannt() {
        let v = |t: &str| t.chars().map(|c| m(&c.to_string())).collect::<Vec<_>>();
        assert_eq!(appended(&v("ab"), &v("abc")), 1);
        assert_eq!(appended(&v("abc"), &v("bcd")), 1);
        assert_eq!(appended(&v("abc"), &v("abc")), 0);
        assert_eq!(appended(&v("abc"), &v("")), 0);
        assert_eq!(appended(&v("abc"), &v("xy")), 2);
        assert_eq!(appended(&v("aa"), &v("aaa")), 1);
    }
}
