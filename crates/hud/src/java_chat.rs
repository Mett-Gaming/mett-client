//! Chat mit den Maßen und dem Verhalten der Java Edition (26.3), eigene Umsetzung. Alle Maße in Java-GUI-Pixeln.
use crate::{Color, Draw, Rect, Style, rgb};

/// Breite des Texts.
pub const WIDTH: f32 = 320.0;
/// Für den Text bleiben links 4 und rechts 8 frei.
pub const MARGINS: f32 = 12.0;
/// Breite des Hintergrunds: 4 links, 8 rechts vom Text.
pub const FULL_WIDTH: f32 = WIDTH + 12.0;
pub const LINE: f32 = 9.0;
/// Zeilen im Spiel und bei offenem Chat.
pub const ROWS: usize = 10;
pub const ROWS_OPEN: usize = 20;
/// Abstand zum unteren Bildschirmrand.
pub const BOTTOM: f32 = 40.0;
const TEXT_X: f32 = 4.0;
/// So weit liegt die Textoberkante über der Zeilenunterkante.
const TEXT_UP: f32 = 8.0;
const VISIBLE_SECONDS: f32 = 10.0;
/// Fortgesetzte Zeilen beginnen mit einem Leerzeichen.
const INDENT: f32 = 4.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub color: u32,
    pub text: String,
}

pub type Row = Vec<Segment>;

/// Deckkraft nach Alter: voll, dann in der letzten Sekunde quadratisch auf null.
pub fn fade(age_seconds: f32) -> f32 {
    let t = ((1.0 - age_seconds / VISIBLE_SECONDS) * 10.0).clamp(0.0, 1.0);
    t * t
}

/// Java-Pixel in Bildschirmpixeln: ganze Zahl, damit die Pixelschrift scharf bleibt.
pub fn unit(screen: (f32, f32)) -> f32 {
    (screen.1 / 480.0).floor().max(1.0)
}

fn code_color(c: char, normal: u32) -> Option<u32> {
    Some(match c.to_ascii_lowercase() {
        '0' => 0x000000,
        '1' => 0x0000AA,
        '2' => 0x00AA00,
        '3' => 0x00AAAA,
        '4' => 0xAA0000,
        '5' => 0xAA00AA,
        '6' => 0xFFAA00,
        '7' => 0xAAAAAA,
        '8' => 0x555555,
        '9' => 0x5555FF,
        'a' => 0x55FF55,
        'b' => 0x55FFFF,
        'c' => 0xFF5555,
        'd' => 0xFF55FF,
        'e' => 0xFFFF55,
        'f' => 0xFFFFFF,
        'r' => normal,
        _ => return None,
    })
}

/// Farbzeichen auswerten. Formatzeichen ohne Farbe werden entfernt.
pub fn parse(text: &str, normal: u32) -> Row {
    let mut row: Row = vec![];
    let mut color = normal;
    let mut it = text.chars();
    while let Some(c) = it.next() {
        if c == '§' {
            if let Some(n) = it.next().and_then(|k| code_color(k, normal)) {
                color = n;
            }
        } else if !c.is_control() {
            match row.last_mut() {
                Some(s) if s.color == color => s.text.push(c),
                _ => row.push(Segment { color, text: c.into() }),
            }
        }
    }
    row
}

/// Bricht an Wortgrenzen um; überlange Wörter werden geteilt. `advance` ist die Breite eines Zeichens.
pub fn wrap(row: &[Segment], width: f32, advance: &dyn Fn(char) -> f32) -> Vec<Row> {
    let chars: Vec<(char, u32)> = row.iter().flat_map(|s| s.text.chars().map(move |c| (c, s.color))).collect();
    let mut rows = vec![];
    let mut start = 0;
    while start < chars.len() {
        let room = if rows.is_empty() { width } else { width - INDENT };
        let (mut used, mut end, mut space) = (0.0, start, None);
        while end < chars.len() {
            // Vor der Breite prüfen: Auch ein Leerzeichen genau an der Grenze ist eine Umbruchstelle.
            if chars[end].0 == ' ' {
                space = Some(end);
            }
            used += advance(chars[end].0);
            if used > room {
                break;
            }
            end += 1;
        }
        // Mindestens ein Zeichen, sonst ginge es nie weiter.
        let mut next = end.max(start + 1);
        let mut stop = next;
        if end < chars.len()
            && let Some(s) = space.filter(|s| *s > start)
        {
            stop = s;
            next = s + 1;
        }
        let mut out: Row = vec![];
        for (c, color) in &chars[start..stop] {
            match out.last_mut() {
                Some(s) if s.color == *color => s.text.push(*c),
                _ => out.push(Segment { color: *color, text: (*c).into() }),
            }
        }
        if !rows.is_empty()
            && let Some(first) = out.first_mut()
        {
            first.text.insert(0, ' ');
        }
        rows.push(out);
        start = next;
    }
    if rows.is_empty() {
        rows.push(vec![]);
    }
    rows
}

