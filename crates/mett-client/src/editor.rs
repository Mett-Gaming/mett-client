//! HUD-Editor und Einstellungsmenü. Ohne Windows, damit die Bedienung testbar bleibt.
use crate::menu::{self, Page};
use crate::picker::Picker;
use config::{Config, Module, Profile};
use hud::editor::{self, Edges, Guide};
use hud::ui::{Input, Ui};
use hud::{Align, Draw, Kind, Placement, Rect, rgb, theme};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    /// Nur das HUD, zum Anordnen.
    Compact,
    /// Mit Einstellungsfenster.
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    None,
    Close,
    OpenFolder,
    CopyReport,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    Search,
    Name,
    Note,
    Items,
}

struct Drag {
    kind: Kind,
    /// Wo die Maus gedrückt wurde, und das Modul in dem Moment.
    from: (f32, f32),
    start: Rect,
    /// Leer heißt verschieben.
    edges: Edges,
}

/// Kleiner geht ein Modul nicht, in Pixeln bei 100 %.
const SMALLEST: (f32, f32) = (16.0, 10.0);

pub struct Editor {
    /// Stand der Chat-Eingabe fürs Menü.
    pub chat_state: &'static str,
    pub panel_state: &'static str,
    pub style_state: &'static str,
    pub tab_state: &'static str,
    pub zoom_state: &'static str,
    pub apple_state: &'static str,
    pub cross_state: &'static str,
    pub bright_state: &'static str,
    /// Gegenstände, Effekte.
    pub gear_states: [&'static str; 2],
    /// Sekunden, für bewegte Farben in der Vorschau.
    pub time: f32,
    pub(crate) picker: Option<Picker>,
    /// Auf der Design-Seite: das Fenster statt der Module.
    pub(crate) design_menu: bool,
    /// Auf der Crosshair-Seite: Farben statt Form.
    pub(crate) cross_colors: bool,
    /// Gewählte Seite der Zoom-Einstellungen.
    pub(crate) zoom_page: usize,
    /// Siehe `items`.
    pub native: Option<(f32, f32)>,
    pub view: View,
    pub(crate) page: Page,
    pub(crate) selected: Kind,
    /// Textfeld, in das gerade getippt wird.
    pub(crate) typing: Option<Field>,
    pub(crate) search: String,
    /// Oberste sichtbare Kartenzeile.
    pub(crate) scroll: usize,
    pub(crate) undo: Vec<Profile>,
    pub(crate) confirm_delete: bool,
    pub(crate) confirm_reset: bool,
    pub(crate) note: String,
    pub(crate) folder: Option<PathBuf>,
    drag: Option<Drag>,
    guides: Vec<Guide>,
    /// Bildschirm und DPI des letzten Bildes.
    last: ((f32, f32), f32),
    active: Option<u32>,
}

/// `native`: Größe von Minecrafts Chat-Anzeige, wenn sie den Platz unseres Chats einnimmt.
pub fn items(p: &Profile, screen: (f32, f32), dpi: f32, native: Option<(f32, f32)>) -> Vec<(Kind, Rect)> {
    // An der Schnellleiste klebt sie von selbst.
    let free = |k: Kind| k != Kind::Armor || !p.armor.sticky;
    let visible = p.modules.iter().filter(|(k, m)| m.enabled && k.placed() && free(**k));
    let rect = |k: Kind, m: &Module| {
        let unit = k.unit(screen, dpi);
        match native.filter(|_| k == Kind::Chat) {
            // Ihre Größe gibt Minecraft vor.
            Some((w, h)) => {
                Placement { scale: 1.0, w: 1.0, h: 1.0, ..m.place }.resolve(screen, (w / unit, h / unit), unit)
            }
            None => p.place(k).resolve(screen, base(p, k), unit),
        }
    };
    visible.map(|(k, m)| (*k, rect(*k, m))).collect()
}

/// Größe ohne Maßstab; die Rüstung kann senkrecht stehen.
fn base(p: &Profile, k: Kind) -> (f32, f32) {
    if k == Kind::Armor { hud::armor_hud::size(&p.armor) } else { k.size() }
}

impl Editor {
    pub fn new(view: View, folder: Option<PathBuf>) -> Self {
        Self {
            view,
            page: Page::Modules,
            selected: Kind::Keystrokes,
            typing: None,
            search: String::new(),
            scroll: 0,
            undo: vec![],
            confirm_delete: false,
            confirm_reset: false,
            note: String::new(),
            folder,
            drag: None,
            guides: vec![],
            last: ((0.0, 0.0), 1.0),
            active: None,
            chat_state: "aus",
            panel_state: "aus",
            style_state: "aus",
            tab_state: "aus",
            zoom_state: "aus",
            apple_state: "aus",
            cross_state: "aus",
            bright_state: "aus",
            gear_states: ["aus"; 2],
            time: 0.0,
            picker: None,
            design_menu: false,
            cross_colors: false,
            zoom_page: 0,
            native: None,
        }
    }

