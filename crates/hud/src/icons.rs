//! Symbole des Menüs von game-icons.net (CC BY 3.0, Urheber in `assets/game-icons/LIZENZ.txt`), weiß auf einer Tafel.
use crate::appleskin::Atlas;
use crate::{Draw, Rect, SHEET_ICONS};
use std::sync::OnceLock;

/// Kante eines Symbols auf der Tafel und Symbole je Zeile.
const CELL: f32 = 128.0;
const ROW: u32 = 8;

/// Reihenfolge wie auf der Tafel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Icon {
    Keyboard,
    Click,
    Clock,
    Speed,
    Processor,
    Battery,
    Notes,
    Bars,
    Hourglass,
    Stopwatch,
    Quill,
    Bubble,
    Lens,
    Friends,
    Apple,
    Puzzle,
    Move,
    Cog,
    Palette,
    Check,
    Knobs,
    Previous,
    Next,
    Reset,
    Pencil,
    Crosshair,
    Sun,
    Chest,
    Helmet,
    Chestplate,
    Leggings,
    Boots,
    Potion,
    Bag,
    Cube,
}

pub fn atlas() -> Option<&'static Atlas> {
    static ATLAS: OnceLock<Option<Atlas>> = OnceLock::new();
    let load = || Atlas::load(include_bytes!("../../../assets/game-icons/icons.png"));
    ATLAS.get_or_init(load).as_ref()
}

/// Mittig im Rechteck, `size` ist die Kante. `color` färbt ein.
pub fn draw(icon: Icon, at: Rect, size: f32, color: [f32; 4], out: &mut Vec<Draw>) {
    let i = icon as u32;
    let from = Rect { x: (i % ROW) as f32 * CELL, y: (i / ROW) as f32 * CELL, w: CELL, h: CELL };
    let rect = Rect {
        x: (at.x + (at.w - size) / 2.0).round(),
        y: (at.y + (at.h - size) / 2.0).round(),
        w: size.round(),
        h: size.round(),
    };
    out.push(Draw::Icon { sheet: SHEET_ICONS, rect, from, color });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jedes_symbol_steht_auf_der_tafel() {
        let a = atlas().unwrap();
        assert_eq!((a.width, a.height), (1024, 640));
        for i in 0..=Icon::Cube as u32 {
            let (x, y) = (i % ROW * 128, i / ROW * 128);
            let solid = (0..128 * 128).filter(|p| {
                let at = ((y + p / 128) * a.width + x + p % 128) * 4 + 3;
                a.rgba[at as usize] > 128
            });
            assert!(solid.count() > 500, "{i}");
        }
    }
}
