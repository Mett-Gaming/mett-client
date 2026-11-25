//! Eingabezeile des Chats mit den Maßen der Java Edition (26.3), eigene Umsetzung. Kein Windows, voll testbar.
use crate::java_chat::{self, Segment};
use crate::{Draw, Rect, rgb};

pub const MAX: usize = 256;
/// Nur im Arbeitsspeicher, nie auf der Platte.
const HISTORY: usize = 100;
/// Zeilen je Raste des Mausrads.
pub const WHEEL: i32 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    Backspace,
    Delete,
}

#[derive(Debug, Default)]
pub struct ChatInput {
    text: Vec<char>,
    cursor: usize,
    /// Anderes Ende der Auswahl; gleich `cursor`, wenn nichts ausgewählt ist.
    anchor: usize,
    /// Erstes sichtbares Zeichen.
    first: usize,
    history: Vec<String>,
    /// Stelle im Verlauf; `history.len()` ist die eigene, noch nicht gesendete Zeile.
    at: usize,
    draft: Vec<char>,
}

/// Was im Chat erlaubt ist: keine Steuerzeichen und kein Farbzeichen.
fn allowed(c: char) -> bool {
    c != '§' && !c.is_control()
}

impl ChatInput {
    /// Neue, leere Zeile; der Verlauf bleibt.
    pub fn open(&mut self, start: &str) {
        self.set(start.chars().collect());
        self.at = self.history.len();
        self.draft.clear();
    }

    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// Zeichen und Cursor.
    pub fn content(&self) -> (&[char], usize) {
        (&self.text, self.cursor)
    }

    /// Ersetzt einen Bereich und setzt den Cursor dahinter.
    pub fn replace(&mut self, from: usize, to: usize, s: &str) {
        let n = self.text.len();
        (self.anchor, self.cursor) = (from.min(n), to.min(n));
        self.insert(s);
    }

    /// Waagerechte Lage eines Zeichens in Java-Pixeln, nach dem letzten Zeichnen.
    pub fn x(&self, i: usize, advance: &dyn Fn(char) -> f32) -> f32 {
        let i = i.min(self.text.len());
        4.0 + self.text[self.first.min(i)..i].iter().map(|c| advance(*c)).sum::<f32>()
    }

    fn set(&mut self, t: Vec<char>) {
        self.text = t;
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self.first = 0;
    }

