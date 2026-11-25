//! Spielerliste mit den Maßen der Java Edition (26.3), eigene Umsetzung. Alle Maße in Java-GUI-Pixeln.
//! Den Ping kennt Minecraft nicht; sein Platz bleibt frei.
use crate::java_chat::{parse, text};
use crate::{Draw, Rect, rgb};

const MOST: usize = 80;
const ROWS: usize = 20;
const TOP: f32 = 10.0;
const ROW: f32 = 9.0;
const GAP: f32 = 5.0;
/// Kopf, 8 Pixel und einer Abstand.
const HEAD: f32 = 9.0;
const FACE: usize = 8;
/// Platz rechts vom Namen.
const PING: f32 = 13.0;

/// Name und Gesicht (8 × 8 Punkte, je Rot, Grün, Blau, Deckkraft).
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub name: String,
    pub face: Option<Vec<u8>>,
}

/// Ein Kästchen je Punkt, gleiche Nachbarn in einer Zeile zusammen.
fn head(face: &[u8], x: f32, y: f32, unit: f32, out: &mut Vec<Draw>) {
    let dots = face.as_chunks::<4>().0;
    for (j, row) in dots.as_chunks::<FACE>().0.iter().take(FACE).enumerate() {
        let mut i = 0;
        while i < FACE {
            let n = row[i..].iter().take_while(|c| **c == row[i]).count();
            let [r, g, b, a] = row[i];
            if a > 0 {
                let rect = Rect { x: x + i as f32 * unit, y: y + j as f32 * unit, w: n as f32 * unit, h: unit };
                let color = rgb((r as u32) << 16 | (g as u32) << 8 | b as u32, 1.0);
                out.push(Draw::Box { rect, radius: 0.0, color });
            }
            i += n;
        }
    }
}

/// Ohne Farbzeichen, für die Reihenfolge.
fn plain(name: &str) -> String {
    parse(name, 0).iter().map(|s| s.text.as_str()).collect::<String>().to_lowercase()
}

/// (Spalten, Zeilen) wie in Java: höchstens 20 Zeilen je Spalte.
fn shape(count: usize) -> (usize, usize) {
    let (mut columns, mut rows) = (1, count);
    while rows > ROWS {
        columns += 1;
        rows = count.div_ceil(columns);
    }
    (columns, rows)
}

