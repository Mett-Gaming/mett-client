//! Eigenes Crosshair: Formen aus Strecken, mit Drehung, Umriss, Punkt und Regenbogen.
use crate::color::from_hsv;
use crate::{Draw, Rect, rgb};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    #[default]
    Cross,
    /// Kreuz ohne oberen Arm.
    T,
    Circle,
    Square,
    Arrow,
    /// Nur der Punkt.
    Dot,
}

impl Shape {
    pub const ALL: [Shape; 6] = [Shape::Cross, Shape::T, Shape::Circle, Shape::Square, Shape::Arrow, Shape::Dot];

    pub fn name(self) -> &'static str {
        match self {
            Shape::Cross => "Kreuz",
            Shape::T => "T-Form",
            Shape::Circle => "Kreis",
            Shape::Square => "Quadrat",
            Shape::Arrow => "Pfeil",
            Shape::Dot => "Punkt",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crosshair {
    pub shape: Shape,
    /// Waagrechte und senkrechte Ausdehnung.
    pub length: f32,
    pub height: f32,
    pub gap: f32,
    pub thickness: f32,
    /// Grad.
    pub rotation: f32,
    pub dot: bool,
    pub dot_size: f32,
    pub dot_color: u32,
    /// 0 ist aus.
    pub outline: f32,
    pub outline_color: u32,
    pub color: u32,
    pub alpha: f32,
    pub rainbow: bool,
    /// Kehrt um, was darunter liegt, statt einer festen Farbe.
    pub invert: bool,
}

impl Default for Crosshair {
    fn default() -> Self {
        Self {
            shape: Shape::Cross,
            length: 6.0,
            height: 6.0,
            gap: 3.0,
            thickness: 2.0,
            rotation: 0.0,
            dot: false,
            dot_size: 2.0,
            dot_color: 0xE8E0D1,
            outline: 0.0,
            outline_color: 0x000000,
            color: 0xE8E0D1,
            alpha: 0.9,
            rainbow: false,
            invert: false,
        }
    }
}

/// Runden des Regenbogens je Sekunde.
const SPEED: f32 = 0.2;

type Point = (f32, f32);

/// Strecken um den Nullpunkt, ungedreht. `t` ist die Dicke.
fn lines(c: &Crosshair, (w, h, gap, t): (f32, f32, f32, f32)) -> Vec<(Point, Point)> {
    let in_ = t / 2.0 + gap;
    let left = ((-in_ - w, 0.0), (-in_, 0.0));
    let right = ((in_, 0.0), (in_ + w, 0.0));
    let up = ((0.0, -in_ - h), (0.0, -in_));
    let down = ((0.0, in_), (0.0, in_ + h));
    match c.shape {
        Shape::Cross => vec![left, right, up, down],
        Shape::T => vec![left, right, down],
        Shape::Square => {
            // Die Ecken schließen bündig.
            let (x, y, e) = (w + gap, h + gap, t / 2.0);
            vec![
                ((-x - e, -y), (x + e, -y)),
                ((-x - e, y), (x + e, y)),
                ((-x, -y + e), (-x, y - e)),
                ((x, -y + e), (x, y - e)),
            ]
        }
        Shape::Arrow => vec![((-w, gap + h), (0.0, gap)), ((0.0, gap), (w, gap + h))],
        Shape::Circle | Shape::Dot => vec![],
    }
}

/// `center` ist die Mitte, `time` in Sekunden treibt den Regenbogen.
pub fn draw(center: Point, c: &Crosshair, dpi: f32, time: f32, out: &mut Vec<Draw>) {
    // Ganze Pixel, sonst wirken dünne Linien verwaschen.
    let px = |v: f32| (v * dpi).round();
    let t = px(c.thickness).max(1.0);
    let mid = |v: f32| (v - t / 2.0).floor() + t / 2.0;
    let (x0, y0) = (mid(center.0), mid(center.1));
    // Umgekehrt zeichnet Weiß; die Deckkraft bestimmt, wie stark.
    let c = &if c.invert {
        Crosshair { color: 0xFFFFFF, dot_color: 0xFFFFFF, rainbow: false, outline: 0.0, ..*c }
    } else {
        *c
    };
    if c.invert {
        out.push(Draw::Invert(true));
    }
    let color = if c.rainbow { from_hsv(time * SPEED, 1.0, 1.0) } else { c.color };
    let border = px(c.outline).max(0.0);
    let turn = c.rotation.rem_euclid(360.0);
    let (sin, cos) = turn.to_radians().sin_cos();
    let spin = |p: Point| (x0 + p.0 * cos - p.1 * sin, y0 + p.0 * sin + p.1 * cos);
    let all = lines(c, (px(c.length), px(c.height), px(c.gap), t));
    let line = |(a, b): (Point, Point), grow: f32, color, out: &mut Vec<Draw>| {
        let straight = turn == 0.0 && (a.0 == b.0 || a.1 == b.1);
        if straight {
            let (w, h) = if a.1 == b.1 { (b.0 - a.0, t) } else { (t, b.1 - a.1) };
            let (x, y) = if a.1 == b.1 { (a.0, a.1 - t / 2.0) } else { (a.0 - t / 2.0, a.1) };
            let rect = Rect { x: x0 + x - grow, y: y0 + y - grow, w: w + 2.0 * grow, h: h + 2.0 * grow };
            out.push(Draw::Box { rect, radius: 0.0, color });
        } else {
            let len = (b.0 - a.0).hypot(b.1 - a.1).max(0.01);
            let (dx, dy) = ((b.0 - a.0) / len * grow, (b.1 - a.1) / len * grow);
            let (from, to) = (spin((a.0 - dx, a.1 - dy)), spin((b.0 + dx, b.1 + dy)));
            out.push(Draw::Line { from, to, width: t + 2.0 * grow, color });
        }
    };
    let ring = |grow: f32, color, out: &mut Vec<Draw>| {
        let (rx, ry) = (px(c.length) + px(c.gap) + t / 2.0 + grow, px(c.height) + px(c.gap) + t / 2.0 + grow);
        // Ein Kreis hat nur einen Halbmesser.
        let r = rx.min(ry);
        let rect = Rect { x: x0 - r, y: y0 - r, w: 2.0 * r, h: 2.0 * r };
        out.push(Draw::Outline { rect, radius: r, width: t + 2.0 * grow, color });
    };
    let dotted = c.dot || c.shape == Shape::Dot;
    let dot = |grow: f32, color, out: &mut Vec<Draw>| {
        let d = px(c.dot_size).max(1.0);
        let at = |v: f32| (v - d / 2.0).round();
        let rect = Rect { x: at(x0) - grow, y: at(y0) - grow, w: d + 2.0 * grow, h: d + 2.0 * grow };
        out.push(Draw::Box { rect, radius: 0.0, color });
    };
    // Erst alle Umrisse, damit keiner über einer Linie liegt.
    let mut passes = vec![(0.0, color, if c.rainbow { color } else { c.dot_color })];
    if border > 0.0 {
        passes.insert(0, (border, c.outline_color, c.outline_color));
    }
    for (grow, color, dot_color) in passes {
        let paint = rgb(color, c.alpha);
        all.iter().for_each(|l| line(*l, grow, paint, out));
        if c.shape == Shape::Circle {
            ring(grow, paint, out);
        }
        if dotted {
            dot(grow, rgb(dot_color, c.alpha), out);
        }
    }
    if c.invert {
        out.push(Draw::Invert(false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(c: &Crosshair) -> Vec<Draw> {
        let mut v = vec![];
        draw((400.5, 300.0), c, 1.0, 0.0, &mut v);
        v
    }

    fn boxes(v: &[Draw]) -> Vec<Rect> {
        v.iter().filter_map(|d| if let Draw::Box { rect, .. } = d { Some(*rect) } else { None }).collect()
    }

    #[test]
    fn kreuz_liegt_auf_ganzen_pixeln_und_symmetrisch() {
        for thickness in [1.0, 2.0, 3.0] {
            let c = Crosshair { thickness, ..Crosshair::default() };
            let b = boxes(&run(&c));
            assert_eq!(b.len(), 4);
            for r in &b {
                for v in [r.x, r.y, r.w, r.h] {
                    assert_eq!(v, v.round(), "{r:?}");
                }
            }
            let (left, right) = (b[0], b[1]);
            let (up, down) = (b[2], b[3]);
            assert_eq!((left.x + left.w + right.x) / 2.0, up.x + up.w / 2.0);
            assert_eq!((up.y + up.h + down.y) / 2.0, left.y + left.h / 2.0);
        }
    }

    #[test]
    fn jede_form_zeichnet_etwas_und_umriss_verdoppelt() {
        for shape in Shape::ALL {
            let c = Crosshair { shape, ..Crosshair::default() };
            let plain = run(&c).len();
            assert!(plain > 0, "{shape:?}");
            let more = run(&Crosshair { outline: 1.0, ..c });
            assert_eq!(more.len(), 2 * plain, "{shape:?}");
        }
    }

    #[test]
    fn gedreht_werden_linien_gezeichnet() {
        let v = run(&Crosshair { rotation: 45.0, ..Crosshair::default() });
        assert!(v.iter().all(|d| matches!(d, Draw::Line { .. })) && v.len() == 4);
        // Eine volle Drehung ist keine.
        assert_eq!(run(&Crosshair { rotation: 360.0, ..Crosshair::default() }), run(&Crosshair::default()));
    }

    #[test]
    fn umgekehrt_steht_zwischen_klammern() {
        let v = run(&Crosshair { invert: true, rainbow: true, outline: 2.0, color: 0xFF0000, ..Crosshair::default() });
        assert!(v.first() == Some(&Draw::Invert(true)) && v.last() == Some(&Draw::Invert(false)));
        // Keine Farben, kein Umriss.
        let white = v.iter().all(|d| !matches!(d, Draw::Box { color, .. } if *color != rgb(0xFFFFFF, 0.9)));
        assert!(white && v.len() == 6);
    }

    #[test]
    fn regenbogen_wandert() {
        let c = Crosshair { rainbow: true, ..Crosshair::default() };
        let at = |t| {
            let mut v = vec![];
            draw((10.0, 10.0), &c, 1.0, t, &mut v);
            v
        };
        assert_ne!(at(0.0), at(1.0));
        assert_eq!(at(0.0), at(5.0));
    }

    #[test]
    fn alte_einstellungen_bleiben_lesbar() {
        let t = r#"{"length":9.0,"gap":2.0,"thickness":1.0,"dot":true,"color":255,"alpha":0.5}"#;
        let c: Crosshair = serde_json::from_str(t).unwrap();
        assert!(c.length == 9.0 && c.dot && c.color == 255 && c.shape == Shape::Cross && c.outline == 0.0);
    }
}
