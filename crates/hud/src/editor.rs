//! Verschieben und Einrasten, ohne Windows.
use crate::{Anchor, Placement, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub vertical: bool,
    pub at: f32,
}

/// Oberstes Element unter dem Punkt.
pub fn hit<K: Copy>(items: &[(K, Rect)], p: (f32, f32)) -> Option<K> {
    items.iter().rev().find(|(_, r)| r.contains(p)).map(|(k, _)| *k)
}

/// Rastet an Rand, Mitte und anderen Modulen ein. Liefert die neue Lage und die Hilfslinien.
pub fn snap(r: Rect, others: &[Rect], screen: (f32, f32), dist: f32, margin: f32) -> (Rect, Vec<Guide>) {
    let xs: Vec<_> = others.iter().map(|o| (o.x, o.w)).collect();
    let ys: Vec<_> = others.iter().map(|o| (o.y, o.h)).collect();
    let (x, gx) = axis(r.x, r.w, screen.0, &xs, dist, margin);
    let (y, gy) = axis(r.y, r.h, screen.1, &ys, dist, margin);
    let mut guides = vec![];
    guides.extend(gx.map(|at| Guide { vertical: true, at }));
    guides.extend(gy.map(|at| Guide { vertical: false, at }));
    (Rect { x, y, ..r }, guides)
}

fn axis(p: f32, len: f32, total: f32, others: &[(f32, f32)], dist: f32, margin: f32) -> (f32, Option<f32>) {
    // (Zielposition, Hilfslinie)
    let mut c = vec![(margin, margin), (total - margin - len, total - margin), ((total - len) / 2.0, total / 2.0)];
    let gap = margin / 4.0;
    for &(o, ol) in others {
        c.push((o, o));
        c.push((o + ol - len, o + ol));
        c.push((o + (ol - len) / 2.0, o + ol / 2.0));
        c.push((o + ol + gap, o + ol));
        c.push((o - gap - len, o));
    }
    let best = c.into_iter().min_by(|a, b| (a.0 - p).abs().total_cmp(&(b.0 - p).abs()));
    match best {
        Some((to, line)) if (to - p).abs() <= dist => (to, Some(line)),
        _ => (p, None),
    }
}

/// Welche Kanten gezogen werden.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    pub fn any(self) -> bool {
        self.left || self.right || self.top || self.bottom
    }
}

/// Kante oder Ecke unter dem Punkt, wie bei einem Windows-Fenster: ein schmaler Streifen um den Rand, an den Enden etwas
/// länger für die Ecken.
pub fn edges(r: Rect, p: (f32, f32), dpi: f32) -> Edges {
    let out = 5.0 * dpi;
    let wide = Rect { x: r.x - out, y: r.y - out, w: r.w + 2.0 * out, h: r.h + 2.0 * out };
    if !wide.contains(p) {
        return Edges::default();
    }
    // Innen bleibt immer genug zum Verschieben.
    let inside = |len: f32| (4.0 * dpi).min(len / 4.0);
    let corner = |len: f32| (14.0 * dpi).min(len / 3.0);
    let (x0, x1, y0, y1) = (p.0 - r.x, r.x + r.w - p.0, p.1 - r.y, r.y + r.h - p.1);
    let (ix, iy, cx, cy) = (inside(r.w), inside(r.h), corner(r.w), corner(r.h));
    let mut e = Edges { left: x0 < ix, right: x1 < ix, top: y0 < iy, bottom: y1 < iy };
    if e.left || e.right {
        e.top |= y0 < cy;
        e.bottom |= y1 < cy;
    }
    if e.top || e.bottom {
        e.left |= x0 < cx;
        e.right |= x1 < cx;
    }
    e
}

