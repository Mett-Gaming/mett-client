//! Farbwähler: Feld für Sättigung und Helligkeit, Balken für den Farbton, Eingabe als Hexzahl.
use config::Config;
use hud::color::{from_hsv, to_hsv};
use hud::ui::Ui;
use hud::{Draw, Kind, Rect, rgb};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    Background,
    Text,
    Accent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuPart {
    Background,
    Card,
    Border,
    Text,
    Muted,
    Accent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CrossPart {
    Lines,
    Dot,
    Outline,
}

/// Welche Farbe geändert wird.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    Design(Part),
    Module(Kind, Part),
    Menu(MenuPart),
    Crosshair(CrossPart),
}

impl Paint {
    pub fn of(self, c: &mut Config) -> Option<&mut u32> {
        fn part(s: &mut hud::Style, p: Part) -> &mut u32 {
            match p {
                Part::Background => &mut s.background,
                Part::Text => &mut s.text,
                Part::Accent => &mut s.accent,
            }
        }
        Some(match self {
            Paint::Design(p) => part(&mut c.profile_mut().design.style, p),
            Paint::Module(k, p) => part(&mut c.profile_mut().modules.get_mut(&k)?.style, p),
            Paint::Crosshair(p) => {
                let x = &mut c.profile_mut().crosshair;
                match p {
                    CrossPart::Lines => &mut x.color,
                    CrossPart::Dot => &mut x.dot_color,
                    CrossPart::Outline => &mut x.outline_color,
                }
            }
            Paint::Menu(p) => {
                let m = &mut c.menu;
                match p {
                    MenuPart::Background => &mut m.background,
                    MenuPart::Card => &mut m.card,
                    MenuPart::Border => &mut m.border,
                    MenuPart::Text => &mut m.text,
                    MenuPart::Muted => &mut m.muted,
                    MenuPart::Accent => &mut m.accent,
                }
            }
        })
    }
}

pub struct Picker {
    paint: Paint,
    /// Das Farbfeld, an dem er hängt.
    anchor: Rect,
    /// Farbton und Sättigung gehen bei Schwarz und Grau sonst verloren.
    hsv: (f32, f32, f32),
    /// Was gerade getippt wird.
    pub typed: Option<String>,
}

const W: f32 = 236.0;
const H: f32 = 232.0;
/// Eigene Nummern, sonst mischen sich Regler der Seite ein.
const AREA: u32 = 9001;
const HUE: u32 = 9002;

impl Picker {
    pub fn new(paint: Paint, color: u32, anchor: Rect) -> Self {
        Self { paint, anchor, hsv: to_hsv(color), typed: None }
    }

    pub fn rect(&self, main: Rect, s: f32) -> Rect {
        let (w, h) = (W * s, H * s);
        let x = (self.anchor.x + self.anchor.w - w).clamp(main.x, (main.x + main.w - w).max(main.x));
        let below = self.anchor.y + self.anchor.h + 4.0 * s;
        // Passt er nicht darunter, kommt er darüber.
        let y = if below + h <= main.y + main.h { below } else { self.anchor.y - h - 4.0 * s };
        Rect { x, y: y.max(main.y), w, h }
    }

    pub fn char(&mut self, ch: char, c: &mut Config) {
        let Some(t) = &mut self.typed else { return };
        match ch {
            '\u{8}' => {
                t.pop();
            }
            ch if ch.is_ascii_hexdigit() && t.len() < 6 => t.push(ch.to_ascii_uppercase()),
            _ => return,
        }
        let value = (t.len() == 6).then(|| u32::from_str_radix(t, 16).ok()).flatten();
        if let (Some(v), Some(to)) = (value, self.paint.of(c)) {
            *to = v;
            self.hsv = to_hsv(v);
        }
    }