/// Schatten: ein Viertel der Helligkeit, einen Pixel versetzt.
pub fn text(
    x: f32,
    y: f32,
    unit: f32,
    row: &[Segment],
    alpha: f32,
    advance: &dyn Fn(char) -> f32,
    out: &mut Vec<Draw>,
) {
    let mut at = x;
    for s in row {
        let color = rgb(s.color, alpha);
        let shadow: Color = [color[0] / 4.0, color[1] / 4.0, color[2] / 4.0, alpha];
        let t = s.text.clone();
        out.push(Draw::Game { x: at + unit, y: y + unit, unit, color: shadow, text: t.clone() });
        out.push(Draw::Game { x: at, y, unit, color, text: t });
        at += s.text.chars().map(advance).sum::<f32>() * unit;
    }
}

/// Beispiele für den Editor, von alt nach neu.
pub const SAMPLE: [&str; ROWS] = [
    "<Alex> Ist noch jemand wach?",
    "<Steve> Gleich da",
    "§eSteve ist dem Spiel beigetreten",
    "<Steve> Hallo zusammen!",
    "<Alex> Hi! Kommst du mit in die Mine?",
    "§aDu hast einen Diamanten gefunden",
    "<Steve> Ja, ich hole nur noch schnell Fackeln und etwas zu essen, dann können wir los",
    "§7[Server] Neustart in 10 Minuten",
    "§cDu hast keine Rechte für diesen Befehl",
    "<Alex> Bis gleich!",
];

/// Maße von Minecrafts Anzeige: Zeilenhöhe, Textabstand links und oben, Deckkraft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Native {
    pub line: f32,
    pub left: f32,
    pub top: f32,
    pub opacity: f32,
}

pub const BEDROCK: Native = Native { line: 11.0, left: 2.0, top: 2.0, opacity: 0.7 };
pub const JAVA: Native = Native { line: LINE, left: TEXT_X, top: LINE - TEXT_UP, opacity: 0.5 };

/// So viele Zeilen passen hinein, mindestens eine.
pub fn rows(height: f32, unit: f32) -> usize {
    ((height / (LINE * unit) + 0.01) as usize).max(1)
}

/// So viele einzeilige Nachrichten passen hinein, mindestens eine.
pub fn native_rows(height: f32, unit: f32, n: Native) -> usize {
    ((height / (n.line * unit) + 0.01) as usize).max(1)
}

/// Platz für so viele einzeilige Nachrichten.
pub fn native_height(unit: f32, rows: usize, n: Native) -> f32 {
    rows as f32 * n.line * unit
}

/// Beispiele in den Maßen von Minecrafts eigener Anzeige; `unit` sind Pixel je Einheit der Oberfläche.
pub fn native_sample(
    at: (f32, f32),
    (width, unit): (f32, f32),
    (count, n): (usize, Native),
    advance: &dyn Fn(char) -> f32,
    out: &mut Vec<Draw>,
) {
    let room = width / unit - 2.0 * n.left;
    let rows: Vec<Row> = SAMPLE.iter().flat_map(|t| wrap(&parse(crate::lang::t(t), 0xFFFFFF), room, advance)).collect();
    let h = n.line * unit;
    for (i, row) in rows.iter().rev().cycle().take(count).enumerate() {
        let y = at.1 - (i + 1) as f32 * h;
        let rect = Rect { x: at.0, y, w: width, h };
        out.push(Draw::Box { rect, radius: 0.0, color: rgb(0, n.opacity) });
        text(at.0 + n.left * unit, y + n.top * unit, unit, row, 1.0, advance, out);
    }
}

