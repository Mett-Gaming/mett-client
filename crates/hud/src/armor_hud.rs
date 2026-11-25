//! Rüstung neben der Schnellleiste. Maße und Verhalten nach „uku's Armor HUD“ (MIT, Copyright 2021 BerdinskiyBear, Fork
//! von uku), auf dem auch NoRisks Armor HUD beruht; eigene Umsetzung. Einheiten sind die der Spieloberfläche.
use crate::appleskin::Atlas;
use crate::color::from_hsv;
use crate::game_icons::GameIcons;
use crate::{Draw, Rect, SHEET_WARN, gear, rgb};
use serde::{Deserialize, Serialize};

const STEP: f32 = 20.0;
const SIZE: f32 = 22.0;
/// Abstand zur Schnellleiste.
const GAP: f32 = 7.0;
const WARN: f32 = 8.0;
/// Zeilenhöhe der Schrift.
const LINE: f32 = 9.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    #[default]
    Left,
    Right,
    /// Helm und Brust links, Beine und Füße rechts.
    Split,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shown {
    Always,
    IfAny,
    #[default]
    NotEmpty,
    Damaged,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Durability {
    #[default]
    Bar,
    Number,
    Percent,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Klebt an der Schnellleiste; sonst frei verschiebbar.
    pub sticky: bool,
    pub side: Side,
    /// Hintergrund wie die Schnellleiste.
    pub background: bool,
    pub vertical: bool,
    pub reversed: bool,
    pub shown: Shown,
    pub durability: Durability,
    /// Symbole leerer Plätze.
    pub icons: bool,
    pub warning: bool,
    pub bob: f32,
    pub min_value: f32,
    pub min_percent: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sticky: true,
            side: Side::Left,
            background: true,
            vertical: false,
            reversed: false,
            shown: Shown::NotEmpty,
            durability: Durability::Bar,
            icons: true,
            warning: true,
            bob: 3.0,
            min_value: 20.0,
            min_percent: 10.0,
        }
    }
}

/// Warnzeichen von uku's Armor HUD (MIT).
pub fn warn() -> Option<&'static Atlas> {
    static ATLAS: std::sync::OnceLock<Option<Atlas>> = std::sync::OnceLock::new();
    ATLAS.get_or_init(|| Atlas::load(include_bytes!("../../../assets/armor-hud/warn.png"))).as_ref()
}

/// Ein Platz: Name und Abnutzung, leer ohne.
pub type Slot<'a> = Option<(&'a str, i32)>;

/// Rest und Höchstes, wenn bekannt.
fn left(slot: &Slot) -> Option<(i32, i32)> {
    let (name, damage) = (*slot)?;
    let most = gear::most(name)?;
    Some(((most - damage).clamp(0, most), most))
}

fn warns(s: &Settings, slot: &Slot) -> bool {
    left(slot).is_some_and(|(l, m)| l as f32 / m as f32 <= s.min_percent / 100.0 || l as f32 <= s.min_value)
}

/// Welche Plätze zu sehen sind, mit ihrer Nummer.
fn visible<'a>(s: &Settings, all: &[Slot<'a>; 4]) -> Vec<(usize, Slot<'a>)> {
    let any = all.iter().any(Option::is_some);
    let keep = |slot: &Slot| match s.shown {
        Shown::Always => true,
        Shown::IfAny => any,
        Shown::NotEmpty => slot.is_some(),
        Shown::Damaged => warns(s, slot),
    };
    let mut out: Vec<_> = all.iter().copied().enumerate().filter(|(_, slot)| keep(slot)).collect();
    if s.reversed {
        out.reverse();
    }
    out
}

fn extent(n: usize, vertical: bool) -> (f32, f32) {
    let long = SIZE + (n.max(1) - 1) as f32 * STEP;
    if vertical { (SIZE, long) } else { (long, SIZE) }
}

/// Größe des ganzen Felds bei vier Teilen, in Einheiten.
pub fn size(s: &Settings) -> (f32, f32) {
    extent(4, s.vertical)
}

pub struct Frame<'a> {
    pub icons: &'a GameIcons,
    /// Pixel je Einheit.
    pub unit: f32,
    /// Breite eines Texts in Schriftpixeln.
    pub width: &'a dyn Fn(&str) -> f32,
    /// Läuft mit der Zeit, für das Wippen.
    pub tick: u32,
}

/// Neben der Schnellleiste. `bar` in Pixeln.
pub fn beside(bar: Rect, s: &Settings, all: &[Slot; 4], f: &Frame, out: &mut Vec<Draw>) {
    let shown = visible(s, all);
    let put = |list: &[(usize, Slot)], right: bool, out: &mut Vec<Draw>| {
        if list.is_empty() {
            return;
        }
        let (w, h) = extent(list.len(), s.vertical);
        let x = if right { bar.x + bar.w + GAP * f.unit } else { bar.x - (GAP + w) * f.unit };
        group((x, bar.y + bar.h - h * f.unit), s, list, right, f, out);
    };
    match s.side {
        Side::Left => put(&shown, false, out),
        Side::Right => put(&shown, true, out),
        Side::Split => {
            let (a, b): (Vec<_>, Vec<_>) = shown.into_iter().partition(|(i, _)| *i < 2);
            put(&a, false, out);
            put(&b, true, out);
        }
    }
}

