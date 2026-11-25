//! Sättigung und Erschöpfung an der Hungerleiste, wie in AppleSkin (Unlicense) und mit dessen Bilddatei.
//! Alle Maße in Einheiten der Spieloberfläche: zehn Symbole zu 9, alle 8 eines, von rechts nach links.
use crate::{Draw, Rect, SHEET_APPLESKIN};
use std::sync::OnceLock;

const ICON: f32 = 9.0;
const STEP: f32 = 8.0;
const WIDTH: f32 = 81.0;
const FULL: f32 = 20.0;
/// Zeile des Erschöpfungsbalkens in der Bilddatei.
const BAR: f32 = 18.0;
/// Bei so viel Erschöpfung geht ein Punkt Sättigung oder Hunger verloren.
pub const MOST_EXHAUSTION: f32 = 4.0;

/// `icons.png`, ausgepackt.
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Atlas {
    pub fn load(file: &'static [u8]) -> Option<Self> {
        let mut d = png::Decoder::new(file);
        d.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
        let mut r = d.read_info().ok()?;
        let mut rgba = vec![0; r.output_buffer_size()];
        let i = r.next_frame(&mut rgba).ok()?;
        let right = i.color_type == png::ColorType::Rgba && rgba.len() == (i.width * i.height * 4) as usize;
        right.then_some(Self { width: i.width, height: i.height, rgba })
    }

    fn solid(&self, x: usize, y: usize) -> bool {
        self.rgba.get((y * self.width as usize + x) * 4 + 3).is_some_and(|a| *a > 0)
    }
}

pub fn atlas() -> Option<&'static Atlas> {
    static ATLAS: OnceLock<Option<Atlas>> = OnceLock::new();
    ATLAS.get_or_init(|| Atlas::load(include_bytes!("../../../assets/appleskin/icons.png"))).as_ref()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Show {
    pub saturation: Option<f32>,
    pub exhaustion: Option<f32>,
}

/// Welches der vier Symbole, wie in AppleSkin. `i` zählt von rechts.
fn quarter(saturation: f32, i: usize) -> f32 {
    match saturation / 2.0 - i as f32 {
        p if p >= 1.0 => 3.0,
        p if p > 0.5 => 2.0,
        p if p > 0.25 => 1.0,
        _ => 0.0,
    }
}

/// Liegt der Punkt auf einer Keule? Umriss ist das volle Sättigungssymbol der Bilddatei.
fn on_icon(a: &Atlas, from_right: usize, y: usize) -> bool {
    let inside = |i: usize| {
        // Spalte im Symbol `i`.
        let x = (i * STEP as usize + ICON as usize).checked_sub(from_right + 1).filter(|x| *x < 9);
        x.is_some_and(|x| (0..=x).any(|x| a.solid(27 + x, y)) && (x..9).any(|x| a.solid(27 + x, y)))
    };
    (0..10).any(inside)
}

/// `right`/`top`: rechte obere Ecke der Hungerleiste in Pixeln, `unit`: Pixel je Einheit.
pub fn draw(right: f32, top: f32, unit: f32, show: &Show, out: &mut Vec<Draw>) {
    let Some(a) = atlas() else { return };
    let mut put = |x: f32, y: f32, from: Rect, alpha| {
        // Auf ganze Punkte, sonst verschwimmt das Muster.
        let (l, t) = ((right + x * unit).round(), (top + y * unit).round());
        let (r, b) = ((right + (x + from.w) * unit).round(), (top + (y + from.h) * unit).round());
        let rect = Rect { x: l, y: t, w: r - l, h: b - t };
        out.push(Draw::Icon { sheet: SHEET_APPLESKIN, rect, from, color: [1.0, 1.0, 1.0, alpha] });
    };
    if let Some(e) = show.exhaustion.filter(|e| e.is_finite()) {
        // AppleSkin zeichnet den Balken hinter die Symbole. Wir liegen darüber und lassen sie deshalb aus.
        let n = ((e / MOST_EXHAUSTION).clamp(0.0, 1.0) * WIDTH) as usize;
        for y in 0..ICON as usize {
            let mut x = 0;
            while x < n {
                let free = (x..n).take_while(|x| !on_icon(a, *x, y)).count();
                if free > 0 {
                    let left = WIDTH - (x + free) as f32;
                    let from = Rect { x: left, y: BAR + y as f32, w: free as f32, h: 1.0 };
                    put(left - WIDTH, y as f32, from, 0.75);
                }
                x += free.max(1);
            }
        }
    }
    let Some(s) = show.saturation.filter(|s| s.is_finite()) else { return };
    let s = s.clamp(0.0, FULL);
    for i in 0..(s / 2.0).ceil() as usize {
        let from = Rect { x: quarter(s, i) * ICON, y: 0.0, w: ICON, h: ICON };
        put(-(i as f32) * STEP - ICON, 0.0, from, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icons(show: Show) -> Vec<(Rect, Rect)> {
        let mut out = vec![];
        draw(1551.0, 1317.0, 3.0, &show, &mut out);
        let icon = |d: &Draw| if let Draw::Icon { rect, from, .. } = d { Some((*rect, *from)) } else { None };
        out.iter().filter_map(icon).collect()
    }

    #[test]
    fn bilddatei_ist_lesbar() {
        let a = atlas().unwrap();
        assert_eq!((a.width, a.height), (256, 256));
        assert!(!a.solid(27, 0) && a.solid(1, 18) && !a.solid(0, 18));
    }

    #[test]
    fn viertel_wie_in_appleskin() {
        let q = |s| {
            let all = icons(Show { saturation: Some(s), exhaustion: None });
            all.iter().map(|i| i.1.x / 9.0).collect::<Vec<_>>()
        };
        assert!(q(0.0).is_empty());
        assert_eq!(q(0.4), [0.0]);
        assert_eq!(q(0.8), [1.0]);
        assert_eq!(q(1.5), [2.0]);
        assert_eq!(q(5.0), [3.0, 3.0, 1.0]);
        assert_eq!(q(99.0), [3.0; 10]);
    }

    #[test]
    fn balken_ist_rechts_buendig_und_spart_keulen_aus() {
        assert!(icons(Show { saturation: None, exhaustion: Some(0.0) }).is_empty());
        let half = icons(Show { saturation: None, exhaustion: Some(2.0) });
        assert!(half.iter().all(|(r, _)| r.x >= 1551.0 - 120.0 && r.x + r.w <= 1551.0));
        // Jeder Ausschnitt liegt an seiner Stelle im Balken.
        assert!(half.iter().all(|(r, f)| (r.x - 1551.0) / 3.0 + 81.0 == f.x && f.y >= 18.0 && f.y < 27.0));
        let a = atlas().unwrap();
        // Keule Mitte und Ecke rechts oben des rechten Symbols.
        assert!(on_icon(a, 5, 3) && !on_icon(a, 0, 0));
        assert!(icons(Show { saturation: Some(f32::NAN), exhaustion: Some(f32::INFINITY) }).is_empty());
    }
}
