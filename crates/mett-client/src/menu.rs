//! Einstellungsmenü: Seitenleiste, Reiter, Suche, Modulkarten und Detailseite. Aufbau wie beim NoRisk Client,
//! Aussehen und Texte von Mett.
use crate::editor::{Action, Editor, Field, View};
use crate::picker::{CrossPart, MenuPart, Paint, Part, Picker};
use config::{Config, Design, Module, Profile};
use hud::crosshair::Shape;
use hud::icons::Icon;
use hud::lang::t;
use hud::ui::{Look, Ui};
use hud::zoom::Transition;
use hud::{Align, Draw, Kind, Rect, Style, rgb, theme};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Modules,
    /// Einstellungen eines Moduls.
    Detail(Kind),
    Profiles,
    Options,
    Design,
}

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
fn about(k: Kind) -> (Icon, &'static str) {
    match k {
        Kind::Keystrokes => (Icon::Keyboard, "Zeigt gedrückte Tasten und Maustasten."),
        Kind::Cps => (Icon::Click, "Klicks pro Sekunde, links und rechts."),
        Kind::Clock => (Icon::Clock, "Aktuelle Uhrzeit."),
        Kind::Fps => (Icon::Speed, "Bilder pro Sekunde."),
        Kind::System => (Icon::Processor, "Auslastung von CPU, RAM und Grafikkarte."),
        Kind::Battery => (Icon::Battery, "Ladestand des Akkus."),
        Kind::Music => (Icon::Notes, "Was gerade läuft, z. B. in Spotify."),
        Kind::Ping => (Icon::Bars, "Antwortzeit des Servers, auf dem du spielst."),
        Kind::Session => (Icon::Hourglass, "Wie lange Minecraft schon läuft."),
        Kind::Stopwatch => (Icon::Stopwatch, "Stoppuhr: F9 Start und Pause, F10 zurück."),
        Kind::Note => (Icon::Quill, "Eigener Text auf dem Bildschirm."),
        Kind::Zoom => (Icon::Lens, "Zoom mit weichem Verlauf und Mausrad."),
        Kind::Crosshair => (Icon::Crosshair, "Eigenes Crosshair statt dem von Minecraft."),
        Kind::Fullbright => (Icon::Sun, "Auch im Dunkeln alles hell."),
        Kind::Armor => (Icon::Chestplate, "Deine Rüstung neben der Schnellleiste."),
        Kind::Potions => (Icon::Potion, "Deine Effekte und wie lange sie noch laufen."),
        Kind::Items => (Icon::Bag, "Zählt Gegenstände in deinem Inventar."),
        Kind::AppleSkin => (Icon::Apple, "Sättigung und Erschöpfung an der Hungerleiste."),
        Kind::Players => (Icon::Friends, "Spielerliste wie in Java, solange Tab gehalten wird."),
        Kind::Chat => (Icon::Bubble, "Minecrafts Chat, frei platziert, mit eigener Eingabe."),
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
    q.is_empty() || t(k.name()).to_lowercase().contains(&q) || t(about(k).1).to_lowercase().contains(&q)
}

/// Alle Module nach ihrem Namen in der gezeigten Sprache.
pub fn sorted() -> Vec<Kind> {
    let mut all = Kind::ALL.to_vec();
    all.sort_by_key(|k| t(k.name()).to_lowercase());
    all
}

impl Editor {
    pub(crate) fn menu(&mut self, ui: &mut Ui, c: &mut Config, rail: Rect, main: Rect) -> Action {
        let s = ui.scale;
        // Unter dem offenen Farbwähler reagiert nichts.
        let open = self.picker.take();
        ui.locked = open.is_some();
        ui.glass(rail);
        ui.glass(main);
        let action = self.rail(ui, c, rail);
        self.header(ui, c, main);
        let line = |y| Rect { x: main.x, y, w: main.w, h: s };
        ui.fill(line(main.y + HEADER * s), ui.look.border, 0.7);
        ui.fill(line(main.y + main.h - FOOTER * s), ui.look.border, 0.7);
        let foot = Rect { x: main.x + 24.0 * s, y: main.y + main.h - FOOTER * s, w: main.w - 48.0 * s, h: FOOTER * s };
        let note = std::mem::take(&mut self.note);
        ui.text(foot, 12.0, ui.look.muted, Align::Left, note.clone());
        self.note = note;
        ui.text(foot, 12.0, ui.look.bright(), Align::Right, format!("{}: {}", t("Profil"), c.profile().name));
        let b = body(main, s);
        let page = match self.page {
            Page::Modules => self.cards(ui, c, main),
            Page::Detail(k) => self.detail(ui, c, b, k),
            Page::Profiles => self.profiles(ui, c, b),
            Page::Options => self.options(ui, c, b),
            Page::Design => self.design_page(ui, c, b),
        };
        ui.locked = false;
        if let Some(mut p) = open
            && p.show(ui, c, main)
        {
            self.picker = Some(p);
        }
        if page == Action::None { action } else { page }
    }

    fn pick(&mut self, c: &mut Config, paint: Paint, at: Rect) {
        self.end_typing(c);
        if let Some(color) = paint.of(c).map(|v| *v) {
            self.picker = Some(Picker::new(paint, color, at));
        }
    }

    fn go(&mut self, c: &mut Config, page: Page) {
        self.picker = None;
        self.confirm_reset = false;
        self.end_typing(c);
        self.confirm_delete = false;
        self.note.clear();
        self.page = page;
    }

    fn rail(&mut self, ui: &mut Ui, c: &mut Config, rail: Rect) -> Action {
        let s = ui.scale;
        let mut action = Action::None;
        let modules = matches!(self.page, Page::Modules | Page::Detail(_) | Page::Profiles);
        let entries = [
            (Icon::Puzzle, "MODULE", modules),
            (Icon::Move, "HUD", false),
            (Icon::Cog, "OPTIONEN", self.page == Page::Options),
            (Icon::Palette, "DESIGN", self.page == Page::Design),
        ];
        for (i, (icon, name, on)) in entries.into_iter().enumerate() {
            if ui.rail_item(rail_rect(rail, s, i), icon, name, on) {
                match i {
                    0 => self.go(c, Page::Modules),
                    1 => {
                        self.end_typing(c);
                        self.view = View::Compact;
                    }
                    2 => self.go(c, Page::Options),
                    _ => self.go(c, Page::Design),
                }
            }
        }
        let done = Rect { y: rail.y + rail.h - 84.0 * s, ..rail_rect(rail, s, 0) };
        if ui.rail_item(done, Icon::Check, "FERTIG", false) {
            action = Action::Close;
        }
        action
    }

    fn header(&mut self, ui: &mut Ui, c: &mut Config, main: Rect) {
        let s = ui.scale;
        let at = |x: f32, w: f32| Rect { x: main.x + x * s, y: main.y + 18.0 * s, w: w * s, h: 36.0 * s };
        let right = |x: f32, w: f32| at(800.0 - 24.0 - x - w, w);
        if let Page::Detail(k) = self.page {
            if ui.icon_button(at(24.0, 36.0), Icon::Previous) {
                self.go(c, Page::Modules);
                return;
            }
            ui.text(at(72.0, 300.0), 18.0, ui.look.text, Align::Left, t(k.name()).to_uppercase());
            let m = c.profile_mut().modules.get_mut(&k).expect("vollständig");
            if ui.button(right(112.0, 80.0), if m.enabled { "AN" } else { "AUS" }, m.enabled) {
                m.enabled = !m.enabled;
            }
            if ui.button(right(0.0, 104.0), "RESET", false) {
                *m = Module { enabled: m.enabled, ..Module::standard(k) };
                if k == Kind::Armor {
                    c.profile_mut().armor = Default::default();
                }
                if k == Kind::Crosshair {
                    c.profile_mut().crosshair = Profile::default().crosshair;
                }
            }
            return;
        }
        if self.page == Page::Options {
            ui.text(at(24.0, 300.0), 18.0, ui.look.text, Align::Left, "OPTIONEN");
            return;
        }
        if self.page == Page::Design {
            for (i, (name, x, w)) in [("HUD", 24.0, 104.0), ("MENÜ", 136.0, 104.0)].into_iter().enumerate() {
                if ui.tab(at(x, w), name, self.design_menu == (i == 1)) {
                    self.design_menu = i == 1;
                }
            }
            if ui.button(right(0.0, 104.0), "RESET", false) {
                match self.design_menu {
                    true => c.menu = Look::default(),
                    false => c.profile_mut().design = Design::default(),
                }
            }
            return;
        }
        let tabs = [(Page::Modules, "MODULE", 24.0, 104.0), (Page::Profiles, "PROFILE", 136.0, 104.0)];
        for (page, name, x, w) in tabs {
            if ui.tab(at(x, w), name, self.page == page) {
                self.go(c, page);
            }
        }
        if self.page == Page::Modules {
            let on = self.typing == Some(Field::Search);
            let shown = match (on, self.search.is_empty()) {
                (true, _) => format!("{}|", self.search),
                (false, true) => t("Suchen…").into(),
                (false, false) => self.search.clone(),
            };
            if ui.field(right(0.0, 190.0), &shown, on) {
                self.typing = if on { None } else { Some(Field::Search) };
            }
        }
    }

    fn cards(&mut self, ui: &mut Ui, c: &mut Config, main: Rect) -> Action {
        let s = ui.scale;
        let found: Vec<Kind> = sorted().into_iter().filter(|k| matches(*k, &self.search)).collect();
        let rows = (body(main, s).h / ((CARD + GAP) * s)).max(1.0) as usize;
        let last = found.len().div_ceil(2).saturating_sub(rows);
        if !ui.locked && main.contains(ui.input.mouse) && ui.input.wheel != 0.0 {
            // Zeilenweise; es ist nie eine halbe Karte zu sehen.
            self.scroll = if ui.input.wheel > 0.0 { self.scroll.saturating_sub(1) } else { self.scroll + 1 };
        }
        self.scroll = self.scroll.min(last);
        if found.is_empty() {
            ui.text(card_rect(main, s, 0), 14.0, ui.look.muted, Align::Left, "Nichts gefunden.");
        }
        for (slot, k) in found.into_iter().skip(self.scroll * 2).take(rows * 2).enumerate() {
            let r = card_rect(main, s, slot);
            let more = more_rect(r, s);
            let m = c.profile_mut().modules.get_mut(&k).expect("vollständig");
            let (icon, text) = about(k);
            ui.card(r, icon, &t(k.name()).to_uppercase(), text, m.enabled);
            ui.icon(more, 18.0, ui.look.muted, Icon::Knobs);
            let inside = !ui.locked && r.contains(ui.input.mouse);
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
            ui.fill(Rect { x: main.x + main.w - 10.0 * s, y, w: 4.0 * s, h }, ui.look.accent, 0.9);
        }
        Action::None
    }

    /// Der Chat hat eigene Schalter.
    fn chat(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        let hint =
            "Liest den Spielspeicher. Mit eigener Eingabe läuft Code im Spiel. Serverregeln können beides verbieten.";
        ui.text(line(b, 0, s), 12.0, ui.look.muted, Align::Left, hint);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, ui.look.border, 0.7);
        ui.toggle(line(b, CHAT_INPUT, s), "Eigene Eingabe mit T (lädt ein Plugin ins Spiel)", &mut c.chat_input);
        ui.text(
            line(b, 2, s),
            12.0,
            ui.look.bright(),
            Align::Left,
            format!("{}: {}   {}: {}", t("Eingabe"), t(self.chat_state), t("Chat-Anzeige"), t(self.panel_state)),
        );
        let mut entries = c.chat_entries() as f32;
        ui.slider_reset(3, line(b, 3, s), "Höchstens Nachrichten", &mut entries, (1.0, 40.0), 1.0, 10.0);
        c.chat_entries = entries.round() as u32;
        ui.toggle(line(b, 4, s), "Nachrichten sanft ausblenden", &mut c.chat_fade);
        ui.toggle(line(b, 5, s), "Aussehen wie in der Java Edition", &mut c.chat_java);
        let state = format!("{}: {}", t("Java-Aussehen"), t(self.style_state));
        ui.text(line(b, 6, s), 12.0, ui.look.bright(), Align::Left, state);
        let m = c.profile_mut().modules.get_mut(&Kind::Chat).expect("vollständig");
        if ui.button(cell(b, 7, 0, 3, s), "Größe zurücksetzen", false) {
            (m.place.scale, m.place.w, m.place.h) = (1.0, 1.0, 1.0);
        }
        let hint = "Nachrichten zeigt Minecraft selbst, hierher verschoben. Breite und Höhe ziehst du am Rand.";
        ui.text(line(b, 8, s), 12.0, ui.look.muted, Align::Left, hint);
        Action::None
    }

    fn apple(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        ui.text(line(b, 0, s), 14.0, ui.look.text, Align::Left, about(Kind::AppleSkin).1);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, ui.look.border, 0.7);
        let state = format!("AppleSkin: {}", t(self.apple_state));
        ui.text(line(b, 1, s), 12.0, ui.look.bright(), Align::Left, state);
        ui.toggle(line(b, 2, s), "Sättigung", &mut c.apple_saturation);
        ui.toggle(line(b, 3, s), "Erschöpfung", &mut c.apple_exhaustion);
        let notes = [
            "Liest nur deine eigenen Werte. Nach dem Betreten erscheint es, sobald sich einer davon ändert.",
            "Liest den Spielspeicher. Serverregeln können das verbieten.",
        ];
        for (i, t) in notes.into_iter().enumerate() {
            ui.text(line(b, 4 + i, s), 12.0, ui.look.muted, Align::Left, t);
        }
        Action::None
    }

    /// Dieselben Einstellungen wie NoRisks Armor HUD, soweit Bedrock sie hergibt.
    fn armor(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        use hud::armor_hud::{Durability, Settings, Shown, Side};
        let s = ui.scale;
        let d = Settings::default();
        let x = &mut c.profile_mut().armor;
        let at = |i: usize| line(b, i, s);
        let half = |i: usize, right: usize| cell(b, i, right, 2, s);
        ui.toggle(half(0, 0), "An der Schnellleiste", &mut x.sticky);
        ui.toggle(half(0, 1), "Hintergrund", &mut x.background);
        ui.toggle(half(1, 0), "Senkrecht", &mut x.vertical);
        ui.toggle(half(1, 1), "Umgekehrte Reihenfolge", &mut x.reversed);
        ui.toggle(half(2, 0), "Symbole leerer Plätze", &mut x.icons);
        ui.toggle(half(2, 1), "Warnzeichen", &mut x.warning);
        let sides = [(Side::Left, "Links"), (Side::Right, "Rechts"), (Side::Split, "Beide Seiten")];
        pick_one(ui, at(3), "Seite", &sides, &mut x.side);
        let shown = [
            (Shown::Always, "Immer"),
            (Shown::IfAny, "Wenn etwas angelegt ist"),
            (Shown::NotEmpty, "Nur angelegte Teile"),
            (Shown::Damaged, "Nur fast kaputte"),
        ];
        pick_one(ui, at(4), "Anzeigen", &shown, &mut x.shown);
        let kinds = [(Durability::Bar, "Balken"), (Durability::Number, "Zahl"), (Durability::Percent, "Prozent")];
        pick_one(ui, at(5), "Haltbarkeit", &kinds, &mut x.durability);
        ui.slider_reset(51, at(6), "Wippen der Warnung", &mut x.bob, (0.0, 10.0), 1.0, d.bob);
        ui.slider_reset(52, at(7), "Warnen ab Haltbarkeit", &mut x.min_value, (0.0, 100.0), 1.0, d.min_value);
        ui.slider_reset(53, at(8), "Warnen ab Prozent", &mut x.min_percent, (0.0, 100.0), 1.0, d.min_percent);
        let state = t(self.gear_states[0]);
        let hint = t("Liest nur deine eigenen Werte aus dem Spielspeicher.");
        ui.text(at(10), 12.0, ui.look.muted, Align::Left, format!("{}: {state}   {hint}", t("Rüstung")));
        Action::None
    }

    /// Stellt eine Einstellung von Minecraft um.
    fn fullbright(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        ui.text(line(b, 0, s), 14.0, ui.look.text, Align::Left, about(Kind::Fullbright).1);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, ui.look.border, 0.7);
        let state = format!("Fullbright: {}", t(self.bright_state));
        ui.text(line(b, 1, s), 12.0, ui.look.bright(), Align::Left, state);
        ui.slider_reset(42, line(b, 2, s), "Helligkeit", &mut c.fullbright, (1.0, 15.0), 0.5, 10.0);
        let hint = "Stellt eine Einstellung von Minecraft um und am Ende zurück. Serverregeln können das verbieten.";
        ui.text(line(b, 3, s), 12.0, ui.look.muted, Align::Left, hint);
        Action::None
    }

    fn players(&mut self, ui: &mut Ui, b: Rect) -> Action {
        let s = ui.scale;
        ui.text(line(b, 0, s), 14.0, ui.look.text, Align::Left, about(Kind::Players).1);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, ui.look.border, 0.7);
        let state = format!("{}: {}", t("Spielerliste"), t(self.tab_state));
        ui.text(line(b, 1, s), 12.0, ui.look.bright(), Align::Left, state);
        let notes = [
            "Liest Namen und Gesichter aus Minecrafts Spielerliste, nur solange Tab gehalten wird.",
            "Tab erreicht Minecraft beim Spielen dann nicht. Serverregeln können das verbieten.",
        ];
        for (i, t) in notes.into_iter().enumerate() {
            ui.text(line(b, 2 + i, s), 12.0, ui.look.muted, Align::Left, t);
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
                let state = format!("Zoom: {}", t(self.zoom_state));
                ui.text(at(0), 12.0, ui.look.bright(), Align::Left, state);
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
                let hint = "Hand, Gehbewegung und Fernrohr lassen sich in Bedrock nicht getrennt steuern.";
                ui.text(at(6), 12.0, ui.look.muted, Align::Left, hint);
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
            Kind::Crosshair => return self.crosshair(ui, c, b),
            Kind::Armor => return self.armor(ui, c, b),
            Kind::Fullbright => return self.fullbright(ui, c, b),
            _ => {}
        }
        let s = ui.scale;
        ui.text(line(b, 0, s), 14.0, ui.look.text, Align::Left, about(k).1);
        ui.fill(Rect { x: b.x, y: b.y + 38.0 * s, w: b.w, h: s }, ui.look.border, 0.7);
        let design = c.profile().design;
        let m = c.profile_mut().modules.get_mut(&k).expect("vollständig");
        ui.toggle(line(b, 1, s), "Globales Design benutzen", &mut m.global);
        let global = m.global;
        let mut now = if global { design } else { Design { scale: m.place.scale, style: m.style } };
        let locked = ui.locked;
        ui.locked |= global;
        let (resized, clicked) = controls(ui, b, 2, &mut now);
        ui.locked = locked;
        if global {
            // Gilt gerade nicht.
            let rows = Rect { h: line(b, 10, s).y - line(b, 2, s).y - 4.0 * s, ..line(b, 2, s) };
            ui.fill(rows, ui.look.background, 0.55);
        } else {
            (m.place.scale, m.style) = (now.scale, now.style);
            if resized {
                (m.place.w, m.place.h) = (1.0, 1.0);
            }
        }
        if let Some((part, at)) = clicked {
            self.pick(c, Paint::Module(k, part), at);
        }
        match k {
            Kind::Ping => {
                let hint = "Misst zu dem Server, mit dem du gerade verbunden bist. Liest dafür den Spielspeicher.";
                ui.text(line(b, 10, s), 12.0, ui.look.muted, Align::Left, hint);
            }
            Kind::Note => self.text_field(ui, c, line(b, 10, s), "Text", Field::Note),
            Kind::Items => self.text_field(ui, c, line(b, 10, s), "Gegenstände", Field::Items),
            Kind::Potions => {
                let state = t(self.gear_states[1]);
                let hint = t("Liest nur deine eigenen Werte aus dem Spielspeicher.");
                let text = format!("{}: {state}   {hint}", t(k.name()));
                ui.text(line(b, 10, s), 12.0, ui.look.muted, Align::Left, text);
            }
            _ => {}
        }
        Action::None
    }

    /// Design für alle Module und für dieses Fenster.
    fn design_page(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        if !self.design_menu {
            let hint = "Gilt für jedes Modul, das das globale Design benutzt.";
            ui.text(line(b, 0, s), 12.0, ui.look.muted, Align::Left, hint);
            let (_, clicked) = controls(ui, b, 1, &mut c.profile_mut().design);
            if let Some((part, at)) = clicked {
                self.pick(c, Paint::Design(part), at);
            }
            return Action::None;
        }
        ui.text(line(b, 0, s), 12.0, ui.look.muted, Align::Left, "Aussehen dieses Fensters.");
        let d = Look::default();
        let m = &mut c.menu;
        ui.slider_reset(31, line(b, 1, s), "Deckkraft", &mut m.opacity, (0.2, 1.0), 0.02, d.opacity);
        ui.slider_reset(32, line(b, 2, s), "Ecken", &mut m.radius, (0.0, 16.0), 1.0, d.radius);
        let parts = [
            ("Hintergrund", MenuPart::Background, m.background),
            ("Karten", MenuPart::Card, m.card),
            ("Rahmen", MenuPart::Border, m.border),
            ("Schriftfarbe", MenuPart::Text, m.text),
            ("Nebentext", MenuPart::Muted, m.muted),
            ("Akzentfarbe", MenuPart::Accent, m.accent),
        ];
        for (i, (name, part, color)) in parts.into_iter().enumerate() {
            let (clicked, at) = ui.color(line(b, 3 + i, s), name, color);
            if clicked {
                self.pick(c, Paint::Menu(part), at);
            }
        }
        Action::None
    }

    fn text_field(&mut self, ui: &mut Ui, c: &mut Config, r: Rect, label: &str, field: Field) {
        ui.text(r, 13.0, ui.look.text, Align::Left, label);
        let on = self.typing == Some(field);
        let p = c.profile();
        let mut text = if field == Field::Items { p.items.clone() } else { p.note.clone() };
        if on {
            text.push('|');
        } else if text.is_empty() {
            text = t("Klicken und tippen").into();
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
                    self.note = t("Das letzte Profil bleibt.").into();
                }
                self.undo.clear();
            }
            self.confirm_delete = !self.confirm_delete;
        }
        let Some(dir) = self.folder.clone() else { return action };
        if ui.button(cell(b, 9, 0, 3, s), "Exportieren", false) {
            self.note = match config::export(&dir, c.profile()) {
                Ok(_) => t("Profil liegt im Ordner.").into(),
                Err(e) => format!("{}: {e}", t("Fehler")),
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
                    format!("{}: {count}, {}: {bad}", t("Übernommen"), t("nicht lesbar"))
                }
                Err(_) => t("Noch keine Profile im Ordner.").into(),
            };
            self.undo.clear();
        }
        if ui.button(cell(b, 9, 2, 3, s), "Ordner öffnen", false) {
            action = Action::OpenFolder;
        }
        action
    }
}