    fn remember(&mut self, p: &Profile) {
        if self.undo.last() != Some(p) {
            self.undo.push(p.clone());
            if self.undo.len() > 50 {
                self.undo.remove(0);
            }
        }
    }

    pub fn undo(&mut self, c: &mut Config) {
        if let Some(p) = self.undo.pop() {
            *c.profile_mut() = p;
        }
    }

    /// Ausgewähltes Modul mit den Pfeiltasten schieben.
    pub fn nudge(&mut self, c: &mut Config, dx: f32, dy: f32) {
        self.remember(c.profile());
        let (screen, dpi) = self.last;
        // Genau ein Bildschirmpixel.
        let unit = self.selected.unit(screen, dpi).max(0.01);
        if let Some(m) = c.profile_mut().modules.get_mut(&self.selected) {
            m.place.x += dx / unit;
            m.place.y += dy / unit;
        }
    }

    pub fn is_typing(&self) -> bool {
        self.typing.is_some() || self.picker.as_ref().is_some_and(|p| p.typed.is_some())
    }

    pub fn char(&mut self, c: &mut Config, ch: char) {
        if let Some(p) = self.picker.as_mut().filter(|p| p.typed.is_some()) {
            match ch {
                '\r' => p.typed = None,
                ch => p.char(ch, c),
            }
            return;
        }
        let Some(field) = self.typing else { return };
        let text = match field {
            Field::Search => &mut self.search,
            Field::Name => &mut c.profile_mut().name,
            Field::Note => &mut c.profile_mut().note,
            Field::Items => &mut c.profile_mut().items,
        };
        let most = if field == Field::Items { 80 } else { 28 };
        match ch {
            '\u{8}' => {
                text.pop();
            }
            '\r' => self.end_typing(c),
            ch if !ch.is_control() && text.chars().count() < most => text.push(ch),
            _ => {}
        }
    }

    /// War einer offen?
    pub fn close_picker(&mut self) -> bool {
        self.picker.take().is_some()
    }

    pub fn end_typing(&mut self, c: &mut Config) {
        if let Some(p) = &mut self.picker {
            p.typed = None;
        }
        if self.typing.take() == Some(Field::Name) && c.profile().name.trim().is_empty() {
            c.profile_mut().name = hud::lang::t("Profil").into();
        }
    }

    pub fn frame(
        &mut self,
        c: &mut Config,
        screen: (f32, f32),
        dpi: f32,
        input: &Input,
        out: &mut Vec<Draw>,
    ) -> Action {
        self.last = (screen, dpi);
        // Regler, Schalter usw. ändern das Profil direkt; der Stand vor dem Klick landet im Rückgängig-Speicher.
        let before = (input.pressed || input.right_pressed).then(|| c.profile().clone());
        let (rail, main) = menu::rects(screen, dpi);
        let full = self.view == View::Full;
        let over = |r: Rect| r.contains(input.mouse);
        // Solange eine Farbe gewählt wird, bewegt sich nichts.
        let over_ui = full && (over(rail) || over(main) || self.picker.is_some());
        if !over_ui || self.drag.is_some() {
            self.modules(c, screen, dpi, input, out);
        } else {
            self.frames(c, screen, dpi, input, false, out);
        }
        self.draw_guides(screen, dpi, out);
        self.draw_size(c, screen, dpi, out);

        let mut active = self.active.take();
        let mut ui = Ui { look: c.menu, locked: false, input, active: &mut active, out, scale: dpi };
        // Öffnet ein Klick auf ein Modul erst das Menü, darf derselbe Klick darin nichts auslösen.
        let action = match (full, self.view) {
            (true, _) => self.menu(&mut ui, c, rail, main),
            (false, _) => Action::None,
        };
        self.active = active;
        if let Some(p) = before
            && p.name == c.profile().name
            && p != *c.profile()
        {
            self.remember(&p);
        }
        action
    }