/// Zieht die Kanten um `by`; die gegenüberliegenden bleiben stehen. Kanten liegen auf ganzen Pixeln im Fenster.
pub fn resize(r: Rect, e: Edges, by: (f32, f32), min: (f32, f32), screen: (f32, f32)) -> Rect {
    let axis = |at: f32, len: f32, low: bool, high: bool, by: f32, min: f32, total: f32| {
        let (mut a, mut b) = (at, at + len);
        if low {
            a = (a + by).round().min(b - min).max(0.0);
        }
        if high {
            b = (b + by).round().max(a + min).min(total.max(a + min));
        }
        (a, b - a)
    };
    let (x, w) = axis(r.x, r.w, e.left, e.right, by.0, min.0, screen.0);
    let (y, h) = axis(r.y, r.h, e.top, e.bottom, by.1, min.1, screen.1);
    Rect { x, y, w, h }
}

/// Gezogene Kanten rasten an Rändern und anderen Modulen ein. Liefert die Verschiebung und die Hilfslinien.
pub fn snap_edges(r: Rect, e: Edges, others: &[Rect], screen: (f32, f32), dist: f32) -> ((f32, f32), Vec<Guide>) {
    let near = |at: f32, lines: Vec<f32>| {
        let best = lines.into_iter().min_by(|a, b| (a - at).abs().total_cmp(&(b - at).abs()));
        best.filter(|l| (l - at).abs() <= dist)
    };
    let xs = || others.iter().flat_map(|o| [o.x, o.x + o.w]).chain([0.0, screen.0 / 2.0, screen.0]).collect();
    let ys = || others.iter().flat_map(|o| [o.y, o.y + o.h]).chain([0.0, screen.1 / 2.0, screen.1]).collect();
    let (mut by, mut guides) = ((0.0, 0.0), vec![]);
    if e.left || e.right {
        let at = if e.left { r.x } else { r.x + r.w };
        if let Some(l) = near(at, xs()) {
            by.0 = l - at;
            guides.push(Guide { vertical: true, at: l });
        }
    }
    if e.top || e.bottom {
        let at = if e.top { r.y } else { r.y + r.h };
        if let Some(l) = near(at, ys()) {
            by.1 = l - at;
            guides.push(Guide { vertical: false, at: l });
        }
    }
    (by, guides)
}

