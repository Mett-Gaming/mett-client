//! Reine HUD-Logik: Anker, Layout und Zeichenbefehle. Kein Windows, voll testbar.
use serde::{Deserialize, Serialize};

pub mod appleskin;
pub mod chat_input;
pub mod editor;
pub mod hints;
pub mod java_chat;
pub mod symbols;
pub mod tab_list;
pub mod ui;
pub mod wire;
pub mod zoom;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.x + self.w && p.1 >= self.y && p.1 < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    fn factors(self) -> (f32, f32) {
        use Anchor::*;
        let x = match self {
            TopLeft | Left | BottomLeft => 0.0,
            Top | Center | Bottom => 0.5,
            _ => 1.0,
        };
        let y = match self {
            TopLeft | Top | TopRight => 0.0,
            Left | Center | Right => 0.5,
            _ => 1.0,
        };
        (x, y)
    }
}

/// Lage eines Widgets; der Versatz zählt vom Ankerpunkt aus nach rechts/unten.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub anchor: Anchor,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    /// Zusätzlich gezogene Breite und Höhe, 1 ist die übliche.
    #[serde(default = "one")]
    pub w: f32,
    #[serde(default = "one")]
    pub h: f32,
}

fn one() -> f32 {
    1.0
}

/// Unsinnige Werte aus einer Datei gelten als üblich.
fn stretch(v: f32) -> f32 {
    if v.is_finite() && v > 0.0 { v.clamp(0.01, 100.0) } else { 1.0 }
}

impl Placement {
    /// Rechnet in Pixel um und hält das Widget im Fenster.
    pub fn resolve(&self, screen: (f32, f32), size: (f32, f32), dpi: f32) -> Rect {
        let s = self.scale.clamp(0.25, 4.0) * dpi;
        let (w, h) = (size.0 * s * stretch(self.w), size.1 * s * stretch(self.h));
        let (fx, fy) = self.anchor.factors();
        let x = (screen.0 - w) * fx + self.x * dpi;
        let y = (screen.1 - h) * fy + self.y * dpi;
        Rect { x: x.clamp(0.0, (screen.0 - w).max(0.0)), y: y.clamp(0.0, (screen.1 - h).max(0.0)), w, h }
    }
}

pub type Color = [f32; 4];

pub fn rgb(hex: u32, alpha: f32) -> Color {
    let c = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
    [c(16), c(8), c(0), alpha]
}

#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    Box {
        rect: Rect,
        radius: f32,
        color: Color,
    },
    Outline {
        rect: Rect,
        radius: f32,
        width: f32,
        color: Color,
    },
    Text {
        rect: Rect,
        size: f32,
        color: Color,
        align: Align,
        text: String,
    },
    /// Ausschnitt aus den Symbolen von AppleSkin, `from` in Bildpunkten.
    Icon {
        rect: Rect,
        from: Rect,
        alpha: f32,
    },
    /// Minecraft-Schrift; `unit` ist die Größe eines Schriftpixels.
    Game {
        x: f32,
        y: f32,
        unit: f32,
        color: Color,
        text: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Design-Tokens des Mett Launchers.
pub mod theme {
    pub const APP: u32 = 0x120E0B;
    pub const SURFACE: u32 = 0x1B1611;
    pub const RAISED: u32 = 0x241D17;
    pub const HOVER: u32 = 0x2F261E;
    pub const BORDER: u32 = 0x54402E;
    pub const TEXT: u32 = 0xE8E0D1;
    pub const MUTED: u32 = 0xAA9074;
    pub const ACCENT: u32 = 0x7E8758;
    pub const ACCENT_HOVER: u32 = 0x9AA36E;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Keystrokes,
    Cps,
    Clock,
    Fps,
    System,
    Battery,
    Music,
    Ping,
    Session,
    Stopwatch,
    Note,
    Chat,
    Zoom,
    Players,
    AppleSkin,
}

/// Übliche Größe eines Textfelds; alle sind gleich hoch.
const LABEL: (f32, f32) = (130.0, 30.0);

impl Kind {
    pub const ALL: [Kind; 15] = [
        Kind::Keystrokes,
        Kind::Cps,
        Kind::Clock,
        Kind::Fps,
        Kind::System,
        Kind::Battery,
        Kind::Music,
        Kind::Ping,
        Kind::Session,
        Kind::Stopwatch,
        Kind::Note,
        Kind::Chat,
        Kind::Zoom,
        Kind::Players,
        Kind::AppleSkin,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Keystrokes => "Tasten",
            Kind::Cps => "CPS",
            Kind::Clock => "Uhr",
            Kind::Fps => "FPS",
            Kind::System => "System",
            Kind::Battery => "Akku",
            Kind::Music => "Musik",
            Kind::Ping => "Ping",
            Kind::Session => "Spielzeit",
            Kind::Stopwatch => "Stoppuhr",
            Kind::Note => "Notiz",
            Kind::Chat => "Chat",
            Kind::Zoom => "Zoom",
            Kind::Players => "Spielerliste",
            Kind::AppleSkin => "AppleSkin",
        }
    }

    /// Hat einen Platz auf dem Bildschirm, den man verschieben kann.
    pub fn placed(self) -> bool {
        !matches!(self, Kind::Zoom | Kind::Players | Kind::AppleSkin)
    }

    /// Bildschirmpixel je Maßeinheit: der Chat rechnet in Java-Pixeln, alles andere in Windows-Skalierung.
    pub fn unit(self, screen: (f32, f32), dpi: f32) -> f32 {
        if self == Kind::Chat { java_chat::unit(screen) } else { dpi }
    }

    /// Ein Feld mit einer Zeile Text.
    pub fn label(self) -> bool {
        self.placed() && !matches!(self, Kind::Keystrokes | Kind::Chat)
    }

    pub fn size(self) -> (f32, f32) {
        match self {
            Kind::Keystrokes => (124.0, 162.0),
            Kind::Chat => (java_chat::FULL_WIDTH, java_chat::LINE * java_chat::ROWS as f32),
            // Längere Texte brauchen mehr Platz.
            Kind::System | Kind::Music | Kind::Note => (230.0, LABEL.1),
            _ => LABEL,
        }
    }
}

