//! Einstellungsmenü: Seitenleiste, Reiter, Suche, Modulkarten und Detailseite. Aufbau wie beim NoRisk Client,
//! Aussehen und Texte von Mett.
use crate::editor::{Action, Editor, Field, View};
use config::{Config, Module, Profile};
use hud::ui::Ui;
use hud::zoom::Transition;
use hud::{Align, Draw, Kind, Rect, Style, rgb, theme};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Modules,
    /// Einstellungen eines Moduls.
    Detail(Kind),
    Crosshair,
    Profiles,
    Options,
}

const BACKGROUNDS: [u32; 6] = [theme::SURFACE, 0x000000, 0x2B2F36, 0x4A2A2A, 0x2E3524, 0xE8E0D1];
const TEXTS: [u32; 6] = [theme::TEXT, 0xFFFFFF, 0x120E0B, 0xD9A34A, 0xD9ABAA, 0x9AA36E];
const ACCENTS: [u32; 6] = [theme::ACCENT, 0xC2A47F, 0xD2615A, 0xD9A34A, 0x8D97A1, 0xD9ABAA];
const MAX_PROFILES: usize = 7;
const HEADER: f32 = 72.0;
const FOOTER: f32 = 44.0;
const CARD: f32 = 92.0;
const GAP: f32 = 12.0;
/// Zeile des Schalters auf der Chat-Seite.
const CHAT_INPUT: usize = 1;