/// Wählt den nächsten Anker, damit die Lage Fenstergrößen übersteht.
pub fn placement(r: Rect, screen: (f32, f32), dpi: f32, from: Placement) -> Placement {
    use Anchor::*;
    let third = |c: f32, total: f32| ((c / total * 3.0) as i32).clamp(0, 2) as usize;
    let col = third(r.x + r.w / 2.0, screen.0);
    let row = third(r.y + r.h / 2.0, screen.1);
    let anchor = [[TopLeft, Top, TopRight], [Left, Center, Right], [BottomLeft, Bottom, BottomRight]][row][col];
    let (fx, fy) = anchor.factors();
    Placement { anchor, x: (r.x - (screen.0 - r.w) * fx) / dpi, y: (r.y - (screen.1 - r.h) * fy) / dpi, ..from }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: (f32, f32) = (1200.0, 900.0);
    const ONE: Placement = Placement { anchor: Anchor::TopLeft, x: 0.0, y: 0.0, scale: 1.0, w: 1.0, h: 1.0 };

    fn r(x: f32, y: f32) -> Rect {
        Rect { x, y, w: 100.0, h: 50.0 }
    }

    #[test]
    fn lage_bleibt_beim_umrechnen_gleich() {
        for (x, y) in [(0.0, 0.0), (550.0, 10.0), (1090.0, 840.0), (13.0, 420.0), (700.0, 500.0)] {
            for dpi in [1.0, 1.5] {
                let p = placement(r(x, y), S, dpi, Placement { scale: 1.0 / dpi, ..ONE });
                let back = p.resolve(S, (100.0, 50.0), dpi);
                assert!((back.x - x).abs() < 0.01 && (back.y - y).abs() < 0.01, "{x},{y} {back:?}");
            }
        }
    }

    #[test]
    fn ecke_bleibt_ecke_bei_neuer_fenstergroesse() {
        let p = placement(r(1090.0, 840.0), S, 1.0, ONE);
        let b = p.resolve((2560.0, 1440.0), (100.0, 50.0), 1.0);
        assert_eq!((b.x, b.y), (2450.0, 1380.0));
    }

    #[test]
    fn kanten_und_ecken_wie_bei_fenstern() {
        let at = |x: f32, y: f32| edges(r(200.0, 100.0), (x, y), 1.0);
        let e = |s: &str| Edges {
            left: s.contains('l'),
            right: s.contains('r'),
            top: s.contains('o'),
            bottom: s.contains('u'),
        };
        for (x, y, want) in [
            (250.0, 125.0, ""),
            (197.0, 125.0, "l"),
            (302.0, 125.0, "r"),
            (250.0, 97.0, "o"),
            (250.0, 153.0, "u"),
            (198.0, 98.0, "lo"),
            (302.0, 98.0, "ro"),
            (198.0, 152.0, "lu"),
            (302.0, 152.0, "ru"),
            // Nahe der Ecke gilt der Rand schon als Ecke.
            (210.0, 98.0, "lo"),
            (190.0, 125.0, ""),
        ] {
            assert_eq!(at(x, y), e(want), "{x} {y}");
        }
        // Auch ein winziges Feld lässt sich noch verschieben.
        let tiny = Rect { x: 10.0, y: 10.0, w: 8.0, h: 6.0 };
        assert!(!edges(tiny, (14.0, 13.0), 1.0).any());
    }

    #[test]
    fn gegenueberliegende_kante_bleibt_stehen() {
        let start = r(200.0, 100.0);
        let all = |l, r, o, u| Edges { left: l, right: r, top: o, bottom: u };
        let go = |e, by| resize(start, e, by, (16.0, 12.0), S);
        assert_eq!(go(all(false, true, false, false), (7.0, 99.0)), Rect { w: 107.0, ..start });
        assert_eq!(go(all(true, false, true, false), (-5.0, 3.0)), Rect { x: 195.0, y: 103.0, w: 105.0, h: 47.0 });
        // Nicht kleiner als erlaubt, nicht aus dem Fenster.
        assert_eq!(go(all(true, false, false, false), (500.0, 0.0)), Rect { x: 284.0, w: 16.0, ..start });
        assert_eq!(go(all(false, true, false, true), (5000.0, -500.0)), Rect { w: 1000.0, h: 12.0, ..start });
        assert_eq!(go(all(true, false, false, false), (-999.0, 0.0)), Rect { x: 0.0, w: 300.0, ..start });
        // Immer ganze Pixel, auch von krummen Maßen aus.
        let odd = resize(Rect { x: 10.3, w: 50.4, ..start }, all(false, true, false, false), (1.0, 0.0), (1.0, 1.0), S);
        assert!((odd.x + odd.w - 62.0).abs() < 0.001 && odd.x == 10.3);
    }

    #[test]
    fn gezogene_kante_rastet_ein() {
        let e = Edges { right: true, ..Default::default() };
        let (by, g) = snap_edges(r(200.0, 100.0), e, &[r(305.0, 400.0)], S, 8.0);
        assert_eq!((by, g.len()), ((5.0, 0.0), 1));
        assert_eq!(snap_edges(r(200.0, 100.0), e, &[], S, 8.0).1.len(), 0);
    }

    #[test]
    fn rastet_ein_oder_laesst_frei() {
        let (a, g) = snap(r(553.0, 300.0), &[], S, 8.0, 16.0);
        assert_eq!((a.x, a.y, g.len()), (550.0, 300.0, 1));
        let (b, g) = snap(r(204.0, 303.0), &[r(200.0, 100.0)], S, 8.0, 16.0);
        assert_eq!((b.x, b.y, g.len()), (200.0, 303.0, 1));
    }

    #[test]
    fn trifft_das_oberste() {
        let items = [(1, r(0.0, 0.0)), (2, r(50.0, 0.0))];
        assert_eq!(hit(&items, (75.0, 10.0)), Some(2));
        assert_eq!(hit(&items, (10.0, 10.0)), Some(1));
        assert_eq!(hit(&items, (500.0, 10.0)), None);
    }
}