    /// Minecrafts Chat-Anzeige gibt ihre Breite vor.
    fn allowed(&self, kind: Kind, e: Edges) -> Edges {
        match kind == Kind::Chat && self.native.is_some() {
            true => Edges { left: false, right: false, ..e },
            false => e,
        }
    }

    /// Modul unter der Maus und welche Kanten sie trifft. Das oberste gewinnt.
    fn grip(&self, list: &[(Kind, Rect)], mouse: (f32, f32), dpi: f32) -> Option<(Kind, Rect, Edges)> {
        list.iter().rev().find_map(|(k, r)| {
            let e = self.allowed(*k, editor::edges(*r, mouse, dpi));
            (e.any() || r.contains(mouse)).then_some((*k, *r, e))
        })
    }

    /// Zeigerbild wie an Windows-Fenstern: 0 Pfeil, 1 waagrecht, 2 senkrecht, 3 schräg wie \\, 4 schräg wie /.
    pub fn shape(&self, c: &Config, mouse: (f32, f32)) -> u32 {
        let (screen, dpi) = self.last;
        let e = match &self.drag {
            Some(d) => d.edges,
            None if self.over_ui(screen, dpi, mouse) => return 0,
            None => {
                let list = items(c.profile(), screen, dpi, self.native);
                self.grip(&list, mouse, dpi).map(|g| g.2).unwrap_or_default()
            }
        };
        match (e.left || e.right, e.top || e.bottom) {
            (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (true, true) => {
                if e.left == e.top {
                    3
                } else {
                    4
                }
            }
        }
    }

    fn over_ui(&self, screen: (f32, f32), dpi: f32, mouse: (f32, f32)) -> bool {
        let (rail, main) = menu::rects(screen, dpi);
        match self.view {
            View::Full => rail.contains(mouse) || main.contains(mouse),
            View::Compact => false,
        }
    }

    /// Neue Größe übernehmen.
    fn take(&mut self, c: &mut Config, kind: Kind, r: Rect, e: Edges, screen: (f32, f32), dpi: f32) {
        let unit = kind.unit(screen, dpi);
        let mut r = r;
        if let (Kind::Chat, Some((w, h))) = (kind, self.native) {
            // Ihre Höhe sind ganze Nachrichten; die nicht gezogene Kante bleibt stehen.
            let line = h / c.chat_entries() as f32;
            c.chat_entries = ((r.h / line).round() as u32).clamp(1, 40);
            let new = line * c.chat_entries as f32;
            if e.top {
                r.y += r.h - new;
            }
            r.h = new;
            self.native = Some((w, new));
        }
        let fixed = kind == Kind::Chat && self.native.is_some();
        let scale = c.profile().place(kind).scale;
        let size = base(c.profile(), kind);
        let Some(m) = c.profile_mut().modules.get_mut(&kind) else { return };
        if !fixed {
            let s = scale.clamp(0.25, 4.0) * unit;
            m.place.w = r.w / (size.0 * s);
            m.place.h = r.h / (size.1 * s);
        }
        m.place = editor::placement(r, screen, unit, m.place);
    }

    /// Ziehen und Rechtsklick auf den Modulen.
    fn modules(&mut self, c: &mut Config, screen: (f32, f32), dpi: f32, input: &Input, out: &mut Vec<Draw>) {
        let list = items(c.profile(), screen, dpi, self.native);
        if let Some((kind, rect, edges)) = self.grip(&list, input.mouse, dpi) {
            if input.pressed {
                self.selected = kind;
                if !edges.any() && pen(rect, dpi).contains(input.mouse) {
                    self.open(c, kind);
                } else {
                    self.remember(c.profile());
                    self.drag = Some(Drag { kind, from: input.mouse, start: rect, edges });
                }
            }
            if input.right_pressed {
                self.open(c, kind);
            }
        }
        if let Some(d) = &self.drag {
            if input.down {
                let (kind, edges, start) = (d.kind, d.edges, d.start);
                let by = (input.mouse.0 - d.from.0, input.mouse.1 - d.from.1);
                let others: Vec<Rect> = list.iter().filter(|i| i.0 != kind).map(|i| i.1).collect();
                if edges.any() {
                    let min = (SMALLEST.0 * dpi, SMALLEST.1 * dpi);
                    // Ohne Bewegung ändert ein Klick auf den Rand nichts.
                    if by != (0.0, 0.0) {
                        let mut r = editor::resize(start, edges, by, min, screen);
                        self.guides.clear();
                        if c.snap {
                            let (more, guides) = editor::snap_edges(r, edges, &others, screen, 8.0 * dpi);
                            r = editor::resize(start, edges, (by.0 + more.0, by.1 + more.1), min, screen);
                            self.guides = guides;
                        }
                        self.take(c, kind, r, edges, screen, dpi);
                    }
                } else {
                    let m = c.profile().modules[&kind].clone();
                    let unit = kind.unit(screen, dpi);
                    let free = Rect { x: start.x + by.0, y: start.y + by.1, ..start };
                    let (r, guides) = if c.snap {
                        editor::snap(free, &others, screen, 8.0 * dpi, 16.0 * dpi)
                    } else {
                        (free, vec![])
                    };
                    self.guides = guides;
                    let place = editor::placement(r, screen, unit, m.place);
                    c.profile_mut().modules.get_mut(&kind).expect("vorhanden").place = place;
                }
            } else {
                self.drag = None;
                self.guides.clear();
            }
        }
        self.frames(c, screen, dpi, input, true, out);
    }

    /// Einstellungen eines Moduls zeigen.
    fn open(&mut self, c: &mut Config, kind: Kind) {
        self.end_typing(c);
        self.selected = kind;
        self.page = Page::Detail(kind);
        self.view = View::Full;
    }

    fn frames(&self, c: &Config, screen: (f32, f32), dpi: f32, input: &Input, hover: bool, out: &mut Vec<Draw>) {
        let list = items(c.profile(), screen, dpi, self.native);
        let under = if hover { self.grip(&list, input.mouse, dpi).map(|g| g.0) } else { None };
        for (k, rect) in list {
            let lit = Some(k) == under || k == self.selected;
            let color = if lit { rgb(theme::ACCENT_HOVER, 1.0) } else { rgb(theme::BORDER, 0.9) };
            out.push(Draw::Outline { rect, radius: 4.0 * dpi, width: if lit { 2.0 } else { 1.0 } * dpi, color });
            if Some(k) == under {
                let r = pen(rect, dpi);
                out.push(Draw::Box { rect: r, radius: 4.0 * dpi, color: rgb(theme::ACCENT, 1.0) });
                hud::icons::draw(hud::icons::Icon::Pencil, r, 13.0 * dpi, rgb(theme::APP, 1.0), out);
            }
        }
    }

    /// Maße in Pixeln, solange eine Kante gezogen wird.
    fn draw_size(&self, c: &Config, screen: (f32, f32), dpi: f32, out: &mut Vec<Draw>) {
        let Some(d) = self.drag.as_ref().filter(|d| d.edges.any()) else { return };
        let list = items(c.profile(), screen, dpi, self.native);
        let Some((_, r)) = list.into_iter().find(|i| i.0 == d.kind) else { return };
        let (w, h) = (96.0 * dpi, 20.0 * dpi);
        let below = r.y + r.h + 6.0 * dpi;
        let y = if below + h > screen.1 { (r.y - h - 6.0 * dpi).max(0.0) } else { below };
        let x = (r.x + (r.w - w) / 2.0).clamp(0.0, (screen.0 - w).max(0.0));
        let rect = Rect { x, y, w, h };
        out.push(Draw::Box { rect, radius: 4.0 * dpi, color: rgb(theme::ACCENT, 1.0) });
        let text = format!("{:.0} × {:.0} px", r.w, r.h);
        out.push(Draw::Text { rect, size: 12.0 * dpi, color: rgb(theme::APP, 1.0), align: Align::Center, text });
    }

    fn draw_guides(&self, screen: (f32, f32), dpi: f32, out: &mut Vec<Draw>) {
        for g in &self.guides {
            let rect = if g.vertical {
                Rect { x: g.at, y: 0.0, w: dpi, h: screen.1 }
            } else {
                Rect { x: 0.0, y: g.at, w: screen.0, h: dpi }
            };
            out.push(Draw::Box { rect, radius: 0.0, color: rgb(theme::ACCENT_HOVER, 0.8) });
        }
    }
}

/// Stift in der oberen rechten Ecke eines Moduls.
fn pen(r: Rect, dpi: f32) -> Rect {
    let size = (18.0 * dpi).min(r.h);
    Rect { x: r.x + r.w - size, y: r.y, w: size, h: size }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: (f32, f32) = (1280.0, 720.0);

    fn step(e: &mut Editor, c: &mut Config, mouse: (f32, f32), pressed: bool, down: bool) -> Action {
        e.frame(c, S, 1.0, &Input { mouse, pressed, down, ..Default::default() }, &mut vec![])
    }

    fn rect(c: &Config, k: Kind) -> Rect {
        items(c.profile(), S, 1.0, None).into_iter().find(|i| i.0 == k).unwrap().1
    }

    #[test]
    fn ziehen_verschiebt_und_laesst_sich_zuruecknehmen() {
        let mut c = Config { snap: false, ..Config::default() };
        let start = c.clone();
        let mut e = Editor::new(View::Compact, None);
        step(&mut e, &mut c, (20.0, 20.0), true, true);
        step(&mut e, &mut c, (320.0, 220.0), false, true);
        step(&mut e, &mut c, (320.0, 220.0), false, false);
        let r = rect(&c, Kind::Cps);
        assert!((r.x - 316.0).abs() < 0.01 && (r.y - 216.0).abs() < 0.01, "{r:?}");
        e.undo(&mut c);
        assert_eq!(c, start);
    }

    #[test]
    fn klick_ohne_bewegung_aendert_die_lage_nicht() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Compact, None);
        let before = rect(&c, Kind::Cps);
        step(&mut e, &mut c, (20.0, 20.0), true, true);
        step(&mut e, &mut c, (20.0, 20.0), false, false);
        assert_eq!(rect(&c, Kind::Cps), before);
    }