fn themes() -> [(&'static str, Style); 4] {
    let d = Style::default();
    [
        ("Mett", d),
        ("Hell", Style { background: 0xE8E0D1, opacity: 0.85, text: 0x120E0B, ..d }),
        ("Kontrast", Style { background: 0, opacity: 1.0, text: 0xFFFFFF, outline: true, radius: 0.0, ..d }),
        ("Nur Text", Style { opacity: 0.0, text: 0xFFFFFF, ..d }),
    ]
}

/// Symbol und Beschreibung für die Karte.
fn about(k: Kind) -> (&'static str, &'static str) {
    match k {
        Kind::Keystrokes => ("W", "Zeigt gedrückte Tasten und Maustasten."),
        Kind::Cps => ("◎", "Klicks pro Sekunde, links und rechts."),
        Kind::Clock => ("◷", "Aktuelle Uhrzeit."),
        Kind::Fps => ("▶", "Bilder pro Sekunde."),
        Kind::System => ("▤", "Auslastung von CPU, RAM und Grafikkarte."),
        Kind::Battery => ("▮", "Ladestand des Akkus."),
        Kind::Music => ("♪", "Was gerade läuft, z. B. in Spotify."),
        Kind::Ping => ("≋", "Antwortzeit des eingetragenen Servers."),
        Kind::Session => ("⌛", "Wie lange Minecraft schon läuft."),
        Kind::Stopwatch => ("◔", "Stoppuhr: F9 Start und Pause, F10 zurück."),
        Kind::Note => ("✎", "Eigener Text auf dem Bildschirm."),
        Kind::Zoom => ("⌕", "Zoom mit weichem Verlauf und Mausrad."),
        Kind::AppleSkin => ("◍", "Sättigung und Erschöpfung an der Hungerleiste."),
        Kind::Players => ("☷", "Spielerliste wie in Java, solange Tab gehalten wird."),
        Kind::Chat => ("☰", "Minecrafts Chat, frei platziert, mit eigener Eingabe."),
    }
}

/// Seitenleiste und Hauptfenster.
pub fn rects(screen: (f32, f32), dpi: f32) -> (Rect, Rect) {
    let (rail, gap, w, h) = (92.0 * dpi, 12.0 * dpi, 800.0 * dpi, 560.0 * dpi);
    let x = ((screen.0 - rail - gap - w) / 2.0).max(0.0);
    let y = ((screen.1 - h) / 2.0).max(0.0);
    (Rect { x, y, w: rail, h }, Rect { x: x + rail + gap, y, w, h })
}

fn body(main: Rect, s: f32) -> Rect {
    Rect {
        x: main.x + 24.0 * s,
        y: main.y + (HEADER + 16.0) * s,
        w: main.w - 48.0 * s,
        h: main.h - (HEADER + FOOTER + 24.0) * s,
    }
}

pub fn card_rect(main: Rect, s: f32, slot: usize) -> Rect {
    let b = body(main, s);
    let w = (b.w - GAP * s) / 2.0;
    let (col, row) = ((slot % 2) as f32, (slot / 2) as f32);
    Rect { x: b.x + col * (w + GAP * s), y: b.y + row * (CARD + GAP) * s, w, h: CARD * s }
}

pub fn more_rect(card: Rect, s: f32) -> Rect {
    Rect { x: card.x + card.w - 48.0 * s, y: card.y, w: 48.0 * s, h: card.h }
}

pub fn matches(k: Kind, search: &str) -> bool {
    let q = search.trim().to_lowercase();
    q.is_empty() || k.name().to_lowercase().contains(&q) || about(k).1.to_lowercase().contains(&q)
}

impl Editor {
    pub(crate) fn menu(&mut self, ui: &mut Ui, c: &mut Config, rail: Rect, main: Rect) -> Action {
        let s = ui.scale;
        ui.glass(rail);
        ui.glass(main);
        let action = self.rail(ui, c, rail);
        self.header(ui, c, main);
        let line = |y| Rect { x: main.x, y, w: main.w, h: s };
        ui.fill(line(main.y + HEADER * s), theme::BORDER, 0.7);
        ui.fill(line(main.y + main.h - FOOTER * s), theme::BORDER, 0.7);
        let foot = Rect { x: main.x + 24.0 * s, y: main.y + main.h - FOOTER * s, w: main.w - 48.0 * s, h: FOOTER * s };
        let note = std::mem::take(&mut self.note);
        ui.text(foot, 12.0, theme::MUTED, Align::Left, note.clone());
        self.note = note;
        ui.text(foot, 12.0, theme::ACCENT_HOVER, Align::Right, format!("Profil: {}", c.profile().name));
        let b = body(main, s);
        let page = match self.page {
            Page::Modules => self.cards(ui, c, main),
            Page::Detail(k) => self.detail(ui, c, b, k),
            Page::Crosshair => {
                let state = format!("Minecrafts Crosshair: {}", self.cross_state);
                ui.text(self::line(b, 7, s), 12.0, theme::MUTED, Align::Left, state);
                crosshair(ui, c, b)
            }
            Page::Profiles => self.profiles(ui, c, b),
            Page::Options => self.options(ui, c, b),
        };
        if page == Action::None { action } else { page }
    }

    fn go(&mut self, c: &mut Config, page: Page) {
        self.confirm_reset = false;
        self.end_typing(c);
        self.confirm_delete = false;
        self.note.clear();
        self.page = page;
    }

    fn rail(&mut self, ui: &mut Ui, c: &mut Config, rail: Rect) -> Action {
        let s = ui.scale;
        let mut action = Action::None;
        let modules = matches!(self.page, Page::Modules | Page::Detail(_) | Page::Crosshair | Page::Profiles);
        let entries = [("▦", "MODULE", modules), ("✥", "HUD", false), ("⚙", "OPTIONEN", self.page == Page::Options)];
        for (i, (icon, name, on)) in entries.into_iter().enumerate() {
            if ui.rail_item(rail_rect(rail, s, i), icon, name, on) {
                match i {
                    0 => self.go(c, Page::Modules),
                    1 => {
                        self.end_typing(c);
                        self.view = View::Compact;
                    }
                    _ => self.go(c, Page::Options),
                }
            }
        }
        let done = Rect { y: rail.y + rail.h - 84.0 * s, ..rail_rect(rail, s, 0) };
        if ui.rail_item(done, "✓", "FERTIG", false) {
            action = Action::Close;
        }
        action
    }

    fn header(&mut self, ui: &mut Ui, c: &mut Config, main: Rect) {
        let s = ui.scale;
        let at = |x: f32, w: f32| Rect { x: main.x + x * s, y: main.y + 18.0 * s, w: w * s, h: 36.0 * s };
        let right = |x: f32, w: f32| at(800.0 - 24.0 - x - w, w);
        if let Page::Detail(k) = self.page {
            if ui.tab(at(24.0, 36.0), "‹", false) {
                self.go(c, Page::Modules);
                return;
            }
            ui.text(at(72.0, 300.0), 18.0, theme::TEXT, Align::Left, k.name().to_uppercase());
            let m = c.profile_mut().modules.get_mut(&k).expect("vollständig");
            if ui.button(right(112.0, 80.0), if m.enabled { "AN" } else { "AUS" }, m.enabled) {
                m.enabled = !m.enabled;
            }
            if ui.button(right(0.0, 104.0), "RESET", false) {
                *m = Module { enabled: m.enabled, ..Module::standard(k) };
            }
            return;
        }
        if self.page == Page::Options {
            ui.text(at(24.0, 300.0), 18.0, theme::TEXT, Align::Left, "OPTIONEN");
            return;
        }
        let tabs = [
            (Page::Modules, "MODULE", 24.0, 104.0),
            (Page::Profiles, "PROFILE", 136.0, 104.0),
            (Page::Crosshair, "CROSSHAIR", 248.0, 124.0),
        ];
        for (page, name, x, w) in tabs {
            if ui.tab(at(x, w), name, self.page == page) {
                self.go(c, page);
            }
        }
        if self.page == Page::Modules {
            let on = self.typing == Some(Field::Search);
            let shown = match (on, self.search.is_empty()) {
                (true, _) => format!("{}|", self.search),
                (false, true) => "Suchen…".into(),
                (false, false) => self.search.clone(),
            };
            if ui.field(right(0.0, 190.0), &shown, on) {
                self.typing = if on { None } else { Some(Field::Search) };
            }
        }
    }

    fn cards(&mut self, ui: &mut Ui, c: &mut Config, main: Rect) -> Action {
        let s = ui.scale;
        let found: Vec<Kind> = Kind::ALL.into_iter().filter(|k| matches(*k, &self.search)).collect();
        let rows = (body(main, s).h / ((CARD + GAP) * s)).max(1.0) as usize;
        let last = found.len().div_ceil(2).saturating_sub(rows);
        if main.contains(ui.input.mouse) && ui.input.wheel != 0.0 {
            // Zeilenweise; es ist nie eine halbe Karte zu sehen.
            self.scroll = if ui.input.wheel > 0.0 { self.scroll.saturating_sub(1) } else { self.scroll + 1 };
        }
        self.scroll = self.scroll.min(last);
        if found.is_empty() {
            ui.text(card_rect(main, s, 0), 14.0, theme::MUTED, Align::Left, "Nichts gefunden.");
        }
        for (slot, k) in found.into_iter().skip(self.scroll * 2).take(rows * 2).enumerate() {
            let r = card_rect(main, s, slot);
            let more = more_rect(r, s);
            let m = c.profile_mut().modules.get_mut(&k).expect("vollständig");
            let (icon, text) = about(k);
            ui.card(r, icon, &k.name().to_uppercase(), text, m.enabled);
            ui.text(more, 18.0, theme::MUTED, Align::Center, "…");
            let inside = r.contains(ui.input.mouse);
            if inside && (ui.input.right_pressed || (ui.input.pressed && more.contains(ui.input.mouse))) {
                self.selected = k;
                self.go(c, Page::Detail(k));
                break;
            }
            if inside && ui.input.pressed {
                m.enabled = !m.enabled;
                self.selected = k;
            }
        }
        if last > 0 {
            // Rollbalken am Rand.
            let b = body(main, s);
            let h = b.h / (last + 1) as f32;
            let y = b.y + self.scroll as f32 * h;
            ui.fill(Rect { x: main.x + main.w - 10.0 * s, y, w: 4.0 * s, h }, theme::ACCENT, 0.9);
        }
        Action::None
    }

    /// Der Chat hat eigene Schalter.
    fn chat(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        let t =
            "Liest den Spielspeicher. Mit eigener Eingabe läuft Code im Spiel. Serverregeln können beides verbieten.";
        ui.text(line(b, 0, s), 12.0, theme::MUTED, Align::Left, t);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, theme::BORDER, 0.7);
        ui.toggle(line(b, CHAT_INPUT, s), "Eigene Eingabe mit T (lädt ein Plugin ins Spiel)", &mut c.chat_input);
        ui.text(
            line(b, 2, s),
            12.0,
            theme::ACCENT_HOVER,
            Align::Left,
            format!("Eingabe: {}   Chat-Anzeige: {}", self.chat_state, self.panel_state),
        );
        let mut entries = c.chat_entries() as f32;
        ui.slider_reset(3, line(b, 3, s), "Höchstens Nachrichten", &mut entries, (1.0, 40.0), 1.0, 10.0);
        c.chat_entries = entries.round() as u32;
        ui.toggle(line(b, 4, s), "Nachrichten sanft ausblenden", &mut c.chat_fade);
        ui.toggle(line(b, 5, s), "Aussehen wie in der Java Edition", &mut c.chat_java);
        let state = format!("Java-Aussehen: {}", self.style_state);
        ui.text(line(b, 6, s), 12.0, theme::ACCENT_HOVER, Align::Left, state);
        let m = c.profile_mut().modules.get_mut(&Kind::Chat).expect("vollständig");
        if ui.button(cell(b, 7, 0, 3, s), "Größe zurücksetzen", false) {
            (m.place.scale, m.place.w, m.place.h) = (1.0, 1.0, 1.0);
        }
        let t = "Nachrichten zeigt Minecraft selbst, hierher verschoben. Breite und Höhe ziehst du am Rand.";
        ui.text(line(b, 8, s), 12.0, theme::MUTED, Align::Left, t);
        Action::None
    }

    fn apple(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        ui.text(line(b, 0, s), 14.0, theme::TEXT, Align::Left, about(Kind::AppleSkin).1);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, theme::BORDER, 0.7);
        let state = format!("AppleSkin: {}", self.apple_state);
        ui.text(line(b, 1, s), 12.0, theme::ACCENT_HOVER, Align::Left, state);
        ui.toggle(line(b, 2, s), "Sättigung", &mut c.apple_saturation);
        ui.toggle(line(b, 3, s), "Erschöpfung", &mut c.apple_exhaustion);
        let notes = [
            "Liest nur deine eigenen Werte. Nach dem Betreten erscheint es, sobald sich einer davon ändert.",
            "Liest den Spielspeicher. Serverregeln können das verbieten.",
        ];
        for (i, t) in notes.into_iter().enumerate() {
            ui.text(line(b, 4 + i, s), 12.0, theme::MUTED, Align::Left, t);
        }
        Action::None
    }

    fn players(&mut self, ui: &mut Ui, b: Rect) -> Action {
        let s = ui.scale;
        ui.text(line(b, 0, s), 14.0, theme::TEXT, Align::Left, about(Kind::Players).1);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, theme::BORDER, 0.7);
        let state = format!("Spielerliste: {}", self.tab_state);
        ui.text(line(b, 1, s), 12.0, theme::ACCENT_HOVER, Align::Left, state);
        let notes = [
            "Liest Namen und Gesichter aus Minecrafts Spielerliste, nur solange Tab gehalten wird.",
            "Tab erreicht Minecraft beim Spielen dann nicht. Serverregeln können das verbieten.",
        ];
        for (i, t) in notes.into_iter().enumerate() {
            ui.text(line(b, 2 + i, s), 12.0, theme::MUTED, Align::Left, t);
        }
        Action::None
    }

    /// Dieselben Einstellungen wie Zoomify, soweit Bedrock sie hergibt.
    fn zoom(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        let pages = ["Allgemein", "Übergang", "Mausrad", "Zweit-Zoom"];
        for (i, name) in pages.into_iter().enumerate() {
            if ui.tab(cell(b, 0, i, 4, s), name, self.zoom_page == i) {
                self.zoom_page = i;
            }
        }
        let d = hud::zoom::Settings::default();
        let z = &mut c.zoom;
        let at = |i: usize| line(b, i + 1, s);
        // Ganze Zahlen über die üblichen Regler.
        let whole = |ui: &mut Ui, id: u32, i: usize, text: &str, v: &mut i32, range: (f32, f32), step: f32, d: i32| {
            let mut f = *v as f32;
            ui.slider_reset(id, at(i), text, &mut f, range, step, d as f32);
            *v = f.round() as i32;
        };
        match self.zoom_page {
            0 => {
                let state = format!("Zoom: {}", self.zoom_state);
                ui.text(at(0), 12.0, theme::ACCENT_HOVER, Align::Left, state);
                key(ui, at(1), "Taste", &mut z.key);
                ui.toggle(at(2), "Umschalten statt halten", &mut z.toggle);
                ui.slider_reset(10, at(3), "Anfangs-Zoom", &mut z.initial, (1.0, 10.0), 1.0, d.initial);
                whole(
                    ui,
                    11,
                    4,
                    "Maus langsamer mit dem Zoom (%)",
                    &mut z.sensitivity,
                    (0.0, 100.0),
                    10.0,
                    d.sensitivity,
                );
                whole(ui, 12, 5, "Film-Kamera (%, 0 ist aus)", &mut z.cinematic, (0.0, 200.0), 10.0, d.cinematic);
                let t = "Hand, Gehbewegung und Fernrohr lassen sich in Bedrock nicht getrennt steuern.";
                ui.text(at(6), 12.0, theme::MUTED, Align::Left, t);
            }
            1 => {
                ui.slider_reset(13, at(0), "Hinein (s)", &mut z.time_in, (0.0, 5.0), 0.1, d.time_in);
                ui.slider_reset(14, at(1), "Hinaus (s)", &mut z.time_out, (0.0, 5.0), 0.1, d.time_out);
                ease(ui, at(2), "Verlauf hinein", &mut z.ease_in);
                ease(ui, at(3), "Verlauf hinaus", &mut z.ease_out);
            }
            2 => {
                ui.toggle(at(0), "Mit dem Mausrad weiter zoomen", &mut z.scroll);
                whole(ui, 15, 1, "Stufen", &mut z.steps, (0.0, 50.0), 1.0, d.steps);
                whole(ui, 16, 2, "Zoom je Stufe (%)", &mut z.per_step, (110.0, 300.0), 10.0, d.per_step);
                whole(ui, 17, 3, "Weichheit (%)", &mut z.smoothness, (0.0, 100.0), 10.0, d.smoothness);
                ui.toggle(at(4), "Stufe behalten", &mut z.retain);
                ui.toggle(at(5), "Stufen auch mit Tasten", &mut z.scroll_keys);
                key(ui, at(6), "Taste näher", &mut z.key_in);
                key(ui, at(7), "Taste weiter", &mut z.key_out);
            }
            _ => {
                ui.toggle(at(0), "Zweit-Zoom", &mut z.second);
                key(ui, at(1), "Taste (schaltet um)", &mut z.second_key);
                ui.slider_reset(18, at(2), "Zoom", &mut z.second_amount, (2.0, 10.0), 1.0, d.second_amount);
                ui.slider_reset(19, at(3), "Hinein (s)", &mut z.second_in, (0.0, 30.0), 1.0, d.second_in);
                ui.slider_reset(20, at(4), "Hinaus (s)", &mut z.second_out, (0.0, 30.0), 1.0, d.second_out);
                ui.toggle(at(5), "Mett-HUD dabei ausblenden", &mut z.second_hides_hud);
            }
        }
        if ui.button(cell(b, 10, 0, 3, s), "Wie Zoomify", false) {
            c.zoom = d;
        }
        Action::None
    }

    fn detail(&mut self, ui: &mut Ui, c: &mut Config, b: Rect, k: Kind) -> Action {
        match k {
            Kind::Chat => return self.chat(ui, c, b),
            Kind::Zoom => return self.zoom(ui, c, b),
            Kind::Players => return self.players(ui, b),
            Kind::AppleSkin => return self.apple(ui, c, b),
            _ => {}
        }
        let s = ui.scale;
        ui.text(line(b, 0, s), 14.0, theme::TEXT, Align::Left, about(k).1);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, theme::BORDER, 0.7);
        let d = Module::standard(k);
        let m = c.profile_mut().modules.get_mut(&k).expect("vollständig");
        if ui.slider_reset(1, line(b, 1, s), "Größe", &mut m.place.scale, (0.5, 3.0), 0.1, d.place.scale) {
            (m.place.w, m.place.h) = (1.0, 1.0);
        }
        let names: Vec<&str> = themes().iter().map(|t| t.0).collect();
        let now = themes().iter().position(|t| t.1 == m.style);
        if let Some(i) = ui.stepper(line(b, 2, s), "Stil", &names, now) {
            m.style = themes()[i].1;
        }
        ui.slider_reset(2, line(b, 3, s), "Deckkraft", &mut m.style.opacity, (0.0, 1.0), 0.05, d.style.opacity);
        ui.slider_reset(3, line(b, 4, s), "Ecken", &mut m.style.radius, (0.0, 20.0), 1.0, d.style.radius);
        ui.colors(line(b, 5, s), "Hintergrund", &mut m.style.background, &BACKGROUNDS);
        ui.colors(line(b, 6, s), "Schriftfarbe", &mut m.style.text, &TEXTS);
        ui.colors(line(b, 7, s), "Akzentfarbe", &mut m.style.accent, &ACCENTS);
        ui.toggle(line(b, 8, s), "Umriss", &mut m.style.outline);
        let style = m.style;
        match k {
            Kind::Ping => {
                let t = "Misst zu dem Server, mit dem du gerade verbunden bist. Liest dafür den Spielspeicher.";
                ui.text(line(b, 9, s), 12.0, theme::MUTED, Align::Left, t);
            }
            Kind::Note => self.text_field(ui, c, line(b, 9, s), "Text", Field::Note),
            _ => {}
        }
        if ui.button(cell(b, 10, 0, 3, s), "Auf alle anwenden", false) {
            c.profile_mut().apply_to_all(style);
        }
        Action::None
    }

    fn text_field(&mut self, ui: &mut Ui, c: &mut Config, r: Rect, label: &str, field: Field) {
        ui.text(r, 13.0, theme::TEXT, Align::Left, label);
        let on = self.typing == Some(field);
        let mut text = c.profile().note.clone();
        if on {
            text.push('|');
        } else if text.is_empty() {
            text = "Klicken und tippen".into();
        }
        let at = Rect { x: r.x + r.w * 0.42, w: r.w * 0.58, ..r };
        if ui.field(at, &text, on) {
            self.typing = if on { None } else { Some(field) };
        }
    }

    fn profiles(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        let mut action = Action::None;
        for i in 0..c.profiles.len().min(MAX_PROFILES) {
            let mut name = c.profiles[i].name.clone();
            if self.typing == Some(Field::Name) && i == c.active {
                name.push('|');
            }
            if ui.tab(line(b, i, s), &name, i == c.active) && i != c.active {
                self.end_typing(c);
                c.active = i;
                self.undo.clear();
                self.confirm_delete = false;
            }
        }
        let room = c.profiles.len() < MAX_PROFILES;
        if ui.button(cell(b, 8, 0, 4, s), "Neu", false) && room {
            self.end_typing(c);
            c.add(Profile::default());
            self.undo.clear();
        }
        if ui.button(cell(b, 8, 1, 4, s), "Kopieren", false) && room {
            self.end_typing(c);
            c.add(c.profile().clone());
            self.undo.clear();
        }
        let renaming = self.typing == Some(Field::Name);
        if ui.button(cell(b, 8, 2, 4, s), "Umbenennen", renaming) {
            self.end_typing(c);
            self.typing = (!renaming).then_some(Field::Name);
        }
        if ui.button(delete_rect(b, s), if self.confirm_delete { "Sicher?" } else { "Löschen" }, self.confirm_delete) {
            // Erst der zweite Klick löscht.
            if self.confirm_delete {
                self.typing = None;
                if !c.remove_active() {
                    self.note = "Das letzte Profil bleibt.".into();
                }
                self.undo.clear();
            }
            self.confirm_delete = !self.confirm_delete;
        }
        let Some(dir) = self.folder.clone() else { return action };
        if ui.button(cell(b, 9, 0, 3, s), "Exportieren", false) {
            self.note = match config::export(&dir, c.profile()) {
                Ok(_) => "Profil liegt im Ordner.".into(),
                Err(e) => format!("Fehler: {e}"),
            };
        }
        if ui.button(cell(b, 9, 1, 3, s), "Importieren", false) {
            self.end_typing(c);
            self.note = match config::import(&dir) {
                Ok((good, bad)) => {
                    // Schon vorhandene, unveränderte Profile nicht verdoppeln.
                    let new: Vec<_> = good.into_iter().filter(|p| !c.profiles.contains(p)).collect();
                    let count = new.len().min(MAX_PROFILES.saturating_sub(c.profiles.len()));
                    new.into_iter().take(count).for_each(|p| c.add(p));
                    format!("{count} übernommen, {bad} nicht lesbar.")
                }
                Err(_) => "Noch keine Profile im Ordner.".into(),
            };
            self.undo.clear();
        }
        if ui.button(cell(b, 9, 2, 3, s), "Ordner öffnen", false) {
            action = Action::OpenFolder;
        }
        action
    }
}