    fn selection(&self) -> std::ops::Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    pub fn selected(&self) -> String {
        self.text[self.selection()].iter().collect()
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    pub fn insert(&mut self, s: &str) {
        let r = self.selection();
        let room = MAX - (self.text.len() - r.len());
        let new: Vec<char> = s.chars().filter(|c| allowed(*c)).take(room).collect();
        let end = r.start + new.len();
        self.text.splice(r, new);
        self.cursor = end;
        self.anchor = end;
    }

    /// Ziel einer Bewegung; mit `word` bis zur Wortgrenze.
    fn target(&self, back: bool, word: bool) -> usize {
        let (t, mut p) = (&self.text, self.cursor);
        if !word {
            return if back { p.saturating_sub(1) } else { (p + 1).min(t.len()) };
        }
        if back {
            while p > 0 && t[p - 1] == ' ' {
                p -= 1;
            }
            while p > 0 && t[p - 1] != ' ' {
                p -= 1;
            }
        } else {
            while p < t.len() && t[p] != ' ' {
                p += 1;
            }
            while p < t.len() && t[p] == ' ' {
                p += 1;
            }
        }
        p
    }

    pub fn key(&mut self, k: Key, shift: bool, ctrl: bool) {
        match k {
            Key::Up => self.recall(-1),
            Key::Down => self.recall(1),
            Key::Backspace | Key::Delete => {
                if self.selection().is_empty() {
                    self.anchor = self.target(k == Key::Backspace, ctrl);
                }
                self.insert("");
            }
            Key::Left | Key::Right if !shift && !self.selection().is_empty() => {
                let r = self.selection();
                self.cursor = if k == Key::Left { r.start } else { r.end };
                self.anchor = self.cursor;
            }
            Key::Left | Key::Right | Key::Home | Key::End => {
                self.cursor = match k {
                    Key::Home => 0,
                    Key::End => self.text.len(),
                    _ => self.target(k == Key::Left, ctrl),
                };
                if !shift {
                    self.anchor = self.cursor;
                }
            }
        }
    }

    fn recall(&mut self, step: i32) {
        let to = (self.at as i32 + step).clamp(0, self.history.len() as i32) as usize;
        if to == self.at {
            return;
        }
        if self.at == self.history.len() {
            self.draft = self.text.clone();
        }
        self.at = to;
        let t = self.history.get(to).map_or_else(|| self.draft.clone(), |h| h.chars().collect());
        self.set(t);
    }

    /// Fertige Nachricht: Leerraum gekürzt. `None`, wenn nichts übrig bleibt.
    pub fn submit(&mut self) -> Option<String> {
        let t = self.text().split_whitespace().collect::<Vec<_>>().join(" ");
        self.set(vec![]);
        if t.is_empty() {
            return None;
        }
        if self.history.last() != Some(&t) {
            self.history.push(t.clone());
            let extra = self.history.len().saturating_sub(HISTORY);
            self.history.drain(..extra);
        }
        self.at = self.history.len();
        Some(t)
    }

    /// Zeile am unteren Rand. `unit` ist ein Java-Pixel, `blink` ob der Cursor gerade zu sehen ist.
    pub fn draw(
        &mut self,
        screen: (f32, f32),
        unit: f32,
        blink: bool,
        advance: &dyn Fn(char) -> f32,
        color_of: &dyn Fn(usize) -> Option<u32>,
        out: &mut Vec<Draw>,
    ) {
        let (w, h) = (screen.0 / unit, screen.1 / unit);
        let rect = |x: f32, y: f32, w: f32, h: f32| Rect { x: x * unit, y: y * unit, w: w * unit, h: h * unit };
        let fill = |r, color| Draw::Box { rect: r, radius: 0.0, color };
        out.push(fill(rect(2.0, h - 14.0, w - 4.0, 12.0), rgb(0, 0.5)));
        let room = (w - 4.0 - 8.0).max(20.0);
        let width = |from: usize, to: usize| self.text[from..to].iter().map(|c| advance(*c)).sum::<f32>();
        // So weit schieben, dass der Cursor sichtbar bleibt; Platz für den Unterstrich lassen.
        let mut first = self.first.min(self.cursor);
        while first > 0 && width(first - 1, self.text.len()) + 6.0 <= room {
            first -= 1;
        }
        while width(first, self.cursor) + 6.0 > room && first < self.cursor {
            first += 1;
        }
        let mut last = self.cursor;
        while last < self.text.len() && width(first, last + 1) <= room {
            last += 1;
        }
        let (x, y) = (4.0, h - 12.0);
        let r = self.selection();
        let (a, b) = (r.start.clamp(first, last), r.end.clamp(first, last));
        if a < b {
            out.push(fill(rect(x + width(first, a), y - 1.0, width(a, b), 11.0), rgb(0x3355FF, 0.6)));
        }
        let color = 0xE0E0E0;
        // Gleichfarbige Zeichen zusammen.
        let mut shown: Vec<Segment> = vec![];
        for (i, c) in self.text.iter().enumerate().take(last).skip(first) {
            let color = color_of(i).unwrap_or(color);
            match shown.last_mut() {
                Some(s) if s.color == color => s.text.push(*c),
                _ => shown.push(Segment { color, text: (*c).into() }),
            }
        }
        let line = |t: String| [Segment { color, text: t }];
        java_chat::text(x * unit, y * unit, unit, &shown, 1.0, advance, out);
        if blink {
            let at = x + width(first, self.cursor);
            if self.cursor < self.text.len() {
                out.push(fill(rect(at - 1.0, y - 1.0, 1.0, 11.0), rgb(0xD0D0D0, 1.0)));
            } else if self.text.len() < MAX {
                java_chat::text(at * unit, y * unit, unit, &line("_".into()), 1.0, advance, out);
            }
        }
        self.first = first;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(t: &str) -> ChatInput {
        let mut i = ChatInput::default();
        i.open("");
        i.insert(t);
        i
    }

    #[test]
    fn schreiben_und_loeschen() {
        let mut i = with("häß");
        i.key(Key::Left, false, false);
        i.insert("€");
        assert_eq!(i.text(), "hä€ß");
        i.key(Key::Backspace, false, false);
        i.key(Key::Delete, false, false);
        assert_eq!(i.text(), "hä");
        i.key(Key::Home, false, false);
        i.key(Key::Backspace, false, false);
        assert_eq!(i.text(), "hä");
        i.insert("a\n§c");
        assert_eq!(i.text(), "achä");
    }

    #[test]
    fn worte_und_auswahl() {
        let mut i = with("eins zwei  drei");
        i.key(Key::Backspace, false, true);
        assert_eq!(i.text(), "eins zwei  ");
        i.key(Key::Left, true, true);
        assert_eq!(i.selected(), "zwei  ");
        i.insert("x");
        assert_eq!(i.text(), "eins x");
        i.select_all();
        i.key(Key::Left, false, false);
        i.insert("!");
        assert_eq!(i.text(), "!eins x");
    }

    #[test]
    fn laenge_ist_begrenzt() {
        let mut i = with(&"a".repeat(300));
        assert_eq!(i.text().len(), MAX);
        i.insert("b");
        assert_eq!(i.text().len(), MAX);
        i.select_all();
        i.insert("b");
        assert_eq!(i.text(), "b");
    }

    #[test]
    fn verlauf_behaelt_die_angefangene_zeile() {
        let mut i = with("  eins   a ");
        assert_eq!(i.submit().as_deref(), Some("eins a"));
        i.insert("zwei");
        i.submit();
        i.insert("zwei");
        i.submit();
        i.open("");
        assert_eq!(i.submit(), None);
        i.insert("neu");
        i.key(Key::Up, false, false);
        assert_eq!(i.text(), "zwei");
        i.key(Key::Up, false, false);
        i.key(Key::Up, false, false);
        assert_eq!(i.text(), "eins a");
        i.key(Key::Down, false, false);
        i.key(Key::Down, false, false);
        i.key(Key::Down, false, false);
        assert_eq!(i.text(), "neu");
    }

    #[test]
    fn cursor_bleibt_im_bild() {
        let mut i = with(&"a".repeat(200));
        let edge = |i: &mut ChatInput| {
            let mut out = vec![];
            i.draw((600.0, 300.0), 1.0, true, &|_| 6.0, &|_| None, &mut out);
            out.iter().fold(0.0f32, |m, d| match d {
                Draw::Game { x, text, .. } => m.max(x + text.chars().count() as f32 * 6.0),
                _ => m,
            })
        };
        assert!(edge(&mut i) <= 599.0);
        i.key(Key::Home, false, false);
        assert!(edge(&mut i) <= 599.0);
        assert_eq!(i.first, 0);
    }
}