/// Frei platziert, linke obere Ecke.
pub fn free(at: (f32, f32), s: &Settings, all: &[Slot; 4], f: &Frame, out: &mut Vec<Draw>) {
    let shown = visible(s, all);
    if !shown.is_empty() {
        group(at, s, &shown, s.side != Side::Left, f, out);
    }
}

fn group(at: (f32, f32), s: &Settings, list: &[(usize, Slot)], right: bool, f: &Frame, out: &mut Vec<Draw>) {
    let u = f.unit;
    let n = list.len();
    if s.background {
        background(at, n, s.vertical, f, out);
    }
    for (i, (number, slot)) in list.iter().enumerate() {
        let (dx, dy) = if s.vertical { (0.0, STEP * i as f32) } else { (STEP * i as f32, 0.0) };
        let (x, y) = (at.0 + dx * u, at.1 + dy * u);
        let inner = (x + 3.0 * u, y + 3.0 * u);
        let Some((name, damage)) = slot else {
            if s.icons && matches!(s.shown, Shown::Always | Shown::IfAny) {
                let part = ["helmet", "chestplate", "leggings", "boots"][*number];
                let empty = format!("items/empty_armor_slot_{part}");
                f.icons.draw(&empty, inner, u, 1.0, out);
            }
            continue;
        };
        if let Some(picture) = f.icons.item(name) {
            f.icons.draw(&picture, inner, u, 1.0, out);
        }
        let Some((l, m)) = left(slot) else { continue };
        let part = l as f32 / m as f32;
        let hue = from_hsv(part / 3.0, 1.0, 1.0);
        if s.durability == Durability::Bar && *damage > 0 {
            // Wie an jedem Gegenstand: 13 breit, Schwarz darunter.
            let bar = |w: f32, h: f32, color| Draw::Box {
                rect: Rect { x: inner.0 + 2.0 * u, y: inner.1 + 13.0 * u, w: w * u, h: h * u },
                radius: 0.0,
                color: rgb(color, 1.0),
            };
            out.push(bar(13.0, 2.0, 0));
            out.push(bar((13.0 - (m - l) as f32 * 13.0 / m as f32).round(), 1.0, hue));
        }
        // Text und Warnung stehen außen.
        let (mut tx, mut ty) = (x, y);
        let text = match s.durability {
            Durability::Bar => String::new(),
            Durability::Number => l.to_string(),
            Durability::Percent if *damage == 0 => String::new(),
            Durability::Percent => format!("{}%", (part * 100.0).floor() as i32),
        };
        if s.vertical && right {
            tx += SIZE * u;
        }
        if s.durability != Durability::Bar {
            let w = (f.width)(&text);
            if s.vertical {
                let full = (w + 2.0) * u;
                if !right {
                    tx -= full;
                }
                write(&text, (tx + u, ty + (SIZE - LINE) / 2.0 * u), hue, u, out);
                if right {
                    tx += full;
                }
            } else {
                ty -= LINE * u;
                write(&text, (tx + (SIZE / 2.0 - (w / 2.0).floor()) * u, ty), hue, u, out);
            }
        }
        if s.warning && warns(s, slot) {
            let bob = match s.bob as u32 {
                0 => 0.0,
                b => {
                    // Je Platz und Bild ein anderer Wert.
                    let mixed = f.tick.wrapping_mul(2_654_435_761).wrapping_add(i as u32 * 40_503) >> 13;
                    (mixed % b) as f32 - (b as f32 / 2.0).ceil()
                }
            };
            let (wx, wy) = if s.vertical {
                let x = if right { tx } else { tx - (WARN + 2.0) * u };
                (x + u, ty + ((SIZE - WARN) / 2.0 + bob) * u)
            } else {
                (tx + (SIZE - WARN) / 2.0 * u, ty + (bob - WARN - 2.0 + 1.0) * u)
            };
            let rect = Rect { x: wx.round(), y: wy.round(), w: (WARN * u).round(), h: (WARN * u).round() };
            let from = Rect { x: 0.0, y: 0.0, w: WARN, h: WARN };
            out.push(Draw::Icon { sheet: SHEET_WARN, rect, from, color: [1.0; 4] });
        }
    }
}

/// Minecrafts eigene Schnellleiste: je Platz ein Bild, dazu die Enden.
fn background(at: (f32, f32), n: usize, vertical: bool, f: &Frame, out: &mut Vec<Draw>) {
    let u = f.unit;
    let turn = if vertical { "@90" } else { "" };
    let mut put = |name: &str, along: f32| {
        let p = if vertical { (at.0, at.1 + along * u) } else { (at.0 + along * u, at.1) };
        f.icons.draw(&format!("ui/{name}{turn}"), p, u, 1.0, out);
    };
    put("hotbar_start_cap", 0.0);
    for i in 0..n {
        // Der letzte Platz der Leiste ist ihr Ende.
        let k = if i + 1 == n { 8 } else { i.min(7) };
        put(&format!("hotbar_{k}"), 1.0 + i as f32 * STEP);
    }
    put("hotbar_end_cap", 1.0 + n as f32 * STEP);
}