fn crosshair(ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
    let s = ui.scale;
    let p = c.profile_mut();
    ui.toggle(line(b, 0, s), "Eigenes Crosshair", &mut p.crosshair_enabled);
    let d = Profile::default().crosshair;
    let x = &mut p.crosshair;
    ui.slider_reset(10, line(b, 1, s), "Länge", &mut x.length, (0.0, 30.0), 1.0, d.length);
    ui.slider_reset(11, line(b, 2, s), "Lücke", &mut x.gap, (0.0, 20.0), 1.0, d.gap);
    ui.slider_reset(12, line(b, 3, s), "Dicke", &mut x.thickness, (1.0, 8.0), 1.0, d.thickness);
    ui.slider_reset(13, line(b, 4, s), "Deckkraft", &mut x.alpha, (0.1, 1.0), 0.05, d.alpha);
    ui.toggle(line(b, 5, s), "Punkt in der Mitte", &mut x.dot);
    ui.colors(line(b, 6, s), "Farbe", &mut x.color, &TEXTS);
    Action::None
}

/// Wählbare Zoom-Tasten.
const KEYS: [(u8, &str); 18] = [
    (0, "keine"),
    (b'C', "C"),
    (b'V', "V"),
    (b'B', "B"),
    (b'N', "N"),
    (b'M', "M"),
    (b'G', "G"),
    (b'H', "H"),
    (b'J', "J"),
    (b'K', "K"),
    (b'R', "R"),
    (b'X', "X"),
    (b'Y', "Y"),
    (b'Z', "Z"),
    (0x75, "F6"),
    (0x76, "F7"),
    (0x21, "Bild auf"),
    (0x22, "Bild ab"),
];