/// Aussehen eines Moduls.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub background: u32,
    pub opacity: f32,
    pub text: u32,
    pub accent: u32,
    pub radius: f32,
    pub outline: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            background: theme::SURFACE,
            opacity: 0.72,
            text: theme::TEXT,
            accent: theme::ACCENT,
            radius: 6.0,
            outline: false,
        }
    }
}

impl Style {
    /// Feld mit Text; `down` zeigt die Druckfarbe.
    fn field(&self, rect: Rect, s: f32, text: &str, down: bool, out: &mut Vec<Draw>) {
        let color = if down { rgb(self.accent, 0.9) } else { rgb(self.background, self.opacity) };
        let radius = self.radius * s;
        out.push(Draw::Box { rect, radius, color });
        if self.outline {
            out.push(Draw::Outline { rect, radius, width: s, color: rgb(self.accent, 1.0) });
        }
        out.push(Draw::Text {
            rect,
            size: 14.0 * s,
            color: rgb(self.text, 1.0),
            align: Align::Center,
            text: text.into(),
        });
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keys {
    pub w: bool,
    pub a: bool,
    pub s: bool,
    pub d: bool,
    pub space: bool,
    pub lmb: bool,
    pub rmb: bool,
}

pub fn keystrokes(at: Rect, style: &Style, keys: Keys, out: &mut Vec<Draw>) {
    let size = Kind::Keystrokes.size();
    let (sx, sy) = (at.w / size.0, at.h / size.1);
    let s = sx.min(sy);
    let mut key = |x: f32, y: f32, w: f32, h: f32, label: &str, down: bool| {
        let rect = Rect { x: at.x + x * sx, y: at.y + y * sy, w: w * sx, h: h * sy };
        style.field(rect, s, label, down, out);
    };
    key(42.0, 0.0, 40.0, 40.0, "W", keys.w);
    key(0.0, 42.0, 40.0, 40.0, "A", keys.a);
    key(42.0, 42.0, 40.0, 40.0, "S", keys.s);
    key(84.0, 42.0, 40.0, 40.0, "D", keys.d);
    key(0.0, 84.0, 61.0, 40.0, "LMT", keys.lmb);
    key(63.0, 84.0, 61.0, 40.0, "RMT", keys.rmb);
    key(0.0, 126.0, 124.0, 36.0, "Leertaste", keys.space);
}

/// Die Schrift richtet sich nach der knapperen Seite.
pub fn label(at: Rect, kind: Kind, style: &Style, text: &str, out: &mut Vec<Draw>) {
    let size = kind.size();
    style.field(at, (at.w / size.0).min(at.h / size.1), text, false, out);
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crosshair {
    pub length: f32,
    pub gap: f32,
    pub thickness: f32,
    pub dot: bool,
    pub color: u32,
    pub alpha: f32,
}

pub fn crosshair(screen: (f32, f32), c: &Crosshair, dpi: f32, out: &mut Vec<Draw>) {
    // Ganze Pixel, sonst wirken dünne Linien verwaschen.
    let len = (c.length * dpi).round();
    let gap = (c.gap * dpi).round();
    let t = (c.thickness * dpi).round().max(1.0);
    let x0 = ((screen.0 - t) / 2.0).floor();
    let y0 = ((screen.1 - t) / 2.0).floor();
    let mut bar =
        |x, y, w, h| out.push(Draw::Box { rect: Rect { x, y, w, h }, radius: 0.0, color: rgb(c.color, c.alpha) });
    bar(x0 - gap - len, y0, len, t);
    bar(x0 + t + gap, y0, len, t);
    bar(x0, y0 - gap - len, t, len);
    bar(x0, y0 + t + gap, t, len);
    if c.dot {
        bar(x0, y0, t, t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Placement = Placement { anchor: Anchor::TopLeft, x: 10.0, y: 20.0, scale: 1.0, w: 1.0, h: 1.0 };

    #[test]
    fn anker_rechnet_ab_der_ecke() {
        let size = (100.0, 50.0);
        assert_eq!(P.resolve((800.0, 600.0), size, 1.0), Rect { x: 10.0, y: 20.0, w: 100.0, h: 50.0 });
        let p = Placement { anchor: Anchor::BottomRight, x: -10.0, y: -20.0, ..P };
        assert_eq!(p.resolve((800.0, 600.0), size, 1.0), Rect { x: 690.0, y: 530.0, w: 100.0, h: 50.0 });
    }

    #[test]
    fn textfelder_sind_gleich_gross() {
        for k in Kind::ALL.into_iter().filter(|k| k.label()) {
            let (w, h) = k.size();
            assert!(h == LABEL.1 && (w == LABEL.0 || (w > LABEL.0 && w < 2.0 * LABEL.0)), "{k:?}");
        }
        assert!(!Kind::Keystrokes.label() && !Kind::Chat.label() && !Kind::Zoom.label());
    }

    #[test]
    fn breite_und_hoehe_sind_unabhaengig() {
        let p = Placement { w: 1.37, h: 0.5, ..P };
        let r = p.resolve((800.0, 600.0), (100.0, 50.0), 1.0);
        assert!((r.w - 137.0).abs() < 0.001 && r.h == 25.0);
        for bad in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
            assert_eq!(Placement { w: bad, h: bad, ..P }.resolve((800.0, 600.0), (100.0, 50.0), 1.0).w, 100.0);
        }
    }

    #[test]
    fn tasten_fuellen_jede_groesse() {
        let at = Rect { x: 10.0, y: 20.0, w: 248.0, h: 81.0 };
        let mut out = vec![];
        keystrokes(at, &Style::default(), Keys::default(), &mut out);
        let boxes = out.iter().filter_map(|d| if let Draw::Box { rect, .. } = d { Some(*rect) } else { None });
        let (mut right, mut bottom) = (0f32, 0f32);
        for b in boxes {
            right = right.max(b.x + b.w);
            bottom = bottom.max(b.y + b.h);
        }
        assert!((right - 258.0).abs() < 0.01 && (bottom - 101.0).abs() < 0.01);
        // Schrift nach der knapperen Seite.
        assert!(out.iter().all(|d| !matches!(d, Draw::Text { size, .. } if (*size - 7.0).abs() > 0.01)));
    }

    #[test]
    fn dpi_skaliert_groesse_und_versatz() {
        assert_eq!(P.resolve((800.0, 600.0), (100.0, 50.0), 2.0), Rect { x: 20.0, y: 40.0, w: 200.0, h: 100.0 });
    }

    #[test]
    fn bleibt_im_fenster_auch_wenn_es_zu_klein_ist() {
        let p = Placement { x: 9999.0, ..P };
        assert_eq!(p.resolve((800.0, 600.0), (100.0, 50.0), 1.0).x, 700.0);
        assert_eq!(p.resolve((50.0, 20.0), (100.0, 50.0), 1.0).x, 0.0);
    }

    #[test]
    fn crosshair_ist_symmetrisch() {
        for thickness in [1.0, 2.0, 3.0] {
            let c = Crosshair { length: 6.0, gap: 3.0, thickness, dot: true, color: 0xffffff, alpha: 1.0 };
            let mut v = vec![];
            crosshair((801.0, 600.0), &c, 1.0, &mut v);
            let r: Vec<Rect> = v
                .iter()
                .filter_map(|d| match d {
                    Draw::Box { rect, .. } => Some(*rect),
                    _ => None,
                })
                .collect();
            assert_eq!(r.len(), 5);
            assert_eq!(r[4].x - (r[0].x + r[0].w), r[1].x - (r[4].x + r[4].w));
            assert_eq!(r[4].y - (r[2].y + r[2].h), r[3].y - (r[4].y + r[4].h));
        }
    }
}
