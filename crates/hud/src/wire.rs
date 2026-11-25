//! Zeichenbefehle als Bytes, für den Weg vom Client ins Spiel. Beim Lesen wird alles geprüft, Unsinn wird `None`.
use crate::{Align, Color, Draw, Rect};

/// Mehr Befehle oder längere Texte gibt es nicht.
const MAX_ITEMS: usize = 10_000;
const MAX_TEXT: usize = 4096;

pub fn encode(items: &[Draw]) -> Vec<u8> {
    let mut o = vec![];
    let f = |o: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| o.extend(x.to_le_bytes()));
    let t = |o: &mut Vec<u8>, s: &str| {
        // An Zeichengrenzen kürzen.
        let mut n = s.len().min(MAX_TEXT);
        while !s.is_char_boundary(n) {
            n -= 1;
        }
        o.extend((n as u32).to_le_bytes());
        o.extend(&s.as_bytes()[..n]);
    };
    let n = items.len().min(MAX_ITEMS);
    o.extend((n as u32).to_le_bytes());
    for d in &items[..n] {
        match d {
            Draw::Box { rect: r, radius, color: c } => {
                o.push(0);
                f(&mut o, &[r.x, r.y, r.w, r.h, *radius, c[0], c[1], c[2], c[3]]);
            }
            Draw::Outline { rect: r, radius, width, color: c } => {
                o.push(1);
                f(&mut o, &[r.x, r.y, r.w, r.h, *radius, *width, c[0], c[1], c[2], c[3]]);
            }
            Draw::Text { rect: r, size, color: c, align, text } => {
                o.push(2);
                f(&mut o, &[r.x, r.y, r.w, r.h, *size, c[0], c[1], c[2], c[3]]);
                o.push(match align {
                    Align::Left => 0,
                    Align::Center => 1,
                    Align::Right => 2,
                });
                t(&mut o, text);
            }
            Draw::Icon { rect: r, from: s, alpha } => {
                o.push(4);
                f(&mut o, &[r.x, r.y, r.w, r.h, s.x, s.y, s.w, s.h, *alpha]);
            }
            Draw::Game { x, y, unit, color: c, text } => {
                o.push(3);
                f(&mut o, &[*x, *y, *unit, c[0], c[1], c[2], c[3]]);
                t(&mut o, text);
            }
        }
    }
    o
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (a, b) = self.0.split_at_checked(n)?;
        self.0 = b;
        Some(a)
    }

    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    /// Nur endliche, vernünftig große Zahlen; die Grafik soll nie Unsinn bekommen.
    fn f32(&mut self) -> Option<f32> {
        let v = f32::from_le_bytes(self.take(4)?.try_into().ok()?);
        (v.is_finite() && v.abs() <= 1e6).then_some(v)
    }

    fn rect(&mut self) -> Option<Rect> {
        Some(Rect { x: self.f32()?, y: self.f32()?, w: self.f32()?, h: self.f32()? })
    }

    fn color(&mut self) -> Option<Color> {
        Some([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }

    fn text(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        if n > MAX_TEXT {
            return None;
        }
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

pub fn decode(bytes: &[u8]) -> Option<Vec<Draw>> {
    let mut r = Reader(bytes);
    let n = r.u32()? as usize;
    if n > MAX_ITEMS {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(match r.byte()? {
            0 => Draw::Box { rect: r.rect()?, radius: r.f32()?, color: r.color()? },
            1 => Draw::Outline { rect: r.rect()?, radius: r.f32()?, width: r.f32()?, color: r.color()? },
            2 => Draw::Text {
                rect: r.rect()?,
                size: r.f32().filter(|s| *s > 0.0)?,
                color: r.color()?,
                align: match r.byte()? {
                    0 => Align::Left,
                    1 => Align::Center,
                    2 => Align::Right,
                    _ => return None,
                },
                text: r.text()?,
            },
            3 => Draw::Game { x: r.f32()?, y: r.f32()?, unit: r.f32()?, color: r.color()?, text: r.text()? },
            4 => Draw::Icon { rect: r.rect()?, from: r.rect()?, alpha: r.f32()? },
            _ => return None,
        });
    }
    r.0.is_empty().then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Draw> {
        let rect = Rect { x: 1.0, y: 2.5, w: 300.0, h: 4.0 };
        let color = [0.1, 0.2, 0.3, 1.0];
        vec![
            Draw::Box { rect, radius: 6.0, color },
            Draw::Outline { rect, radius: 0.0, width: 2.0, color },
            Draw::Text { rect, size: 14.0, color, align: Align::Right, text: "Grüße 😀".into() },
            Draw::Icon { rect, from: Rect { x: 27.0, y: 0.0, w: 9.0, h: 9.0 }, alpha: 0.75 },
            Draw::Game { x: -3.0, y: 9.0, unit: 3.0, color, text: String::new() },
        ]
    }

    #[test]
    fn hin_und_zurueck() {
        assert_eq!(decode(&encode(&all())), Some(all()));
        assert_eq!(decode(&encode(&[])), Some(vec![]));
    }

    #[test]
    fn jeder_abgeschnittene_stand_wird_abgelehnt() {
        let b = encode(&all());
        for n in 0..b.len() {
            assert_eq!(decode(&b[..n]), None, "{n}");
        }
        let mut more = b.clone();
        more.push(0);
        assert_eq!(decode(&more), None);
    }

    #[test]
    fn beliebige_bytes_stuerzen_nicht_ab() {
        let mut seed = 1u32;
        for n in 0..2000usize {
            let junk: Vec<u8> = (0..n % 200)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 24) as u8
                })
                .collect();
            let _ = decode(&junk);
            // Gültiger Anfang mit gestörtem Byte.
            let mut b = encode(&all());
            let at = n % b.len();
            b[at] ^= (seed >> 16) as u8;
            let _ = decode(&b);
        }
        let nan = [Draw::Game { x: f32::NAN, y: 0.0, unit: 1.0, color: [0.0; 4], text: "x".into() }];
        assert_eq!(decode(&encode(&nan)), None);
    }

    #[test]
    fn lange_texte_werden_an_zeichengrenzen_gekuerzt() {
        let d = [Draw::Game { x: 0.0, y: 0.0, unit: 1.0, color: [0.0; 4], text: format!("a{}", "ä".repeat(4000)) }];
        let Some(Draw::Game { text, .. }) = decode(&encode(&d)).and_then(|v| v.into_iter().next()) else { panic!() };
        assert_eq!(text.len(), MAX_TEXT - 1);
    }
}