/// Verlauf des offenen Chats in den Maßen von Minecrafts Anzeige. `rows` von alt nach neu, `scroll` Zeilen nach oben.
pub fn history(
    at: Rect,
    (unit, n): (f32, Native),
    rows: &[Row],
    scroll: usize,
    advance: &dyn Fn(char) -> f32,
    out: &mut Vec<Draw>,
) {
    let h = n.line * unit;
    let shown = native_rows(at.h, unit, n);
    for (i, row) in rows.iter().rev().skip(scroll).take(shown).enumerate() {
        let y = at.y + at.h - (i + 1) as f32 * h;
        out.push(Draw::Box { rect: Rect { x: at.x, y, w: at.w, h }, radius: 0.0, color: rgb(0, n.opacity) });
        text(at.x + n.left * unit, y + n.top * unit, unit, row, 1.0, advance, out);
    }
    if rows.len() > shown {
        let full = shown as f32 * h;
        let bar = full * shown as f32 / rows.len() as f32;
        let y = at.y + at.h - bar - full * scroll as f32 / rows.len() as f32;
        let rect = Rect { x: at.x + at.w - 2.0 * unit, y, w: 2.0 * unit, h: bar };
        out.push(Draw::Box { rect, radius: 0.0, color: rgb(0xCCCCCC, 0.67) });
    }
}

/// Wo man nach dem Blättern steht. Kommen Zeilen dazu, bleibt beim Lesen dieselbe Stelle stehen.
pub fn scrolled(scroll: usize, before: usize, total: usize, shown: usize, by: i32) -> usize {
    let added = if scroll > 0 { total.saturating_sub(before) } else { 0 };
    (scroll + added).saturating_add_signed(by as isize).min(total.saturating_sub(shown))
}

/// Zeichnet Zeilen von unten nach oben; die letzte in `rows` ist die neueste. `unit` enthält die Größe.
pub fn draw(
    (at, unit): (Rect, f32),
    style: &Style,
    rows: &[(Row, f32)],
    advance: &dyn Fn(char) -> f32,
    out: &mut Vec<Draw>,
) {
    for (i, (_, alpha)) in rows.iter().rev().enumerate() {
        let bottom = at.y + at.h - i as f32 * LINE * unit;
        let top = bottom - LINE * unit;
        if *alpha <= 1e-5 || top < -0.5 {
            continue;
        }
        let rect = Rect { x: at.x, y: top, w: at.w, h: LINE * unit };
        out.push(Draw::Box { rect, radius: 0.0, color: rgb(style.background, style.opacity * alpha) });
    }
    for (i, (row, alpha)) in rows.iter().rev().enumerate() {
        let bottom = at.y + at.h - i as f32 * LINE * unit;
        if *alpha <= 1e-5 || bottom - LINE * unit < -0.5 {
            continue;
        }
        text(at.x + TEXT_X * unit, bottom - TEXT_UP * unit, unit, row, *alpha, advance, out);
    }
}