/// Regler für Größe und Aussehen ab Zeile `row`. Ob die Größe zurückgesetzt und welche Farbe angeklickt wurde.
fn controls(ui: &mut Ui, b: Rect, row: usize, v: &mut Design) -> (bool, Option<(Part, Rect)>) {
    let s = ui.scale;
    let at = |i: usize| line(b, row + i, s);
    let d = Design::default();
    let resized = ui.slider_reset(1, at(0), "Größe", &mut v.scale, (0.5, 3.0), 0.1, d.scale);
    let names: Vec<&str> = themes().iter().map(|t| t.0).collect();
    let now = themes().iter().position(|t| t.1 == v.style);
    if let Some(i) = ui.stepper(at(1), "Stil", &names, now) {
        v.style = themes()[i].1;
    }
    let y = &mut v.style;
    ui.slider_reset(2, at(2), "Deckkraft", &mut y.opacity, (0.0, 1.0), 0.05, d.style.opacity);
    ui.slider_reset(3, at(3), "Ecken", &mut y.radius, (0.0, 20.0), 1.0, d.style.radius);
    let parts = [
        ("Hintergrund", Part::Background, y.background),
        ("Schriftfarbe", Part::Text, y.text),
        ("Akzentfarbe", Part::Accent, y.accent),
    ];
    let mut clicked = None;
    for (i, (name, part, color)) in parts.into_iter().enumerate() {
        let (hit, field) = ui.color(at(4 + i), name, color);
        if hit {
            clicked = Some((part, field));
        }
    }
    ui.toggle(at(7), "Umriss", &mut y.outline);
    (resized, clicked)
}