    #[test]
    fn mausrad_aendert_nichts_und_rechtsklick_oeffnet() {
        let mut c = Config::default();
        let before = c.clone();
        let mut e = Editor::new(View::Compact, None);
        let mut i = Input { mouse: (40.0, 30.0), wheel: 2.0, ..Default::default() };
        e.frame(&mut c, S, 1.0, &i, &mut vec![]);
        assert_eq!(c, before);
        i.wheel = 0.0;
        i.right_pressed = true;
        e.frame(&mut c, S, 1.0, &i, &mut vec![]);
        assert_eq!(e.view, View::Full);
    }

    #[test]
    fn alle_acht_griffe_ziehen_pixelgenau() {
        for k in [Kind::Cps, Kind::Keystrokes, Kind::Chat] {
            for (gx, gy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let mut c = Config { snap: false, ..Config::default() };
                let all = &mut c.profile_mut().modules;
                all.iter_mut().for_each(|(n, m)| m.enabled = *n == k);
                let m = all.get_mut(&k).unwrap();
                m.place = editor::placement(Rect { x: 500.0, y: 300.0, w: 100.0, h: 60.0 }, S, k.unit(S, 1.0), m.place);
                let mut e = Editor::new(View::Compact, None);
                let r = rect(&c, k);
                let at = |g: i32, p: f32, len: f32| {
                    if g < 0 {
                        p
                    } else if g > 0 {
                        p + len
                    } else {
                        p + len / 2.0
                    }
                };
                let from = (at(gx, r.x, r.w), at(gy, r.y, r.h));
                let to = (from.0 + 7.0 * gx as f32, from.1 + 3.0 * gy as f32);
                // Ein Klick allein ändert nichts.
                let start = c.clone();
                step(&mut e, &mut c, from, true, true);
                assert_eq!(c, start);
                let shape = [[3, 2, 4], [1, 0, 1], [4, 2, 3]][(gy + 1) as usize][(gx + 1) as usize];
                assert_eq!(e.shape(&c, from), shape, "{k:?} {gx} {gy}");
                step(&mut e, &mut c, to, false, true);
                step(&mut e, &mut c, to, false, false);
                let n = rect(&c, k);
                let near = |a: f32, b: f32| (a - b).abs() < 0.51;
                assert!(
                    near(n.w, r.w + 7.0 * gx.abs() as f32) && near(n.h, r.h + 3.0 * gy.abs() as f32),
                    "{r:?} {n:?}"
                );
                assert!(near(n.x, r.x - if gx < 0 { 7.0 } else { 0.0 }), "{k:?} {gx} {gy} {r:?} {n:?}");
                assert!(near(n.y, r.y - if gy < 0 { 3.0 } else { 0.0 }), "{k:?} {gx} {gy} {r:?} {n:?}");
                // Gezogene Kanten auf ganzen Pixeln, der Maßstab bleibt.
                if gx > 0 {
                    assert!(((n.x + n.w).round() - (n.x + n.w)).abs() < 0.01);
                }
                assert_eq!(c.profile().modules[&k].place.scale, start.profile().modules[&k].place.scale);
                e.undo(&mut c);
                assert_eq!(c, start);
            }
        }
    }