/// Rollbalken des offenen Chats, rechts neben dem Text.
pub fn scrollbar((at, unit): (Rect, f32), shown: usize, total: usize, scroll: usize, out: &mut Vec<Draw>) {
    if total <= shown {
        return;
    }
    let full = shown as f32 * LINE * unit;
    let h = full * shown as f32 / total as f32;
    let y = at.y + at.h - h - full * scroll as f32 / total as f32;
    let x = at.x + at.w - (MARGINS - 4.0) * unit;
    let mut bar = |x: f32, w: f32, color| {
        out.push(Draw::Box { rect: Rect { x, y, w: w * unit, h }, radius: 0.0, color });
    };
    bar(x, 2.0, rgb(0x3333AA, 0.67));
    bar(x + unit, 1.0, rgb(0xCCCCCC, 0.67));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn six(_: char) -> f32 {
        6.0
    }

    fn plain(rows: &[Row]) -> Vec<String> {
        rows.iter().map(|r| r.iter().map(|s| s.text.as_str()).collect()).collect()
    }

    #[test]
    fn ausblenden_wie_in_java() {
        assert_eq!(fade(0.0), 1.0);
        assert_eq!(fade(9.0), 1.0);
        assert!((fade(9.5) - 0.25).abs() < 1e-4);
        assert_eq!(fade(10.0), 0.0);
        assert_eq!(fade(99.0), 0.0);
    }

    #[test]
    fn farben_werden_getrennt() {
        let r = parse("a§cb§lc§rd§", 0xFFFFFF);
        let want = [(0xFFFFFF, "a"), (0xFF5555, "bc"), (0xFFFFFF, "d")];
        assert_eq!(r.len(), 3);
        for (s, (c, t)) in r.iter().zip(want) {
            assert_eq!((s.color, s.text.as_str()), (c, t));
        }
    }

    #[test]
    fn umbruch_mit_einzug_und_farbe() {
        let rows = wrap(&parse("aa bb §ccc", 0xFFFFFF), 30.0, &six);
        assert_eq!(plain(&rows), ["aa bb", " cc"]);
        assert_eq!(rows[1].last().unwrap().color, 0xFF5555);
        assert_eq!(plain(&wrap(&parse("abcdefgh", 0), 18.0, &six)), ["abc", " de", " fg", " h"]);
        assert_eq!(wrap(&[], 18.0, &six).len(), 1);
        // Zu schmal darf nicht hängen bleiben.
        assert_eq!(wrap(&parse("abc", 0), 1.0, &six).len(), 3);
    }

    #[test]
    fn neueste_zeile_steht_unten() {
        let at = Rect { x: 0.0, y: 100.0, w: FULL_WIDTH * 3.0, h: 90.0 * 3.0 };
        let rows = [(parse("alt", 0xFFFFFF), 1.0), (parse("neu", 0xFFFFFF), 1.0), (parse("weg", 0), 0.0)];
        let mut out = vec![];
        draw((at, 3.0), &Style::default(), &rows, &six, &mut out);
        let boxes: Vec<_> = out.iter().filter(|d| matches!(d, Draw::Box { .. })).collect();
        assert_eq!(boxes.len(), 2);
        let texts: Vec<(f32, &str)> = out
            .iter()
            .filter_map(|d| if let Draw::Game { y, text, .. } = d { Some((*y, text.as_str())) } else { None })
            .collect();
        // Je Text Schatten und Vordergrund; "neu" eine Zeile tiefer als "alt".
        assert_eq!(texts.len(), 4);
        let y = |t| texts.iter().find(|x| x.1 == t).unwrap().0;
        assert_eq!(y("neu") - y("alt"), 27.0);
    }

    #[test]
    fn verlauf_blaettert_und_bleibt_im_feld() {
        let rows: Vec<Row> = (0..50).map(|i| parse(&format!("n{i}"), 0xFFFFFF)).collect();
        let at = Rect { x: 10.0, y: 100.0, w: 600.0, h: 20.0 * 11.0 * 2.0 };
        let draw = |scroll| {
            let mut out = vec![];
            history(at, (2.0, BEDROCK), &rows, scroll, &six, &mut out);
            for d in &out {
                if let Draw::Box { rect: r, .. } = d {
                    assert!(r.y >= at.y - 0.1 && r.y + r.h <= at.y + at.h + 0.1 && r.x + r.w <= at.x + at.w + 0.1);
                }
            }
            let text = |d: &Draw| if let Draw::Game { text, .. } = d { Some(text.clone()) } else { None };
            let t: Vec<String> = out.iter().filter_map(text).collect();
            (t.first().cloned().unwrap(), t.last().cloned().unwrap(), t.len() / 2)
        };
        assert_eq!(draw(0), ("n49".into(), "n30".into(), 20));
        assert_eq!(draw(30), ("n19".into(), "n0".into(), 20));
        // Blättern endet oben und unten.
        assert_eq!(scrolled(0, 50, 50, 20, 7), 7);
        assert_eq!(scrolled(28, 50, 50, 20, 7), 30);
        assert_eq!(scrolled(3, 50, 50, 20, -7), 0);
        // Neue Zeilen: unten bleibt unten, oben bleibt die Stelle.
        assert_eq!(scrolled(0, 50, 52, 20, 0), 0);
        assert_eq!(scrolled(7, 50, 52, 20, 0), 9);
        assert_eq!(scrolled(5, 50, 10, 20, 0), 0);
    }

    #[test]
    fn ganze_pixel() {
        assert_eq!(unit((2560.0, 1440.0)), 3.0);
        assert_eq!(unit((1920.0, 1080.0)), 2.0);
        assert_eq!(unit((640.0, 360.0)), 1.0);
    }

    #[test]
    fn beispiele_stehen_auf_dem_unteren_rand() {
        let mut out = vec![];
        native_sample((30.0, 900.0), (1024.8, 3.0), (10, BEDROCK), &|_| 6.0, &mut out);
        let boxes: Vec<Rect> =
            out.iter().filter_map(|d| if let Draw::Box { rect, .. } = d { Some(*rect) } else { None }).collect();
        let last = boxes.last().unwrap();
        assert!((900.0 - last.y - native_height(3.0, 10, BEDROCK)).abs() < 0.01);
        assert!((boxes[0].y + boxes[0].h - 900.0).abs() < 0.01);
        assert!(boxes.iter().all(|b| b.x == 30.0 && b.w == 1024.8 && b.h == 33.0));
        assert!(boxes.windows(2).all(|w| (w[1].y + w[1].h - w[0].y).abs() < 0.01));
    }
}
