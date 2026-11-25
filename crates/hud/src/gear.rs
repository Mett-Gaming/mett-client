//! Rüstung, Effekte und Gegenstandszähler: Zeilen mit Symbol und Text.
use crate::icons::{self, Icon};
use crate::{Align, Draw, Rect, Style, game_icons, lang, rgb};

/// Höchste Haltbarkeit der üblichen Teile, nach Namen. Unbekanntes zeigt nur den Namen.
const DURABILITY: [(&str, [i32; 4]); 8] = [
    ("leather", [55, 80, 75, 65]),
    ("golden", [77, 112, 105, 91]),
    ("chainmail", [165, 240, 225, 195]),
    ("copper", [121, 176, 165, 143]),
    ("iron", [165, 240, 225, 195]),
    ("diamond", [363, 528, 495, 429]),
    ("netherite", [407, 592, 555, 481]),
    ("turtle", [275, 0, 0, 0]),
];
const PARTS: [&str; 4] = ["_helmet", "_chestplate", "_leggings", "_boots"];
fn bare(name: &str) -> &str {
    name.split_once(':').map_or(name, |n| n.1)
}

/// Höchste Haltbarkeit, wenn bekannt.
pub fn most(name: &str) -> Option<i32> {
    let n = bare(name);
    if n == "elytra" {
        return Some(432);
    }
    let part = PARTS.iter().position(|p| n.ends_with(p))?;
    let row = DURABILITY.iter().find(|d| n.starts_with(d.0))?;
    Some(row.1[part]).filter(|m| *m > 0)
}

/// `minecraft:ender_pearl` → `Ender Pearl`.
pub fn title(name: &str) -> String {
    let words = bare(name).split('_').filter(|w| !w.is_empty());
    let cap = |w: &str| {
        let mut c = w.chars();
        c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
    };
    words.map(cap).collect::<Vec<_>>().join(" ")
}

/// Name und Farbe je Effektnummer.
pub fn effect(id: u32) -> (&'static str, u32) {
    match id {
        1 => ("Schnelligkeit", 0x7CAFC6),
        2 => ("Langsamkeit", 0x8A9FB8),
        3 => ("Eile", 0xD9C043),
        4 => ("Abbaulähmung", 0x8A7A3C),
        5 => ("Stärke", 0xE0524C),
        6 => ("Direktheilung", 0xF82423),
        7 => ("Direktschaden", 0xA0302A),
        8 => ("Sprungkraft", 0x7BE05A),
        9 => ("Übelkeit", 0xA06AB4),
        10 => ("Regeneration", 0xE87FC8),
        11 => ("Resistenz", 0xB09AE0),
        12 => ("Feuerschutz", 0xE49A3A),
        13 => ("Wasseratmung", 0x5A8AE0),
        14 => ("Unsichtbarkeit", 0xC8C8D0),
        15 => ("Blindheit", 0x7A7A82),
        16 => ("Nachtsicht", 0x6A7AE8),
        17 => ("Hunger", 0x8AA05A),
        18 => ("Schwäche", 0x9A9A9A),
        19 => ("Vergiftung", 0x7AB83A),
        20 => ("Ausdörrung", 0x8A7070),
        21 => ("Extraleben", 0xF88A3A),
        22 => ("Absorption", 0xF0D04A),
        23 => ("Sättigung", 0xF8A83A),
        24 => ("Schwebkraft", 0xCEF0F0),
        25 => ("Tödliches Gift", 0x7AB83A),
        26 => ("Meereskraft", 0x3ADAD8),
        27 => ("Sanfter Fall", 0xF0E0C8),
        28 => ("Böses Omen", 0x7A9A4A),
        29 => ("Held des Dorfes", 0x5AE05A),
        30 => ("Dunkelheit", 0x6A6A8A),
        _ => ("Effekt", 0xE8E0D1),
    }
}

/// Bild je Effektnummer, wie Minecraft es nennt.
fn picture(id: u32) -> Option<&'static str> {
    Some(match id {
        1 => "speed",
        2 => "slowness",
        3 => "haste",
        4 => "mining_fatigue",
        5 => "strength",
        8 => "jump_boost",
        9 => "nausea",
        10 => "regeneration",
        11 => "resistance",
        12 => "fire_resistance",
        13 => "water_breathing",
        14 => "invisibility",
        15 => "blindness",
        16 => "night_vision",
        17 => "hunger",
        18 => "weakness",
        19 | 25 => "poison",
        20 => "wither",
        21 => "health_boost",
        22 => "absorption",
        24 => "levitation",
        26 => "conduit_power",
        27 => "slow_falling",
        28 => "bad_omen",
        29 => "village_hero",
        30 => "darkness",
        _ => return None,
    })
}