    /// `false`: schließen.
    pub fn show(&mut self, ui: &mut Ui, c: &mut Config, main: Rect) -> bool {
        let s = ui.scale;
        let r = self.rect(main, s);
        if (ui.input.pressed || ui.input.right_pressed) && !r.contains(ui.input.mouse) {
            return false;
        }
        let Some(to) = self.paint.of(c) else { return false };
        ui.out.push(Draw::Box { rect: r, radius: 0.0, color: rgb(ui.look.background, 1.0) });
        ui.panel(r);
        let pad = 12.0 * s;
        let area = Rect { x: r.x + pad, y: r.y + pad, w: r.w - 2.0 * pad, h: 140.0 * s };
        let hue = Rect { y: area.y + area.h + 10.0 * s, h: 14.0 * s, ..area };
        let field = Rect { x: area.x, y: hue.y + hue.h + 12.0 * s, w: 120.0 * s, h: 32.0 * s };
        if let Some((x, y)) = ui.drag(AREA, area) {
            self.hsv = (self.hsv.0, x, 1.0 - y);
            (*to, self.typed) = (from_hsv(self.hsv.0, x, 1.0 - y), None);
        }
        if let Some((x, _)) = ui.drag(HUE, hue) {
            // Ganz rechts wäre wieder Rot.
            self.hsv.0 = x.min(0.999);
            (*to, self.typed) = (from_hsv(self.hsv.0, self.hsv.1, self.hsv.2), None);
        }
        let (h, sat, v) = self.hsv;
        grid(area, 53, 35, ui.out, |x, y| from_hsv(h, x, 1.0 - y));
        grid(hue, 106, 1, ui.out, |x, _| from_hsv(x * 0.999, 1.0, 1.0));
        let dot = 10.0 * s;
        let mark =
            Rect { x: area.x + area.w * sat - dot / 2.0, y: area.y + area.h * (1.0 - v) - dot / 2.0, w: dot, h: dot };
        let bar = Rect { x: hue.x + hue.w * h - 3.0 * s, y: hue.y - 2.0 * s, w: 6.0 * s, h: hue.h + 4.0 * s };
        for (rect, radius) in [(mark, dot / 2.0), (bar, 2.0 * s)] {
            ui.out.push(Draw::Outline { rect, radius, width: 3.0 * s, color: rgb(0, 0.8) });
            ui.out.push(Draw::Outline { rect, radius, width: 1.5 * s, color: rgb(0xFFFFFF, 1.0) });
        }
        let shown = match &self.typed {
            Some(t) => format!("#{t}|"),
            None => format!("#{to:06X}", to = *to),
        };
        if ui.field(field, &shown, self.typed.is_some()) {
            self.typed = if self.typed.is_some() { None } else { Some(String::new()) };
        }
        let swatch = Rect { x: field.x + field.w + 8.0 * s, w: area.w - field.w - 8.0 * s, ..field };
        ui.out.push(Draw::Box { rect: swatch, radius: ui.look.radius * s, color: rgb(*to, 1.0) });
        true
    }
}

/// Verlauf aus Feldern, auf ganze Pixel, damit keine Nähte entstehen.
fn grid(r: Rect, cols: usize, rows: usize, out: &mut Vec<Draw>, color: impl Fn(f32, f32) -> u32) {
    let edge = |from: f32, len: f32, i: usize, n: usize| (from + len * i as f32 / n as f32).round();
    let part = |i: usize, n: usize| if n < 2 { 0.0 } else { i as f32 / (n - 1) as f32 };
    for y in 0..rows {
        for x in 0..cols {
            let (left, top) = (edge(r.x, r.w, x, cols), edge(r.y, r.h, y, rows));
            let rect =
                Rect { x: left, y: top, w: edge(r.x, r.w, x + 1, cols) - left, h: edge(r.y, r.h, y + 1, rows) - top };
            out.push(Draw::Box { rect, radius: 0.0, color: rgb(color(part(x, cols), part(y, rows)), 1.0) });
        }
    }
}