fn key(ui: &mut Ui, r: Rect, text: &str, v: &mut u8) {
    let names = KEYS.map(|k| k.1);
    if let Some(i) = ui.stepper(r, text, &names, KEYS.iter().position(|k| k.0 == *v)) {
        *v = KEYS[i].0;
    }
}

fn ease(ui: &mut Ui, r: Rect, text: &str, v: &mut Transition) {
    let names = Transition::ALL.map(Transition::name);
    if let Some(i) = ui.stepper(r, text, &names, Transition::ALL.iter().position(|t| t == v)) {
        *v = Transition::ALL[i];
    }
}

impl Editor {
    fn options(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        options(ui, c, b);
        let s = ui.scale;
        let text = if self.confirm_reset { "Sicher? Schaltet alles aus" } else { "Alles zurücksetzen" };
        if ui.button(reset_rect(b, s), text, self.confirm_reset) {
            // Erst der zweite Klick setzt zurück, und nur mit Sicherung.
            if self.confirm_reset {
                self.end_typing(c);
                let saved = self.folder.as_ref().is_some_and(|f| config::backup(f, c).is_ok());
                self.note = if saved {
                    c.reset();
                    self.undo.clear();
                    "Zurückgesetzt. Sicherung liegt im Profil-Ordner.".into()
                } else {
                    "Sicherung nicht möglich, nichts geändert.".into()
                };
            }
            self.confirm_reset = !self.confirm_reset;
        }
        Action::None
    }
}

