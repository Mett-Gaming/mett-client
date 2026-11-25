//! Befehlsliste für die Hinweise: lesen, übersetzen, aktuell halten. Nichts wird gespeichert.
use gamemem::commands::{Commands, Place, Values, find, load};
use gamemem::{Reader, commands};
use hud::hints::{self, Kind, Param};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Woran man die Art freier Parameter erkennt: (Befehl, Parameter, Art). Gleiche Art heißt überall dasselbe.
const ANCHORS: [(&str, &str, Kind); 20] = [
    ("give", "amount", Kind::Number),
    ("enchant", "level", Kind::Number),
    ("time", "amount", Kind::Number),
    ("title", "fadeIn", Kind::Number),
    ("kill", "target", Kind::Selector),
    ("give", "player", Kind::Selector),
    ("tell", "target", Kind::Selector),
    ("teleport", "victim", Kind::Selector),
    ("setblock", "position", Kind::Position),
    ("fill", "from", Kind::Position),
    ("summon", "spawnPos", Kind::Position),
    ("teleport", "lookAtPosition", Kind::Position),
    ("particle", "position", Kind::Position),
    ("me", "message", Kind::Rest),
    ("say", "message", Kind::Rest),
    ("tell", "message", Kind::Rest),
    ("title", "titleText", Kind::Rest),
    ("tellraw", "raw json message", Kind::Rest),
    ("titleraw", "raw json titleText", Kind::Rest),
    ("execute", "command", Kind::Command),
];

/// Gilt für den laufenden Spielstart, auch über Weltwechsel hinweg. Widersprüche zählen nicht.
#[derive(Default)]
struct Kinds(HashMap<u64, Option<Kind>>);

impl Kinds {
    fn learn(&mut self, c: &Commands) {
        for (command, param, kind) in ANCHORS {
            let Some(found) = c.commands.get(command) else { continue };
            for p in
                found.variants.iter().flatten().filter(|p| p.kind != 0 && p.name == param && p.values == Values::Free)
            {
                self.0.entry(p.kind).and_modify(|k| *k = k.filter(|k| *k == kind)).or_insert(Some(kind));
            }
        }
    }

    fn get(&self, kind: u64) -> Kind {
        self.0.get(&kind).copied().flatten().unwrap_or(Kind::Word)
    }
}

fn translate(c: &Commands, kinds: &Kinds) -> hints::Commands {
    let param = |p: &gamemem::commands::Param| Param {
        name: p.name.clone(),
        optional: p.optional,
        kind: match p.values {
            Values::List(i) | Values::Changing(i) => Kind::List(i),
            Values::Chain => Kind::Chain,
            Values::Free => kinds.get(p.kind),
        },
    };
    let commands =
        c.commands.iter().map(|(n, v)| (n.clone(), v.variants.iter().map(|v| v.iter().map(param).collect()).collect()));
    let aliases: BTreeMap<_, _> =
        c.commands.iter().flat_map(|(n, v)| v.aliases.iter().map(move |a| (a.clone(), n.clone()))).collect();
    hints::Commands { commands: commands.collect(), lists: c.lists.iter().map(|l| l.values.clone()).collect(), aliases }
}

pub struct Source {
    reader: Reader,
    pid: u32,
    place: Option<Place>,
    stamp: Option<(usize, usize)>,
    kinds: Kinds,
    raw: Commands,
    out: Option<Arc<hints::Commands>>,
    next: Instant,
    wait: Duration,
}

impl Source {
    pub fn open(pid: u32) -> Option<Self> {
        Some(Self {
            reader: Reader::open_for_scan(pid).ok()?,
            pid,
            place: None,
            stamp: None,
            kinds: Kinds::default(),
            raw: Commands::default(),
            out: None,
            next: Instant::now(),
            wait: Duration::from_secs(1),
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Aktueller Stand. `now`: Der Chat wurde gerade geöffnet.
    pub fn poll(&mut self, now: bool) -> Option<Arc<hints::Commands>> {
        if let Some(p) = self.place {
            let stamp = commands::stamp(&self.reader, &p);
            // Andere Anzahl oder anderer Baum: neue Welt oder der Server hat etwas geändert.
            if stamp.is_some() && stamp == self.stamp && !now && Instant::now() < self.next {
                return self.out.clone();
            }
            match load(&self.reader, &p).filter(|_| stamp.is_some()) {
                Some(c) => self.take(c, stamp),
                None => {
                    (self.place, self.out) = (None, None);
                    self.wait = Duration::from_secs(1);
                    self.next = Instant::now();
                }
            }
        }
        if self.place.is_none() && (now || Instant::now() >= self.next) {
            let found = find(&self.reader).and_then(|p| Some((p, load(&self.reader, &p)?)));
            self.next = Instant::now() + self.wait;
            // Im Menü gibt es keine Liste; dann immer seltener.
            self.wait = (self.wait * 2).min(Duration::from_secs(8));
            if let Some((p, c)) = found {
                self.place = Some(p);
                let stamp = commands::stamp(&self.reader, &p);
                self.take(c, stamp);
            }
        }
        self.out.clone()
    }

    fn take(&mut self, c: Commands, stamp: Option<(usize, usize)>) {
        self.stamp = stamp;
        self.next = Instant::now() + Duration::from_secs(5);
        if self.out.is_none() || c != self.raw {
            self.kinds.learn(&c);
            self.out = Some(Arc::new(translate(&c, &self.kinds)));
            self.raw = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamemem::commands::{Command, Param as Raw};

    fn raw(name: &str, kind: u64, values: Values) -> Raw {
        Raw { name: name.into(), optional: false, values, kind }
    }

    fn commands(list: &[(&str, Vec<Raw>)]) -> Commands {
        let c = list.iter().map(|(n, v)| (n.to_string(), Command { variants: vec![v.clone()], ..Default::default() }));
        Commands { commands: c.collect(), lists: vec![] }
    }

    #[test]
    fn arten_gelten_auch_fuer_serverbefehle() {
        let mut k = Kinds::default();
        let vanilla = commands(&[
            ("kill", vec![raw("target", 7, Values::Free)]),
            ("setblock", vec![raw("position", 8, Values::Free)]),
            // Widerspruch: dieselbe Art an zwei Stellen mit anderer Bedeutung.
            ("say", vec![raw("message", 9, Values::Free)]),
            ("give", vec![raw("player", 9, Values::Free)]),
            ("teleport", vec![raw("victim", 0, Values::Free)]),
        ]);
        k.learn(&vanilla);
        let server = commands(&[(
            "warp",
            vec![
                raw("wer", 7, Values::Free),
                raw("wohin", 8, Values::Free),
                raw("x", 9, Values::Free),
                raw("y", 0, Values::Free),
            ],
        )]);
        k.learn(&server);
        let t = translate(&server, &k);
        let kinds: Vec<_> = t.commands["warp"][0].iter().map(|p| p.kind).collect();
        assert_eq!(kinds, [Kind::Selector, Kind::Position, Kind::Word, Kind::Word]);
        assert!(t.aliases.is_empty());
        // Wie auf einem Server: `party` ist ein anderer Name für `p`.
        let mut server = commands(&[("p", vec![])]);
        server.commands.get_mut("p").unwrap().aliases = vec!["party".into()];
        assert_eq!(translate(&server, &k).aliases, [("party".to_owned(), "p".to_owned())].into());
    }
}
