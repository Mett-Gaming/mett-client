//! Befehlshinweise mit den Maßen und der Bedienung der Java Edition (26.3), eigene Umsetzung. Die Befehle kommen aus
//! dem laufenden Spiel. Kein Windows, voll testbar.
use crate::chat_input::ChatInput;
use crate::java_chat::{self, Segment};
use crate::{Draw, Rect, rgb};
use std::collections::{BTreeMap, BTreeSet};

const ROWS: usize = 10;
const LINE: f32 = 12.0;
/// Deckkraft des Hintergrunds.
const FILL: f32 = 208.0 / 255.0;
const SELECTED: u32 = 0xFFFF00;
const PLAIN: u32 = 0xAAAAAA;
const GHOST: u32 = 0x808080;
const ERROR: u32 = 0xFF5555;
const ARGUMENTS: [u32; 5] = [0x55FFFF, 0xFFFF55, 0x55FF55, 0xFF55FF, 0xFFAA00];
const SELECTORS: [&str; 6] = ["@a", "@e", "@n", "@p", "@r", "@s"];
/// Verschachtelte Befehle (`execute`) bleiben damit schnell.
const MAX_STEPS: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Ein Wort, ohne Vorschläge.
    Word,
    Number,
    /// Werte aus `Commands::lists`.
    List(usize),
    Selector,
    /// Drei Koordinaten.
    Position,
    /// Alles bis zum Ende.
    Rest,
    /// Wieder eine Variante desselben Befehls.
    Chain,
    /// Ein ganzer Befehl ohne Schrägstrich.
    Command,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub optional: bool,
    pub kind: Kind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Commands {
    pub commands: BTreeMap<String, Vec<Vec<Param>>>,
    pub lists: Vec<Vec<String>>,
    /// Anderer Name → Befehl.
    pub aliases: BTreeMap<String, String>,
}

impl Commands {
    fn variants(&self, name: &str) -> Option<&Vec<Vec<Param>>> {
        let name = self.aliases.get(name).map_or(name, String::as_str);
        self.commands.get(name)
    }
}

#[derive(Debug, Default, PartialEq)]
struct Analysis {
    /// Anfang des gerade getippten Worts.
    start: usize,
    found: BTreeSet<String>,
    usage: BTreeSet<String>,
    /// Farbe je Wort: Anfang, Ende, Farbe.
    colors: Vec<(usize, usize, u32)>,
}