/// Minecraft-Schrift mit Schatten.
fn write(text: &str, (x, y): (f32, f32), color: u32, u: f32, out: &mut Vec<Draw>) {
    let dark = crate::color::mix(color, 0, 0.75);
    for (d, c) in [(u, dark), (0.0, color)] {
        out.push(Draw::Game { x: x + d, y: y + d, unit: u, color: rgb(c, 1.0), text: text.into() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: [Slot; 4] = [
        Some(("minecraft:diamond_helmet", 0)),
        Some(("minecraft:netherite_chestplate", 580)),
        None,
        Some(("minecraft:iron_boots", 100)),
    ];

    fn run(s: &Settings) -> Vec<Draw> {
        let icons = crate::game_icons::tests_support();
        let f = Frame { icons: &icons, unit: 2.0, width: &|t| t.len() as f32 * 6.0, tick: 1 };
        let mut out = vec![];
        beside(Rect { x: 500.0, y: 600.0, w: 364.0, h: 44.0 }, s, &FULL, &f, &mut out);
        out
    }

    fn lefts(v: &[Draw]) -> (f32, f32) {
        let x = v.iter().filter_map(|d| match d {
            Draw::Box { rect, .. } | Draw::Icon { rect, .. } => Some((rect.x, rect.x + rect.w)),
            _ => None,
        });
        x.fold((f32::MAX, f32::MIN), |a, b| (a.0.min(b.0), a.1.max(b.1)))
    }

    #[test]
    fn sieben_neben_der_schnellleiste() {
        // Drei Teile: 22 + 2 × 20 = 62 breit.
        let (l, r) = lefts(&run(&Settings::default()));
        assert_eq!((l, r), (500.0 - (7.0 + 62.0) * 2.0, 500.0 - 7.0 * 2.0));
        let (l, r) = lefts(&run(&Settings { side: Side::Right, ..Settings::default() }));
        assert_eq!((l, r), (864.0 + 14.0, 864.0 + (7.0 + 62.0) * 2.0));
        let (l, r) = lefts(&run(&Settings { side: Side::Split, ..Settings::default() }));
        assert_eq!((l, r), (500.0 - (7.0 + 42.0) * 2.0, 864.0 + (7.0 + 22.0) * 2.0));
    }

    #[test]
    fn balken_nur_bei_abnutzung() {
        let bars = |s: &Settings| run(s).iter().filter(|d| matches!(d, Draw::Box { .. })).count();
        assert_eq!(bars(&Settings::default()), 4);
        assert_eq!(bars(&Settings { durability: Durability::Number, ..Settings::default() }), 0);
        // Breite wie im Spiel gerundet: 592 − 580 = 12 von 592 ist 0.
        let v = run(&Settings::default());
        let widths: Vec<f32> =
            v.iter().filter_map(|d| if let Draw::Box { rect, .. } = d { Some(rect.w / 2.0) } else { None }).collect();
        assert_eq!(widths, [13.0, 0.0, 13.0, 6.0]);
    }

    #[test]
    fn warnung_und_text() {
        let warns = |s: &Settings| {
            run(s).iter().filter(|d| matches!(d, Draw::Icon { sheet, .. } if *sheet == SHEET_WARN)).count()
        };
        assert_eq!(warns(&Settings::default()), 1);
        assert_eq!(warns(&Settings { warning: false, ..Settings::default() }), 0);
        let texts = |d| {
            let v = run(&Settings { durability: d, ..Settings::default() });
            let t = |d: &Draw| if let Draw::Game { text, .. } = d { Some(text.clone()) } else { None };
            v.iter().filter_map(t).filter(|t| !t.is_empty()).step_by(2).collect::<Vec<_>>()
        };
        assert_eq!(texts(Durability::Number), ["363", "12", "95"]);
        assert_eq!(texts(Durability::Percent), ["2%", "48%"]);
    }

    #[test]
    fn welche_plaetze_zu_sehen_sind() {
        let n = |shown| {
            let s = Settings { shown, ..Settings::default() };
            visible(&s, &FULL).iter().map(|v| v.0).collect::<Vec<_>>()
        };
        assert_eq!(n(Shown::Always), [0, 1, 2, 3]);
        assert_eq!(n(Shown::NotEmpty), [0, 1, 3]);
        assert_eq!(n(Shown::Damaged), [1]);
        let s = Settings { shown: Shown::IfAny, reversed: true, ..Settings::default() };
        assert_eq!(visible(&s, &FULL).iter().map(|v| v.0).collect::<Vec<_>>(), [3, 2, 1, 0]);
        assert!(visible(&s, &[None; 4]).is_empty());
    }
}