/// Minuten und Sekunden aus Zwanzigstelsekunden.
pub fn time(left: i32) -> String {
    let s = left.max(0) / 20;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub icon: Icon,
    pub color: u32,
    /// Bild aus der Installation, wenn es eins gibt.
    pub picture: Option<String>,
    pub text: String,
}

/// (Nummer, Rest)
pub fn potions(list: &[(u32, i32)]) -> Vec<Row> {
    let row = |(id, left): &(u32, i32)| {
        let (name, color) = effect(*id);
        let picture =
            game_icons::get().and_then(|g| picture(*id).map(|p| format!("ui/{p}_effect")).filter(|p| g.has(p)));
        Row { icon: Icon::Potion, color, picture, text: format!("{}  {}", lang::t(name), time(*left)) }
    };
    list.iter().map(row).collect()
}

/// Zählt die gewünschten Arten. (Name, Anzahl)
pub fn counts(wanted: &str, stacks: &[(&str, u32)]) -> Vec<Row> {
    let names = wanted.split(',').map(|w| w.trim().to_lowercase().replace(' ', "_")).filter(|w| !w.is_empty());
    let row = |n: String| {
        let hit = |s: &&(&str, u32)| s.0 == n || bare(s.0) == n;
        let sum: u32 = stacks.iter().filter(hit).map(|s| s.1).sum();
        let picture = game_icons::get().and_then(|g| g.item(&n));
        Row { icon: Icon::Bag, color: 0xE8E0D1, picture, text: format!("{}  {sum}", title(&n)) }
    };
    names.map(row).take(8).collect()
}

/// Höhe einer Zeile bei Größe 1.
pub const ROW: f32 = 30.0;

/// Zeilen von oben; `at` ist so hoch, wie das Modul eingestellt ist.
pub fn draw(at: Rect, size: (f32, f32), style: &Style, rows: &[Row], out: &mut Vec<Draw>) {
    if rows.is_empty() {
        return;
    }
    let s = (at.w / size.0).min(at.h / size.1);
    let high = at.h / (size.1 / ROW);
    for (i, row) in rows.iter().enumerate() {
        let r = Rect { x: at.x, y: at.y + i as f32 * high, w: at.w, h: high - 2.0 * s };
        out.push(Draw::Box { rect: r, radius: style.radius * s, color: rgb(style.background, style.opacity) });
        if style.outline {
            out.push(Draw::Outline { rect: r, radius: style.radius * s, width: s, color: rgb(style.accent, 1.0) });
        }
        let icon = Rect { w: r.h, ..r };
        let side = (r.h - 6.0 * s).max(1.0);
        let drawn = game_icons::get().zip(row.picture.as_ref()).is_some_and(|(g, p)| {
            let unit = side / 16.0;
            g.draw(p, (icon.x + (icon.w - side) / 2.0, icon.y + (icon.h - side) / 2.0), unit, 1.0, out)
        });
        if !drawn {
            icons::draw(row.icon, icon, side, rgb(row.color, 1.0), out);
        }
        let x = r.x + r.h + 2.0 * s;
        let text = Rect { x, w: (r.x + r.w - x - 6.0 * s).max(1.0), ..r };
        let color = rgb(style.text, 1.0);
        out.push(Draw::Text { rect: text, size: 15.0 * s, color, align: Align::Left, text: row.text.clone() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haltbarkeit_und_namen() {
        assert_eq!(most("minecraft:diamond_helmet"), Some(363));
        assert_eq!(most("minecraft:netherite_boots"), Some(481));
        assert_eq!(most("minecraft:elytra"), Some(432));
        assert_eq!(most("lifeboat:hat_helmet"), None);
        assert_eq!(most("minecraft:turtle_boots"), None);
        assert_eq!(title("minecraft:ender_pearl"), "Ender Pearl");
        assert_eq!((time(1811), time(72_020)), ("1:30".into(), "1:00:01".into()));
    }

    #[test]
    fn zeilen() {
        let c = counts("arrow, Ender Pearl,", &[("minecraft:arrow", 2), ("minecraft:arrow", 60), ("a:b", 5)]);
        assert_eq!(c.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["Arrow  62", "Ender Pearl  0"]);
        assert_eq!(potions(&[(16, 1200)])[0].text, "Nachtsicht  1:00");
    }

    #[test]
    fn zeichnet_innerhalb_der_breite() {
        let mut v = vec![];
        let at = Rect { x: 10.0, y: 10.0, w: 130.0, h: 120.0 };
        draw(at, (130.0, 120.0), &Style::default(), &potions(&[(1, 200)]), &mut v);
        assert!(v.len() >= 3);
        for d in &v {
            if let Draw::Box { rect, .. } = d {
                assert!(rect.x >= at.x && rect.x + rect.w <= at.x + at.w + 0.01);
            }
        }
    }
}