/// Wörter mit Anfang und Ende; Leerzeichen in Anführungszeichen und Klammern trennen nicht. Das letzte ist das
/// gerade getippte und darf leer sein.
fn words(t: &[char], from: usize) -> Vec<(usize, usize)> {
    let (mut out, mut start, mut depth, mut quoted) = (vec![], from, 0i32, false);
    let mut i = from;
    while i < t.len() {
        match t[i] {
            '\\' if quoted => i += 1,
            '"' => quoted = !quoted,
            '[' | '{' if !quoted => depth += 1,
            ']' | '}' if !quoted => depth -= 1,
            ' ' if !quoted && depth <= 0 => {
                if i > start {
                    out.push((start, i));
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push((start.min(t.len()), t.len()));
    out
}

/// Wie viele Koordinaten in einem Wort stehen: `~~1~` sind drei.
fn coordinates(w: &str) -> usize {
    w.chars().filter(|c| matches!(c, '~' | '^')).count().max(1)
}

/// Auch angefangene Zahlen wie `-` sind in Ordnung.
fn number(w: &str) -> bool {
    let w = w.strip_prefix(['-', '+']).unwrap_or(w);
    w.chars().all(|c| c.is_ascii_digit() || c == '.') && w.matches('.').count() <= 1
}

/// Java schlägt auch vor, wenn ein Teil hinter `_`, `.`, `/` oder `:` so anfängt.
fn fits(value: &str, typed: &str) -> bool {
    let (v, t) = (value.to_lowercase(), typed.to_lowercase());
    v.starts_with(&t) || v.match_indices(['_', '.', '/', ':']).any(|(i, _)| v[i + 1..].starts_with(&t))
}

struct Search<'a> {
    c: &'a Commands,
    text: Vec<String>,
    /// Bis hierhin passten die Wörter; je Wort, ob es ein festes Wort ist.
    best: Vec<bool>,
    a: Analysis,
    steps: usize,
}

impl Search<'_> {
    fn suggest<'b>(&mut self, values: impl Iterator<Item = &'b str>) {
        let typed = self.text.last().cloned().unwrap_or_default();
        self.a.found.extend(values.filter(|v| fits(v, &typed) && *v != typed).map(str::to_owned));
    }

    fn reached(&mut self, path: &[bool]) {
        if path.len() > self.best.len() {
            self.best = path.to_vec();
        }
    }

    /// Ab Wort `at` beginnt ein Befehl.
    fn command(&mut self, at: usize, path: &mut Vec<bool>) {
        let c = self.c;
        if at + 1 == self.text.len() {
            self.suggest(c.commands.keys().chain(c.aliases.keys()).map(String::as_str));
            // Ein schon vollständiger Name gilt als erkannt.
            if c.variants(&self.text[at]).is_some() {
                path.push(true);
                self.reached(path);
                path.pop();
            }
            return;
        }
        let name = self.text[at].clone();
        let Some(variants) = c.variants(&name) else { return };
        path.push(true);
        self.reached(path);
        for v in variants {
            self.params(variants, v, at + 1, path);
        }
        // Manche Server nennen keine Parameter; dann ist alles erlaubt.
        if variants.is_empty() {
            let n = path.len();
            path.resize(self.text.len(), false);
            self.reached(path);
            path.truncate(n);
        }
        path.pop();
    }

    fn params(&mut self, all: &[Vec<Param>], rest: &[Param], at: usize, path: &mut Vec<bool>) {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return;
        }
        let Some(p) = rest.first() else { return };
        let c = self.c;
        let last = at + 1 == self.text.len();
        match p.kind {
            Kind::Chain => {
                for v in all {
                    self.params(all, v, at, path);
                }
                return;
            }
            Kind::Command => return self.command(at, path),
            _ => {}
        }
        let word = self.text[at].clone();
        if p.kind == Kind::Number && !number(&word) {
            return;
        }
        if last {
            match p.kind {
                Kind::List(l) => {
                    if let Some(values) = c.lists.get(l) {
                        self.suggest(values.iter().map(String::as_str));
                        // Angefangene Werte sind noch kein Fehler.
                        if !values.iter().any(|v| fits(v, &word)) {
                            return;
                        }
                    }
                }
                Kind::Selector => self.suggest(SELECTORS.into_iter()),
                Kind::Position if word.is_empty() => self.suggest(["~", "~ ~ ~"].into_iter()),
                _ => {}
            }
            self.a.usage.insert(usage(c, rest));
            path.push(literal(c, p));
            self.reached(path);
            path.pop();
            return;
        }
        match p.kind {
            Kind::Rest => {
                let n = path.len();
                path.resize(self.text.len(), false);
                self.reached(path);
                path.truncate(n);
                self.a.usage.insert(usage(c, rest));
            }
            Kind::Position => {
                let (mut have, mut next) = (0, at);
                let n = path.len();
                while have < 3 && next + 1 < self.text.len() {
                    have += coordinates(&self.text[next]);
                    next += 1;
                    path.push(false);
                }
                self.reached(path);
                if have == 3 {
                    self.params(all, &rest[1..], next, path);
                } else if have < 3 {
                    // Das getippte Wort ist noch eine Koordinate.
                    self.a.usage.insert(usage(c, rest));
                    if self.text[next].is_empty() {
                        self.suggest(["~"].into_iter());
                    }
                    path.push(false);
                    self.reached(path);
                }
                path.truncate(n);
            }
            Kind::List(l) if !c.lists.get(l).is_some_and(|v| v.iter().any(|v| v.eq_ignore_ascii_case(&word))) => {
                // Namensräume lässt das Spiel bei manchen Werten weg; nur Fremdes ist ein Fehler.
            }
            _ => {
                path.push(literal(c, p));
                self.reached(path);
                self.params(all, &rest[1..], at + 1, path);
                path.pop();
            }
        }
    }
}

/// Ein festes Wort, keine Auswahl.
fn literal(c: &Commands, p: &Param) -> bool {
    matches!(p.kind, Kind::List(l) if c.lists.get(l).is_some_and(|v| v.len() == 1))
}

