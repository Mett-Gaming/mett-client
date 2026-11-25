//! Kompassleiste: Himmelsrichtungen, Striche je 15 Grad und die Gradzahl.
use crate::lang::t;
use crate::{Align, Draw, Rect, Style, rgb};

pub const SIZE: (f32, f32) = (290.0, 60.0);
/// So viel Grad sind nach jeder Seite zu sehen.
const HALF: f32 = 65.0;
const STEP: usize = 15;
const POINTS: [&str; 8] = ["N", "NO", "O", "SO", "S", "SW", "W", "NW"];

/// Kürzester Weg von `from` nach `to`, mit Vorzeichen.
fn offset(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

pub fn draw(at: Rect, style: &Style, degrees: f32, out: &mut Vec<Draw>) {
    let s = (at.w / SIZE.0).min(at.h / SIZE.1);
    out.push(Draw::Box { rect: at, radius: style.radius * s, color: rgb(style.background, style.opacity) });
    if style.outline {
        out.push(Draw::Outline { rect: at, radius: style.radius * s, width: s, color: rgb(style.accent, 1.0) });
    }
    let (middle, room) = (at.x + at.w / 2.0, at.w / 2.0 - 14.0 * s);
    let y = at.y + (at.h - SIZE.1 * s) / 2.0;
    for mark in (0..360).step_by(STEP) {
        let d = offset(degrees, mark as f32);
        if d.abs() > HALF {
            continue;
        }
        let x = middle + d / HALF * room;
        // Zum Rand hin blasser.
        let alpha = (1.0 - d.abs() / HALF).sqrt();
        let point = (mark % 45 == 0).then(|| POINTS[mark / 45]);
        let (color, long) = if point.is_some() { (style.accent, 8.0) } else { (style.text, 5.0) };
        out.push(Draw::Line {
            from: (x, y + 7.0 * s),
            to: (x, y + (7.0 + long) * s),
            width: 1.5 * s,
            color: rgb(color, alpha),
        });
        if let Some(p) = point {
            // Haupthimmelsrichtungen größer.
            let size = if mark % 90 == 0 { 16.0 } else { 12.0 };
            let rect = Rect { x: x - 20.0 * s, y: y + 15.0 * s, w: 40.0 * s, h: 20.0 * s };
            let color = rgb(style.text, alpha);
            out.push(Draw::Text { rect, size: size * s, color, align: Align::Center, text: t(p).into() });
        }
    }
    // Zeiger in der Mitte.
    let pointer = rgb(style.accent, 1.0);
    for side in [-4.0, 4.0] {
        let from = (middle + side * s, y + 1.0 * s);
        out.push(Draw::Line { from, to: (middle, y + 6.0 * s), width: 1.5 * s, color: pointer });
    }
    let text = format!("{}°", (degrees.round() as i32).rem_euclid(360));
    let rect = Rect { x: middle - 40.0 * s, y: y + 37.0 * s, w: 80.0 * s, h: 18.0 * s };
    out.push(Draw::Text { rect, size: 13.0 * s, color: rgb(style.text, 0.85), align: Align::Center, text });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(degrees: f32) -> Vec<(String, f32)> {
        let mut out = vec![];
        let at = Rect { x: 100.0, y: 50.0, w: SIZE.0, h: SIZE.1 };
        draw(at, &Style::default(), degrees, &mut out);
        for d in &out {
            match d {
                Draw::Line { from, to, .. } => {
                    for p in [from, to] {
                        assert!(p.0 >= at.x && p.0 <= at.x + at.w && p.1 >= at.y && p.1 <= at.y + at.h);
                    }
                }
                Draw::Text { rect, .. } => assert!(rect.y >= at.y && rect.y + rect.h <= at.y + at.h),
                _ => {}
            }
        }
        let text = |d: &Draw| match d {
            Draw::Text { rect, text, .. } => Some((text.clone(), rect.x + rect.w / 2.0 - at.x - at.w / 2.0)),
            _ => None,
        };
        out.iter().filter_map(text).collect()
    }

    #[test]
    fn richtung_steht_in_der_mitte() {
        let s = texts(180.0);
        assert!(s.iter().any(|(t, x)| t == "S" && x.abs() < 0.01));
        // Nach Osten geht es nach links, nach Westen nach rechts.
        assert!(s.iter().any(|(t, x)| t == "SO" && *x < -50.0) && s.iter().any(|(t, x)| t == "SW" && *x > 50.0));
        assert!(s.iter().any(|(t, _)| t == "180°") && !s.iter().any(|(t, _)| t == "N"));
    }

    #[test]
    fn ueber_norden_hinweg() {
        for (d, text) in [(359.7, "0°"), (0.0, "0°"), (2.0, "2°")] {
            let s = texts(d);
            assert!(s.iter().any(|(t, x)| t == "N" && x.abs() < 10.0) && s.iter().any(|(t, _)| t == text), "{d}");
        }
        assert_eq!((offset(350.0, 10.0), offset(10.0, 350.0)), (20.0, -20.0));
    }
}