    #[test]
    fn pfeiltaste_schiebt_einen_pixel() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Compact, None);
        let big = (2560.0, 1440.0);
        e.frame(&mut c, big, 1.5, &Input::default(), &mut vec![]);
        e.selected = Kind::Cps;
        let x = |c: &Config| items(c.profile(), big, 1.5, None).into_iter().find(|i| i.0 == Kind::Cps).unwrap().1.x;
        let before = x(&c);
        e.nudge(&mut c, 1.0, 0.0);
        assert!((x(&c) - before - 1.0).abs() < 0.01);
    }

    #[test]
    fn minecrafts_anzeige_aendert_nur_ihre_nachrichten() {
        let mut c = Config { snap: false, ..Config::default() };
        c.profile_mut().modules.iter_mut().for_each(|(k, m)| m.enabled = *k == Kind::Chat);
        c.profile_mut().modules.get_mut(&Kind::Chat).unwrap().place.y = -60.0;
        let mut e = Editor::new(View::Compact, None);
        e.native = Some((500.0, 160.0));
        let at = |c: &Config, n| items(c.profile(), S, 1.0, n).into_iter().find(|i| i.0 == Kind::Chat).unwrap().1;
        let r = at(&c, e.native);
        step(&mut e, &mut c, (r.x + 250.0, r.y), true, true);
        step(&mut e, &mut c, (r.x + 250.0, r.y - 33.0), false, true);
        step(&mut e, &mut c, (r.x + 250.0, r.y - 33.0), false, false);
        assert_eq!((c.chat_entries, e.native), (12, Some((500.0, 192.0))));
        let n = at(&c, e.native);
        assert!((n.y + n.h - r.y - r.h).abs() < 0.6, "{r:?} {n:?}");
        // Seitlich gibt es keinen Griff.
        assert_eq!(e.shape(&c, (n.x + n.w, n.y + n.h / 2.0)), 0);
    }

    #[test]
    fn fertig_schliesst_und_fenster_verdeckt_module() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Full, None);
        let (rail, main) = menu::rects(S, 1.0);
        let done = (rail.x + 40.0, rail.y + rail.h - 50.0);
        assert_eq!(step(&mut e, &mut c, done, true, true), Action::Close);
        c.profile_mut().modules.get_mut(&Kind::Cps).unwrap().place =
            editor::placement(main, S, 1.0, c.profile().modules[&Kind::Cps].place);
        step(&mut e, &mut c, (main.x + 4.0, main.y + 4.0), true, true);
        assert!(e.drag.is_none());
    }

    #[test]
    fn rahmen_folgt_minecrafts_anzeige() {
        let mut c = Config::default();
        c.profile_mut().modules.get_mut(&Kind::Chat).unwrap().enabled = true;
        let mut e = Editor::new(View::Compact, None);
        e.native = Some((500.0, 160.0));
        let at = |c: &Config| {
            items(c.profile(), S, 1.0, Some((500.0, 160.0))).into_iter().find(|i| i.0 == Kind::Chat).unwrap().1
        };
        let before = at(&c);
        assert_eq!((before.w, before.h), (500.0, 160.0));
        // Beim Ziehen bleibt die Größe und die Ecke folgt der Maus.
        c.snap = false;
        let grab = (before.x + 20.0, before.y + 20.0);
        step(&mut e, &mut c, grab, true, true);
        step(&mut e, &mut c, (grab.0 + 200.0, grab.1 - 200.0), false, true);
        step(&mut e, &mut c, (grab.0 + 200.0, grab.1 - 200.0), false, false);
        let after = at(&c);
        assert!(
            (after.x - before.x - 200.0).abs() < 0.6 && (after.y - before.y + 200.0).abs() < 0.6,
            "{before:?} {after:?}"
        );
        assert_eq!((after.w, after.h), (500.0, 160.0));
        assert!(e.drag.is_none());
    }

    #[test]
    fn chat_eingabe_laesst_sich_auf_der_chat_seite_schalten() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Full, None);
        e.page = Page::Detail(Kind::Chat);
        let r = menu::chat_input_rect(menu::body_of(S, 1.0), 1.0);
        step(&mut e, &mut c, (r.x + r.w - 10.0, r.y + 10.0), true, true);
        step(&mut e, &mut c, (r.x + r.w - 10.0, r.y + 10.0), false, false);
        assert!(c.chat_input);
    }

    fn card(e: &Editor, k: Kind) -> Rect {
        let i = menu::sorted().iter().position(|x| *x == k).unwrap();
        menu::card_rect(menu::rects(S, 1.0).1, 1.0, i - e.scroll * 2)
    }

    #[test]
    fn karte_schaltet_und_punkte_oeffnen_details() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Full, None);
        let r = card(&e, Kind::Fps);
        step(&mut e, &mut c, (r.x + 20.0, r.y + 20.0), true, true);
        assert!(c.profile().modules[&Kind::Fps].enabled);
        let m = menu::more_rect(r, 1.0);
        step(&mut e, &mut c, (m.x + 10.0, m.y + 40.0), true, true);
        assert!(e.page == Page::Detail(Kind::Fps) && c.profile().modules[&Kind::Fps].enabled);
    }

    #[test]
    fn rollen_und_suche() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Full, None);
        let (_, main) = menu::rects(S, 1.0);
        let i = Input { mouse: (main.x + 50.0, main.y + 200.0), wheel: -1.0, ..Default::default() };
        for _ in 0..20 {
            e.frame(&mut c, S, 1.0, &i, &mut vec![]);
        }
        let last = card(&e, *menu::sorted().last().unwrap());
        assert!(e.scroll > 0 && last.y + last.h <= main.y + main.h);
        e.typing = Some(Field::Search);
        "spotify".chars().for_each(|ch| e.char(&mut c, ch));
        step(&mut e, &mut c, (0.0, 0.0), false, false);
        assert_eq!(e.scroll, 0);
        let found: Vec<_> = Kind::ALL.into_iter().filter(|k| menu::matches(*k, &e.search)).collect();
        assert_eq!(found, [Kind::Music]);
    }

    #[test]
    fn stift_oeffnet_ohne_klick_im_menue() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Compact, None);
        let r = rect(&c, Kind::Cps);
        let before = c.clone();
        step(&mut e, &mut c, (r.x + r.w - 4.0, r.y + 4.0), true, true);
        assert!(e.view == View::Full && e.page == Page::Detail(Kind::Cps) && c == before);
    }

    #[test]
    fn loeschen_braucht_zwei_klicks() {
        let mut c = Config::default();
        c.add(Profile::default());
        let mut e = Editor::new(View::Full, None);
        e.page = Page::Profiles;
        let r = menu::delete_rect(menu::body_of(S, 1.0), 1.0);
        let at = (r.x + 5.0, r.y + 5.0);
        step(&mut e, &mut c, at, true, true);
        assert_eq!(c.profiles.len(), 2);
        step(&mut e, &mut c, at, true, true);
        assert_eq!(c.profiles.len(), 1);
    }

    #[test]
    fn alles_zuruecksetzen_braucht_zwei_klicks_und_sichert() {
        let dir = std::env::temp_dir().join(format!("mett-reset-{}", std::process::id()));
        let mut e = Editor::new(View::Full, Some(dir.clone()));
        e.page = Page::Options;
        let mut c = Config { chat_java: true, hide_in_menus: false, ..Config::default() };
        c.add(Profile::default());
        let r = crate::menu::reset_rect(crate::menu::body_of(S, 1.0), 1.0);
        let at = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let click = |e: &mut Editor, c: &mut Config| {
            step(e, c, at, true, true);
            step(e, c, at, false, false);
        };
        let before = c.clone();
        click(&mut e, &mut c);
        assert_eq!(c, before);
        click(&mut e, &mut c);
        assert!(c.profiles.len() == 1 && c.profile().modules.values().all(|m| !m.enabled));
        assert!(!c.chat_java && c.hide_in_menus);
        assert_eq!(config::load(&dir.join("vor-dem-zuruecksetzen.bak")).unwrap(), before);
        // Ohne Sicherung bleibt alles, wie es ist.
        e.folder = None;
        c.chat_java = true;
        click(&mut e, &mut c);
        click(&mut e, &mut c);
        assert!(c.chat_java);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn diagnose_kopieren_aendert_nichts() {
        let mut e = Editor::new(View::Full, None);
        e.page = Page::Options;
        let mut c = Config::default();
        let before = c.clone();
        let r = crate::menu::report_rect(crate::menu::body_of(S, 1.0), 1.0);
        let at = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        assert_eq!(step(&mut e, &mut c, at, true, true), Action::CopyReport);
        assert_eq!(c, before);
    }

    #[test]
    fn module_stehen_alphabetisch() {
        let names: Vec<String> = menu::sorted().iter().map(|k| hud::lang::t(k.name()).to_lowercase()).collect();
        assert!(names.is_sorted() && names.len() == Kind::ALL.len());
    }

    #[test]
    fn globales_design_gilt_bis_es_ausgeschaltet_wird() {
        let mut c = Config::default();
        c.profile_mut().design.scale = 2.0;
        assert_eq!(rect(&c, Kind::Cps).w, 260.0);
        let mut e = Editor::new(View::Full, None);
        e.page = Page::Detail(Kind::Cps);
        let b = menu::body_of(S, 1.0);
        // Gesperrt: Der Regler reagiert nicht.
        let size = (b.x + b.w * 0.45, b.y + 2.0 * 38.0 + 16.0);
        step(&mut e, &mut c, size, true, true);
        step(&mut e, &mut c, size, false, false);
        assert_eq!((c.profile().design.scale, c.profile().modules[&Kind::Cps].place.scale), (2.0, 1.0));
        let toggle = (b.x + b.w - 10.0, b.y + 38.0 + 10.0);
        step(&mut e, &mut c, toggle, true, true);
        step(&mut e, &mut c, toggle, false, false);
        assert!(!c.profile().modules[&Kind::Cps].global);
        assert_eq!(rect(&c, Kind::Cps).w, 130.0);
        step(&mut e, &mut c, size, true, true);
        step(&mut e, &mut c, size, false, false);
        assert!(c.profile().modules[&Kind::Cps].place.scale < 1.0);
    }

    #[test]
    fn farbwaehler_oeffnet_aendert_und_schliesst() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Full, None);
        e.page = Page::Design;
        let b = menu::body_of(S, 1.0);
        let field = (b.x + b.w - 60.0, b.y + 5.0 * 38.0 + 16.0);
        step(&mut e, &mut c, field, true, true);
        step(&mut e, &mut c, field, false, false);
        let r = e.picker.as_ref().unwrap().rect(menu::rects(S, 1.0).1, 1.0);
        // Oben rechts im Feld: volle Sättigung und Helligkeit.
        let before = c.profile().design.style.background;
        let corner = (r.x + r.w - 13.0, r.y + 13.0);
        step(&mut e, &mut c, corner, true, true);
        step(&mut e, &mut c, corner, false, false);
        let now = c.profile().design.style.background;
        assert!(now != before && hud::color::to_hsv(now).2 > 0.95);
        // Eingabe als Hexzahl.
        let hex = (r.x + 40.0, r.y + 12.0 + 140.0 + 10.0 + 14.0 + 12.0 + 16.0);
        step(&mut e, &mut c, hex, true, true);
        assert!(e.is_typing());
        "12z3aBcC".chars().for_each(|ch| e.char(&mut c, ch));
        assert_eq!(c.profile().design.style.background, 0x123ABC);
        // Ein Klick daneben schließt und löst sonst nichts aus.
        let all = c.clone();
        let rail = menu::rects(S, 1.0).0;
        let done = (rail.x + 40.0, rail.y + rail.h - 50.0);
        assert_eq!(step(&mut e, &mut c, done, true, true), Action::None);
        assert!(e.picker.is_none() && c == all);
    }

    #[test]
    fn name_wird_nie_leer() {
        let mut c = Config::default();
        let mut e = Editor::new(View::Full, None);
        e.typing = Some(Field::Name);
        for _ in 0..20 {
            e.char(&mut c, '\u{8}');
        }
        e.char(&mut c, '\r');
        assert_eq!(c.profile().name, "Profil");
    }
}