pub fn draw(screen: (f32, f32), unit: f32, players: &[Player], advance: &dyn Fn(char) -> f32, out: &mut Vec<Draw>) {
    let mut players: Vec<&Player> = players.iter().collect();
    players.sort_by_cached_key(|p| plain(&p.name));
    players.truncate(MOST);
    if players.is_empty() {
        return;
    }
    let rows_of: Vec<_> = players.iter().map(|p| parse(&p.name, 0xFFFFFF)).collect();
    let width = |r: &[crate::java_chat::Segment]| r.iter().flat_map(|s| s.text.chars()).map(advance).sum::<f32>();
    let widest = rows_of.iter().map(|r| width(r)).fold(0.0, f32::max).ceil();
    let (columns, rows) = shape(players.len());
    let (wide, n) = ((screen.0 / unit).floor(), columns as f32);
    let column = ((n * (HEAD + widest + PING)).min(wide - 50.0) / n).floor().max(1.0);
    let total = column * n + (n - 1.0) * GAP;
    let left = (wide / 2.0).floor() - (total / 2.0).floor();
    let mut fill = |x: f32, y: f32, w: f32, h: f32, color| {
        let rect = Rect { x: x * unit, y: y * unit, w: w * unit, h: h * unit };
        out.push(Draw::Box { rect, radius: 0.0, color });
    };
    fill(left - 1.0, TOP - 1.0, total + 2.0, rows as f32 * ROW + 1.0, rgb(0, 0.5));
    let place = |i: usize| (left + (i / rows) as f32 * (column + GAP), TOP + (i % rows) as f32 * ROW);
    for i in 0..players.len() {
        let (x, y) = place(i);
        fill(x, y, column, ROW - 1.0, rgb(0xFFFFFF, 0.125));
    }
    for (i, r) in rows_of.iter().enumerate() {
        let (x, y) = place(i);
        if let Some(f) = &players[i].face {
            head(f, x * unit, y * unit, unit, out);
        }
        text((x + HEAD) * unit, y * unit, unit, r, 1.0, advance, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: usize) -> Vec<Player> {
        (0..n).map(|i| Player { name: format!("Spieler{i:03}"), face: None }).collect()
    }

    fn boxes(out: &[Draw]) -> Vec<Rect> {
        out.iter().filter_map(|d| if let Draw::Box { rect, .. } = d { Some(*rect) } else { None }).collect()
    }

    #[test]
    fn spalten_wie_in_java() {
        let want = [(1, (1, 1)), (20, (1, 20)), (21, (2, 11)), (40, (2, 20)), (41, (3, 14)), (80, (4, 20))];
        want.iter().for_each(|(n, s)| assert_eq!(shape(*n), *s, "{n}"));
    }

    #[test]
    fn mittig_und_auf_ganzen_pixeln() {
        for n in [1, 5, 21, 80, 200] {
            let mut out = vec![];
            draw((2560.0, 1440.0), 3.0, &names(n), &|_| 6.0, &mut out);
            let b = boxes(&out);
            assert_eq!(b.len(), 1 + n.min(80));
            let back = b[0];
            assert!((back.x + back.w / 2.0 - 1280.0).abs() <= 6.0, "{back:?}");
            assert_eq!(back.y, 27.0);
            assert!(b.iter().all(|r| r.x % 1.0 == 0.0 && r.w % 1.0 == 0.0));
            // Alles liegt im Hintergrund.
            assert!(b.iter().all(|r| r.x >= back.x && r.x + r.w <= back.x + back.w + 0.01));
            assert!(b.iter().all(|r| r.y >= back.y && r.y + r.h <= back.y + back.h + 0.01));
        }
        let mut out = vec![];
        draw((2560.0, 1440.0), 3.0, &[], &|_| 6.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn kopf_steht_vor_dem_namen() {
        // Obere Hälfte rot, unten je Punkt eine Farbe, die letzte Reihe durchsichtig.
        let dot = |x: u8, y: u8| match y {
            0..4 => [255, 0, 0, 255],
            7 => [0; 4],
            _ => [x, y, 0, 255],
        };
        let face: Vec<u8> = (0..64u8).flat_map(|i| dot(i % 8, i / 8)).collect();
        let mut out = vec![];
        let one = [Player { name: "Anna".into(), face: Some(face) }];
        draw((2560.0, 1440.0), 3.0, &one, &|_| 6.0, &mut out);
        let b = boxes(&out);
        // Hintergrund, Zeile, 4 ganze Reihen und 3 mal 8 Punkte.
        assert_eq!(b.len(), 2 + 4 + 24);
        let (row, heads) = (b[1], &b[2..]);
        assert!(
            heads
                .iter()
                .all(|h| h.x >= row.x && h.x + h.w <= row.x + 24.0 && h.y >= row.y && h.y + h.h <= row.y + 24.0)
        );
        let text = out.iter().find_map(|d| if let Draw::Game { x, .. } = d { Some(*x) } else { None });
        // Zuerst der Schatten, einen Pixel versetzt.
        assert_eq!(text, Some(row.x + 27.0 + 3.0));
    }

    #[test]
    fn sortiert_ohne_farben_und_gross_klein() {
        let mut out = vec![];
        let list = ["§czora", "Anna", "§abert"].map(|n| Player { name: n.into(), face: None });
        draw((2560.0, 1440.0), 3.0, &list, &|_| 6.0, &mut out);
        let texts: Vec<&str> = out
            .iter()
            .filter_map(|d| if let Draw::Game { text, .. } = d { Some(text.as_str()) } else { None })
            .collect();
        // Je Name Schatten und Vordergrund.
        assert_eq!(texts, ["Anna", "Anna", "bert", "bert", "zora", "zora"]);
    }
}