/// Restliche Parameter in Java-Schreibweise.
fn usage(c: &Commands, rest: &[Param]) -> String {
    let one = |p: &Param| {
        let name = match p.kind {
            Kind::List(l) if literal(c, p) => c.lists[l][0].clone(),
            Kind::Chain | Kind::Command => "…".into(),
            _ => format!("<{}>", p.name),
        };
        if p.optional { format!("[{name}]") } else { name }
    };
    rest.iter().map(one).collect::<Vec<_>>().join(" ")
}

/// Untersucht den Text bis zum Cursor.
fn analyse(c: &Commands, text: &[char]) -> Option<Analysis> {
    if text.first() != Some(&'/') {
        return None;
    }
    let spans = words(text, 1);
    let texts: Vec<String> = spans.iter().map(|(a, b)| text[*a..*b].iter().collect()).collect();
    let mut s = Search { c, text: texts, best: vec![], a: Analysis::default(), steps: 0 };
    s.command(0, &mut vec![]);
    let Search { best, mut a, text: texts, .. } = s;
    a.start = spans.last()?.0;
    let mut n = 0;
    for (i, (from, to)) in spans.iter().enumerate() {
        let color = match best.get(i) {
            Some(true) => PLAIN,
            Some(false) => {
                n += 1;
                ARGUMENTS[(n - 1) % ARGUMENTS.len()]
            }
            // Solange es Vorschläge gibt, wird nur noch getippt.
            None if i + 1 == texts.len() && (!a.found.is_empty() || texts[i].is_empty()) => PLAIN,
            None => ERROR,
        };
        a.colors.push((*from, *to, color));
    }
    a.usage.remove("");
    Some(a)
}

/// Hinweise zur aktuellen Eingabe.
#[derive(Debug, Default)]
pub struct Hints {
    text: Vec<char>,
    cursor: usize,
    start: usize,
    list: Vec<String>,
    usage: Vec<String>,
    colors: Vec<(usize, usize, u32)>,
    selected: usize,
    offset: usize,
    /// Nach Tab bleibt die Liste stehen, damit weiteres Tab durchschaltet.
    applied: bool,
    /// Lage der Liste vom letzten Zeichnen, in Bildpunkten.
    rect: Option<Rect>,
}

impl Hints {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Nach jeder Eingabe.
    pub fn update(&mut self, c: &Commands, input: &ChatInput) {
        let (text, cursor) = input.content();
        if self.text == text && self.cursor == cursor {
            return;
        }
        *self = Self { text: text.to_vec(), cursor, ..Self::default() };
        if let Some(a) = analyse(c, &text[..cursor]) {
            self.start = a.start;
            // Mit Namensraum nur, wenn es den Wert nicht auch ohne gibt oder schon einer getippt ist.
            let spaced = self.text[a.start..cursor].contains(&':');
            let plain = |v: &String| v.strip_prefix("minecraft:").is_none_or(|n| spaced || !a.found.contains(n));
            self.list = a.found.iter().filter(|v| plain(v)).cloned().collect();
            self.usage = a.usage.into_iter().take(ROWS).collect();
            self.colors = a.colors;
        }
        // Reihenfolge wie in Java: erst, was so anfängt.
        let typed: String = self.text[self.start.min(cursor)..cursor].iter().collect::<String>().to_lowercase();
        self.list.sort_by_key(|v| (!v.to_lowercase().starts_with(&typed), v.to_lowercase()));
    }

    /// Ist die Liste offen? Dann gehören ihr Pfeiltasten und Tab.
    pub fn open(&self) -> bool {
        !self.list.is_empty()
    }

    pub fn step(&mut self, by: i32) {
        if self.open() {
            let n = self.list.len() as i32;
            self.selected = (self.selected as i32 + by).rem_euclid(n) as usize;
            self.offset = self.offset.clamp((self.selected + 1).saturating_sub(ROWS), self.selected);
        }
    }

    pub fn scroll(&mut self, rows: i32) {
        let max = self.list.len().saturating_sub(ROWS) as i32;
        self.offset = (self.offset as i32 - rows).clamp(0, max) as usize;
    }

    /// Tab: einsetzen, noch einmal: nächster Vorschlag.
    pub fn apply(&mut self, input: &mut ChatInput, back: bool) {
        if !self.open() {
            return;
        }
        if self.applied {
            self.step(if back { -1 } else { 1 });
        }
        let value = self.list[self.selected].clone();
        input.replace(self.start, self.cursor, &value);
        let (text, cursor) = input.content();
        (self.text, self.cursor) = (text.to_vec(), cursor);
        self.applied = true;
    }