pub fn reset_rect(b: Rect, s: f32) -> Rect {
    cell(b, 8, 0, 2, s)
}

fn options(ui: &mut Ui, c: &mut Config, b: Rect) {
    let s = ui.scale;
    ui.toggle(line(b, 0, s), "HUD in Menüs ausblenden", &mut c.hide_in_menus);
    ui.toggle(line(b, 1, s), "Beim Ziehen einrasten", &mut c.snap);
    let help = [
        "Rechte Umschalttaste: HUD bearbeiten, doppelt oder mit linker: dieses Menü",
        "Karte: Klick schaltet, … oder Rechtsklick öffnet die Einstellungen",
        "HUD: ziehen verschiebt, am Rand ziehen ändert die Größe, Pfeiltasten schieben genau",
        "Strg+Z: rückgängig   Esc: fertig",
    ];
    for (i, t) in help.into_iter().enumerate() {
        ui.text(line(b, 3 + i, s), 12.0, theme::MUTED, Align::Left, t);
    }
}

pub fn rail_rect(rail: Rect, s: f32, i: usize) -> Rect {
    Rect { x: rail.x + 6.0 * s, y: rail.y + (16.0 + i as f32 * 84.0) * s, w: rail.w - 12.0 * s, h: 72.0 * s }
}

pub fn delete_rect(b: Rect, s: f32) -> Rect {
    cell(b, 8, 3, 4, s)
}

#[cfg(test)]
pub fn chat_input_rect(b: Rect, s: f32) -> Rect {
    line(b, CHAT_INPUT, s)
}

#[cfg(test)]
pub fn body_of(screen: (f32, f32), dpi: f32) -> Rect {
    body(rects(screen, dpi).1, dpi)
}

fn line(b: Rect, i: usize, s: f32) -> Rect {
    Rect { x: b.x, y: b.y + i as f32 * 38.0 * s, w: b.w, h: 32.0 * s }
}

fn cell(b: Rect, row: usize, i: usize, count: usize, s: f32) -> Rect {
    let gap = 8.0 * s;
    let w = (b.w - gap * (count - 1) as f32) / count as f32;
    Rect { x: b.x + i as f32 * (w + gap), w, ..line(b, row, s) }
}

/// Abdunkeln hinter dem Menü.
pub fn backdrop(screen: (f32, f32)) -> Draw {
    let rect = Rect { x: 0.0, y: 0.0, w: screen.0, h: screen.1 };
    Draw::Box { rect, radius: 0.0, color: rgb(theme::APP, 0.35) }
}