impl Editor {
    fn crosshair(&mut self, ui: &mut Ui, c: &mut Config, b: Rect) -> Action {
        let s = ui.scale;
        for (i, name) in ["Form", "Farben"].into_iter().enumerate() {
            if ui.tab(cell(b, 0, i, 4, s), name, self.cross_colors == (i == 1)) {
                self.cross_colors = i == 1;
            }
        }
        // Vorschau, denn in der Bildmitte liegt das Menü darüber.
        let box_ = Rect { x: b.x + b.w - 150.0 * s, y: b.y, w: 150.0 * s, h: 150.0 * s };
        ui.fill(box_, 0x6B7A8C, 0.9);
        let center = (box_.x + box_.w / 2.0, box_.y + box_.h / 2.0);
        hud::crosshair::draw(center, &c.profile().crosshair, s, self.time, ui.out);
        let left = Rect { w: b.w - 170.0 * s, ..b };
        let p = c.profile_mut();
        let d = Profile::default().crosshair;
        let x = &mut p.crosshair;
        let at = |i: usize| if i < 4 { line(left, i, s) } else { line(b, i, s) };
        let mut clicked = None;
        if self.cross_colors {
            let mut color = |ui: &mut Ui, i: usize, name: &str, part, value| {
                let (hit, field) = ui.color(at(i), name, value);
                if hit {
                    clicked = Some((part, field));
                }
            };
            ui.toggle(at(1), "Umkehren wie Vanilla", &mut x.invert);
            // Umgekehrt gibt es keine Farben.
            let locked = ui.locked;
            ui.locked |= x.invert;
            color(ui, 2, "Farbe", CrossPart::Lines, x.color);
            ui.toggle(at(3), "Regenbogen", &mut x.rainbow);
            ui.locked = locked;
            ui.slider_reset(13, at(4), "Deckkraft", &mut x.alpha, (0.1, 1.0), 0.05, d.alpha);
            ui.toggle(at(5), "Punkt in der Mitte", &mut x.dot);
            ui.slider_reset(14, at(6), "Größe des Punkts", &mut x.dot_size, (1.0, 12.0), 1.0, d.dot_size);
            ui.locked |= x.invert;
            color(ui, 7, "Farbe des Punkts", CrossPart::Dot, x.dot_color);
            ui.slider_reset(15, at(8), "Umriss", &mut x.outline, (0.0, 4.0), 1.0, d.outline);
            color(ui, 9, "Farbe des Umrisses", CrossPart::Outline, x.outline_color);
            ui.locked = locked;
            if x.invert {
                for (from, to) in [(2, 3), (7, 9)] {
                    let (top, end) = (at(from), at(to));
                    ui.fill(Rect { h: end.y + end.h - top.y, ..top }, ui.look.background, 0.55);
                }
            }
        } else {
            let names = Shape::ALL.map(Shape::name);
            let now = Shape::ALL.iter().position(|k| *k == x.shape);
            if let Some(i) = ui.stepper(at(1), "Form", &names, now) {
                x.shape = Shape::ALL[i];
            }
            ui.slider_reset(10, at(4), "Breite", &mut x.length, (0.0, 30.0), 1.0, d.length);
            ui.slider_reset(16, at(5), "Höhe", &mut x.height, (0.0, 30.0), 1.0, d.height);
            ui.slider_reset(11, at(6), "Lücke", &mut x.gap, (0.0, 20.0), 1.0, d.gap);
            ui.slider_reset(12, at(7), "Dicke", &mut x.thickness, (1.0, 8.0), 1.0, d.thickness);
            ui.slider_reset(17, at(8), "Drehung", &mut x.rotation, (0.0, 355.0), 5.0, d.rotation);
        }
        let state = format!("{}: {}", t("Minecrafts Crosshair"), t(self.cross_state));
        ui.text(line(b, 10, s), 12.0, ui.look.muted, Align::Left, state);
        if let Some((part, field)) = clicked {
            self.pick(c, Paint::Crosshair(part), field);
        }
        Action::None
    }
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

/// Auswahl aus benannten Werten.
fn pick_one<T: Copy + PartialEq>(ui: &mut Ui, r: Rect, text: &str, all: &[(T, &str)], v: &mut T) {
    let names: Vec<&str> = all.iter().map(|a| a.1).collect();
    if let Some(i) = ui.stepper(r, text, &names, all.iter().position(|a| a.0 == *v)) {
        *v = all[i].0;
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
                    t("Zurückgesetzt. Sicherung liegt im Profil-Ordner.").into()
                } else {
                    t("Sicherung nicht möglich, nichts geändert.").into()
                };
            }
            self.confirm_reset = !self.confirm_reset;
        }
        if ui.button(report_rect(b, s), "Diagnose kopieren", false) {
            return Action::CopyReport;
        }
        Action::None
    }
}

/// Daneben.
pub fn report_rect(b: Rect, s: f32) -> Rect {
    cell(b, 8, 1, 2, s)
}

pub fn reset_rect(b: Rect, s: f32) -> Rect {
    cell(b, 8, 0, 2, s)
}

fn options(ui: &mut Ui, c: &mut Config, b: Rect) {
    let s = ui.scale;
    ui.toggle(line(b, 0, s), "HUD in Menüs ausblenden", &mut c.hide_in_menus);
    ui.toggle(line(b, 1, s), "Beim Ziehen einrasten", &mut c.snap);
    let help = [
        "Rechte Umschalttaste: dieses Menü öffnen und schließen",
        "Karte: Klick schaltet, Regler oder Rechtsklick öffnet die Einstellungen",
        "HUD: ziehen verschiebt, am Rand ziehen ändert die Größe, Pfeiltasten schieben genau",
        "Strg+Z: rückgängig   Esc: fertig",
    ];
    for (i, t) in help.into_iter().enumerate() {
        ui.text(line(b, 3 + i, s), 12.0, ui.look.muted, Align::Left, t);
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