    /// Zeile unter dem Zeiger.
    pub fn hit(&self, p: (f32, f32)) -> Option<usize> {
        let r = self.rect.filter(|r| r.contains(p))?;
        let rows = self.list.len().min(ROWS);
        let i = self.offset + ((p.1 - r.y) / r.h * rows as f32) as usize;
        (i < self.list.len()).then_some(i)
    }

    pub fn select(&mut self, i: usize) {
        self.selected = i.min(self.list.len().saturating_sub(1));
    }

    /// Farbe eines Zeichens der Eingabe.
    pub fn color(&self, i: usize) -> Option<u32> {
        if self.colors.is_empty() {
            return None;
        }
        if i == 0 {
            return Some(PLAIN);
        }
        self.colors.iter().find(|c| (c.0..c.1).contains(&i)).map(|c| c.2)
    }

    pub fn draw(
        &mut self,
        input: &ChatInput,
        screen: (f32, f32),
        unit: f32,
        advance: &dyn Fn(char) -> f32,
        out: &mut Vec<Draw>,
    ) {
        self.rect = None;
        let (w, h) = (screen.0 / unit, screen.1 / unit);
        let width = |s: &str| s.chars().map(advance).sum::<f32>();
        let rect = |x: f32, y: f32, w: f32, h: f32| Rect { x: x * unit, y: y * unit, w: w * unit, h: h * unit };
        let fill = |r: Rect, alpha: f32, color: u32| Draw::Box { rect: r, radius: 0.0, color: rgb(color, alpha) };
        let line = |t: &str, color: u32| [Segment { color, text: t.to_owned() }];
        let x = |wide: f32| input.x(self.start, advance).clamp(0.0, (w - wide).max(0.0));
        if self.open() {
            let rows = self.list.len().min(ROWS);
            let wide = self.list.iter().map(|s| width(s)).fold(0.0, f32::max) + 1.0;
            let (x, y) = ((x(wide) - 1.0).max(0.0), h - 12.0 - 3.0 - rows as f32 * LINE);
            let list = rect(x, y, wide, rows as f32 * LINE);
            out.push(fill(list, FILL, 0));
            let more = [(self.offset > 0, y - 1.0), (self.offset + rows < self.list.len(), y + rows as f32 * LINE)];
            if more.iter().any(|m| m.0) {
                for (dots, at) in more {
                    out.push(fill(rect(x, at, wide, 1.0), FILL, 0));
                    // Gepunktete Linie: Hier geht es weiter.
                    for i in (0..wide as usize).step_by(2).filter(|_| dots) {
                        out.push(fill(rect(x + i as f32, at, 1.0, 1.0), 1.0, 0xFFFFFF));
                    }
                }
            }
            for (i, v) in self.list.iter().enumerate().skip(self.offset).take(rows) {
                let color = if i == self.selected { SELECTED } else { PLAIN };
                let top = y + 2.0 + (i - self.offset) as f32 * LINE;
                java_chat::text((x + 1.0) * unit, top * unit, unit, &line(v, color), 1.0, advance, out);
            }
            self.rect = Some(list);
            // Rest des gewählten Vorschlags blass hinter dem Cursor.
            let typed: String = self.text[self.start.min(self.cursor)..self.cursor].iter().collect();
            if self.cursor == self.text.len()
                && let Some(rest) = self.list[self.selected].strip_prefix(typed.as_str())
            {
                let at = input.x(self.cursor, advance);
                java_chat::text(at * unit, (h - 12.0) * unit, unit, &line(rest, GHOST), 1.0, advance, out);
            }
            return;
        }
        let wide = self.usage.iter().map(|s| width(s)).fold(0.0, f32::max);
        let x = x(wide + 1.0).max(1.0);
        for (i, u) in self.usage.iter().enumerate() {
            let y = h - 27.0 - LINE * i as f32;
            out.push(fill(rect(x - 1.0, y, wide + 2.0, LINE), FILL, 0));
            java_chat::text(x * unit, (y + 2.0) * unit, unit, &line(u, PLAIN), 1.0, advance, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Commands {
        let p = |name: &str, kind, optional| Param { name: name.into(), optional, kind };
        let lists: [&[&str]; 7] = [
            &["sharpness", "smite", "fire_aspect", "minecraft:smite", "minecraft:nur_da"],
            &["set"],
            &["add"],
            &["day", "night"],
            &["as"],
            &["at"],
            &["run"],
        ];
        let l = |i: usize, n: &str| p(n, Kind::List(i), false);
        let c = [
            (
                "enchant",
                vec![
                    vec![p("player", Kind::Selector, false), l(0, "name"), p("level", Kind::Number, true)],
                    // Wie im Spiel: auch als Nummer.
                    vec![p("player", Kind::Selector, false), p("id", Kind::Number, false)],
                ],
            ),
            ("time", vec![vec![l(1, "mode"), l(3, "time")], vec![l(2, "mode"), p("amount", Kind::Word, false)]]),
            ("setblock", vec![vec![p("position", Kind::Position, false), l(0, "block")]]),
            ("say", vec![vec![p("message", Kind::Rest, false)]]),
            ("teleport", vec![vec![p("destination", Kind::Selector, false)]]),
            ("p", vec![]),
            (
                "execute",
                vec![
                    vec![l(4, "sub"), p("origin", Kind::Selector, false), p("chain", Kind::Chain, false)],
                    vec![l(5, "sub"), p("origin", Kind::Selector, false), p("chain", Kind::Chain, false)],
                    vec![l(6, "sub"), p("command", Kind::Command, false)],
                ],
            ),
        ];
        Commands {
            commands: c.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
            lists: lists.iter().map(|v| v.iter().map(|s| s.to_string()).collect()).collect(),
            aliases: [("tp", "teleport"), ("party", "p")].map(|(a, b)| (a.to_owned(), b.to_owned())).into(),
        }
    }

    fn input(t: &str) -> ChatInput {
        let mut i = ChatInput::default();
        i.open("");
        i.insert(t);
        i
    }

    fn hints(t: &str) -> Hints {
        let mut h = Hints::default();
        h.update(&commands(), &input(t));
        h
    }

    fn list(t: &str) -> Vec<String> {
        hints(t).list
    }

    #[test]
    fn befehle_und_andere_namen() {
        assert_eq!(list("/t"), ["teleport", "time", "tp"]);
        assert_eq!(list("/en"), ["enchant"]);
        assert!(list("hallo").is_empty() && list("/gibtsnicht").is_empty());
        assert_eq!(list("/tp "), SELECTORS);
        assert_eq!(list("/").len(), 9);
        // Serverbefehl ohne Parameter, über seinen anderen Namen.
        assert_eq!(list("/pa"), ["party"]);
        for t in ["/party", "/party invite Wer", "/p x"] {
            assert!((0..t.len()).all(|i| hints(t).color(i) != Some(ERROR)), "{t}");
        }
    }

    #[test]
    fn parameter_und_werte() {
        assert_eq!(list("/enchant "), SELECTORS);
        assert_eq!(list("/enchant @s s"), ["sharpness", "smite"]);
        assert_eq!(list("/enchant @s minecraft:s"), ["minecraft:smite"]);
        assert_eq!(list("/enchant @s nur"), ["minecraft:nur_da"]);
        // Auch mitten im Wort.
        assert_eq!(list("/enchant @s asp"), ["fire_aspect"]);
        assert!(list("/enchant @s sharpness").is_empty());
        assert_eq!(hints("/enchant @s 5").usage, ["<id>"]);
        let h = hints("/enchant @s sharpness ");
        assert!(h.list.is_empty());
        assert_eq!(h.usage, ["[<level>]"]);
        assert_eq!(list("/time "), ["add", "set"]);
        assert_eq!(list("/time set "), ["day", "night"]);
        assert_eq!(hints("/time add 5").usage, ["<amount>"]);
        assert_eq!(hints("/say a b c").usage, ["<message>"]);
        // Leerzeichen in Auswahlen trennen nicht.
        assert_eq!(list("/enchant @e[type=pig, r=5] sm"), ["smite"]);
        assert_eq!(list("/enchant \"Zwei Worte\" sm"), ["smite"]);
    }

    #[test]
    fn koordinaten() {
        assert_eq!(list("/setblock "), ["~", "~ ~ ~"]);
        assert_eq!(hints("/setblock 1 2").usage, ["<position> <block>"]);
        assert_eq!(list("/setblock 1 2 3 sh"), ["sharpness"]);
        assert_eq!(list("/setblock ~~1~ sh"), ["sharpness"]);
        assert_eq!(list("/setblock ~ ~~ sh"), ["sharpness"]);
        assert!(list("/setblock 1 2 sh").is_empty());
    }

    #[test]
    fn verschachtelt() {
        assert_eq!(list("/execute "), ["as", "at", "run"]);
        assert_eq!(list("/execute as @a "), ["as", "at", "run"]);
        assert_eq!(list("/execute as @a at @s run enchant @s sm"), ["smite"]);
        assert_eq!(list("/execute run t"), ["teleport", "time", "tp"]);
        // Auch sehr lange Ketten bleiben schnell.
        let long = format!("/execute {}", "as @a ".repeat(40));
        let start = std::time::Instant::now();
        assert_eq!(list(&long), ["as", "at", "run"]);
        assert!(start.elapsed().as_millis() < 500);
    }

    #[test]
    fn farben_wie_in_java() {
        let c = |t: &str, i: usize| hints(t).color(i);
        let t = "/enchant @s sharpness 5";
        assert_eq!(c(t, 0), Some(PLAIN));
        assert_eq!(c(t, 1), Some(PLAIN));
        assert_eq!(c(t, 9), Some(ARGUMENTS[0]));
        assert_eq!(c(t, 12), Some(ARGUMENTS[1]));
        assert_eq!(c(t, 22), Some(ARGUMENTS[2]));
        // Das Beispiel aus dem Spiel: Namensraum ist in Bedrock falsch.
        let wrong = "/enchant @s minecraft:sharpness 5";
        assert_eq!(c(wrong, 12), Some(ERROR));
        assert_eq!(c("/falsch", 1), Some(ERROR));
        assert_eq!(c("/ench", 1), Some(PLAIN));
        assert_eq!(c("/time set day", 7), Some(PLAIN));
        assert_eq!(c("/say hallo du", 11), Some(ARGUMENTS[1]));
        assert_eq!(c("hallo", 0), None);
    }

    #[test]
    fn tab_setzt_ein_und_schaltet_durch() {
        let (c, mut i, mut h) = (commands(), input("/enchant @s s"), Hints::default());
        h.update(&c, &i);
        h.apply(&mut i, false);
        assert_eq!(i.text(), "/enchant @s sharpness");
        h.update(&c, &i);
        h.apply(&mut i, false);
        assert_eq!(i.text(), "/enchant @s smite");
        h.update(&c, &i);
        h.apply(&mut i, true);
        assert_eq!(i.text(), "/enchant @s sharpness");
        // Weitertippen sucht neu.
        i.insert(" ");
        h.update(&c, &i);
        assert!(!h.open());
    }

    #[test]
    fn auswahl_rollt_und_laeuft_um() {
        let mut h = Hints { list: (0..25).map(|i| format!("w{i:02}")).collect(), ..Default::default() };
        h.step(-1);
        assert_eq!((h.selected, h.offset), (24, 15));
        h.step(1);
        assert_eq!((h.selected, h.offset), (0, 0));
        (0..10).for_each(|_| h.step(1));
        assert_eq!((h.selected, h.offset), (10, 1));
        h.scroll(-100);
        assert_eq!(h.offset, 15);
        h.scroll(100);
        assert_eq!(h.offset, 0);
    }

    #[test]
    fn zeichnet_im_bild_und_trifft_mit_der_maus() {
        let (c, i, mut h) = (commands(), input("/"), Hints::default());
        h.update(&c, &i);
        let mut out = vec![];
        h.draw(&i, (800.0, 480.0), 2.0, &|_| 6.0, &mut out);
        let r = h.rect.unwrap();
        // Neun Zeilen, drei Pixel über der Eingabe.
        assert_eq!((r.h, r.y + r.h), (9.0 * 24.0, 480.0 - 30.0));
        assert_eq!(h.hit((r.x + 1.0, r.y + 1.0)), Some(0));
        assert_eq!(h.hit((r.x + 1.0, r.y + r.h - 1.0)), Some(8));
        assert_eq!(h.hit((r.x - 1.0, r.y)), None);
        for d in out {
            if let Draw::Game { x, y, .. } = d {
                assert!(x >= 0.0 && y >= 0.0);
            }
        }
    }
}
