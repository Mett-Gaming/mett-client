//! Kleine Bedienelemente für das Einstellungsmenü. Werden in jedem Bild neu berechnet.
use crate::{Align, Draw, Rect, rgb, theme};

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub mouse: (f32, f32),
    pub down: bool,
    /// Nur in dem Bild gesetzt, in dem die linke Taste gedrückt wurde.
    pub pressed: bool,
    pub right_pressed: bool,
    /// Rasten des Mausrads, oben positiv.
    pub wheel: f32,
}

pub struct Ui<'a> {
    pub input: &'a Input,
    /// Gerade gezogener Regler.
    pub active: &'a mut Option<u32>,
    pub out: &'a mut Vec<Draw>,
    pub scale: f32,
}

impl Ui<'_> {
    fn hover(&self, r: Rect) -> bool {
        self.active.is_none() && r.contains(self.input.mouse)
    }

    fn clicked(&self, r: Rect) -> bool {
        self.input.pressed && r.contains(self.input.mouse)
    }

    pub fn panel(&mut self, rect: Rect) {
        let radius = 10.0 * self.scale;
        self.out.push(Draw::Box { rect, radius, color: rgb(theme::APP, 0.97) });
        self.out.push(Draw::Outline { rect, radius, width: self.scale, color: rgb(theme::BORDER, 1.0) });
    }

    /// Halbdurchsichtige Fläche des Menüs.
    pub fn glass(&mut self, rect: Rect) {
        let radius = 4.0 * self.scale;
        self.out.push(Draw::Box { rect, radius, color: rgb(theme::APP, 0.58) });
        self.out.push(Draw::Outline { rect, radius, width: self.scale, color: rgb(theme::BORDER, 0.9) });
    }

    pub fn fill(&mut self, rect: Rect, color: u32, alpha: f32) {
        self.out.push(Draw::Box { rect, radius: 0.0, color: rgb(color, alpha) });
    }

    /// Reiter: ohne Fläche, aktiv mit Rahmen.
    pub fn tab(&mut self, rect: Rect, text: &str, on: bool) -> bool {
        let radius = 4.0 * self.scale;
        let alpha = match (on, self.hover(rect)) {
            (true, _) => 0.45,
            (false, true) => 0.2,
            (false, false) => 0.0,
        };
        self.out.push(Draw::Box { rect, radius, color: rgb(theme::ACCENT, alpha) });
        if on {
            let width = 2.0 * self.scale;
            self.out.push(Draw::Outline { rect, radius, width, color: rgb(theme::ACCENT_HOVER, 1.0) });
        }
        self.text(rect, 14.0, theme::TEXT, Align::Center, text);
        self.clicked(rect)
    }

    /// Eingabefeld; liefert `true` beim Anklicken.
    pub fn field(&mut self, rect: Rect, text: &str, on: bool) -> bool {
        let radius = 4.0 * self.scale;
        self.out.push(Draw::Box { rect, radius, color: rgb(theme::APP, 0.6) });
        let line = if on { theme::ACCENT_HOVER } else { theme::BORDER };
        self.out.push(Draw::Outline { rect, radius, width: 2.0 * self.scale, color: rgb(line, 1.0) });
        let pad = 10.0 * self.scale;
        let inner = Rect { x: rect.x + pad, w: rect.w - 2.0 * pad, ..rect };
        self.text(inner, 13.0, if on { theme::TEXT } else { theme::MUTED }, Align::Left, text);
        self.clicked(rect)
    }

    /// Eintrag der Seitenleiste: Symbol über Text.
    pub fn rail_item(&mut self, rect: Rect, icon: &str, text: &str, on: bool) -> bool {
        let s = self.scale;
        if on || self.hover(rect) {
            let alpha = if on { 0.35 } else { 0.18 };
            self.out.push(Draw::Box { rect, radius: 4.0 * s, color: rgb(theme::ACCENT, alpha) });
        }
        let top = Rect { h: rect.h * 0.65, ..rect };
        let bottom = Rect { y: rect.y + rect.h * 0.62, h: rect.h * 0.3, ..rect };
        self.text(top, 26.0, theme::TEXT, Align::Center, icon);
        self.text(bottom, 10.0, theme::TEXT, Align::Center, text);
        self.clicked(rect)
    }

    /// Modulkarte; eingeschaltete sind heller.
    pub fn card(&mut self, rect: Rect, icon: &str, title: &str, text: &str, on: bool) {
        let s = self.scale;
        let radius = 4.0 * s;
        let hover = if self.hover(rect) { 0.12 } else { 0.0 };
        let (color, alpha) = if on { (theme::ACCENT, 0.4) } else { (theme::RAISED, 0.6) };
        self.out.push(Draw::Box { rect, radius, color: rgb(color, alpha + hover) });
        let line = if on { theme::ACCENT_HOVER } else { theme::BORDER };
        self.out.push(Draw::Outline { rect, radius, width: 2.0 * s, color: rgb(line, 0.9) });
        let left = Rect { x: rect.x + 14.0 * s, y: rect.y + 20.0 * s, w: 52.0 * s, h: 52.0 * s };
        self.text(left, 28.0, if on { theme::TEXT } else { theme::MUTED }, Align::Center, icon);
        let x = rect.x + 78.0 * s;
        let w = rect.w - 78.0 * s - 52.0 * s;
        let title_at = Rect { x, y: rect.y + 16.0 * s, w, h: 24.0 * s };
        self.text(title_at, 15.0, if on { theme::TEXT } else { theme::MUTED }, Align::Left, title);
        let text_at = Rect { x, y: rect.y + 40.0 * s, w, h: 38.0 * s };
        self.text(text_at, 12.0, theme::MUTED, Align::Left, text);
    }

    /// Regler mit Knopf zum Zurücksetzen.
    #[allow(clippy::too_many_arguments)]
    pub fn slider_reset(
        &mut self,
        id: u32,
        rect: Rect,
        text: &str,
        value: &mut f32,
        range: (f32, f32),
        step: f32,
        standard: f32,
    ) -> bool {
        let s = self.scale;
        let x = Rect { x: rect.x + rect.w - 28.0 * s, y: rect.y + 4.0 * s, w: 28.0 * s, h: 24.0 * s };
        let reset = self.button(x, "×", false);
        if reset {
            *value = standard;
        }
        self.slider(id, Rect { w: rect.w - 40.0 * s, ..rect }, text, value, range, step);
        reset
    }

    /// Auswahl mit Pfeilen; liefert die neue Wahl. Ohne passende Wahl steht „Eigener“ da.
    pub fn stepper(&mut self, rect: Rect, text: &str, names: &[&str], now: Option<usize>) -> Option<usize> {
        let s = self.scale;
        self.text(rect, 13.0, theme::TEXT, Align::Left, text);
        let b = Rect { x: rect.x + rect.w - 220.0 * s, w: 220.0 * s, ..rect };
        let left = Rect { w: 32.0 * s, ..b };
        let right = Rect { x: b.x + b.w - 32.0 * s, ..left };
        self.field(b, "", false);
        let shown = now.and_then(|i| names.get(i)).copied().unwrap_or("Eigener");
        self.text(b, 13.0, theme::TEXT, Align::Center, shown);
        let n = names.len();
        if self.tab(left, "‹", false) {
            return Some(now.map_or(n - 1, |i| (i + n - 1) % n));
        }
        if self.tab(right, "›", false) {
            return Some(now.map_or(0, |i| (i + 1) % n));
        }
        None
    }

    pub fn text(&mut self, rect: Rect, size: f32, color: u32, align: Align, text: impl Into<String>) {
        let size = size * self.scale;
        self.out.push(Draw::Text { rect, size, color: rgb(color, 1.0), align, text: text.into() });
    }

    /// Knopf; `on` hebt ihn hervor.
    pub fn button(&mut self, rect: Rect, text: &str, on: bool) -> bool {
        let color = match (on, self.hover(rect)) {
            (true, false) => theme::ACCENT,
            (true, true) => theme::ACCENT_HOVER,
            (false, true) => theme::HOVER,
            (false, false) => theme::RAISED,
        };
        self.out.push(Draw::Box { rect, radius: 6.0 * self.scale, color: rgb(color, 1.0) });
        self.text(rect, 13.0, if on { theme::APP } else { theme::TEXT }, Align::Center, text);
        self.clicked(rect)
    }

    /// Schalter mit Beschriftung links; liefert `true`, wenn umgeschaltet wurde.
    pub fn toggle(&mut self, rect: Rect, text: &str, value: &mut bool) -> bool {
        let s = self.scale;
        self.text(rect, 13.0, theme::TEXT, Align::Left, text);
        let track =
            Rect { x: rect.x + rect.w - 36.0 * s, y: rect.y + (rect.h - 20.0 * s) / 2.0, w: 36.0 * s, h: 20.0 * s };
        let color = if *value { theme::ACCENT } else { theme::HOVER };
        self.out.push(Draw::Box { rect: track, radius: 10.0 * s, color: rgb(color, 1.0) });
        let x = if *value { track.x + 18.0 * s } else { track.x + 2.0 * s };
        let knob = Rect { x, y: track.y + 2.0 * s, w: 16.0 * s, h: 16.0 * s };
        self.out.push(Draw::Box { rect: knob, radius: 8.0 * s, color: rgb(theme::TEXT, 1.0) });
        let changed = self.clicked(rect);
        if changed {
            *value = !*value;
        }
        changed
    }

    /// Regler mit Beschriftung und Wert.
    pub fn slider(&mut self, id: u32, rect: Rect, text: &str, value: &mut f32, range: (f32, f32), step: f32) -> bool {
        let s = self.scale;
        let track =
            Rect { x: rect.x + rect.w * 0.42, y: rect.y + rect.h / 2.0 - 2.0 * s, w: rect.w * 0.44, h: 4.0 * s };
        let grab = Rect { x: track.x - 8.0 * s, y: rect.y, w: track.w + 16.0 * s, h: rect.h };
        if self.clicked(grab) {
            *self.active = Some(id);
        }
        let mut changed = false;
        if *self.active == Some(id) {
            if self.input.down {
                let t = ((self.input.mouse.0 - track.x) / track.w).clamp(0.0, 1.0);
                let v = ((range.0 + t * (range.1 - range.0)) / step).round() * step;
                changed = v != *value;
                *value = v;
            } else {
                *self.active = None;
            }
        }
        let t = ((*value - range.0) / (range.1 - range.0)).clamp(0.0, 1.0);
        self.text(rect, 13.0, theme::TEXT, Align::Left, text);
        self.out.push(Draw::Box { rect: track, radius: 2.0 * s, color: rgb(theme::HOVER, 1.0) });
        let fill = Rect { w: track.w * t, ..track };
        self.out.push(Draw::Box { rect: fill, radius: 2.0 * s, color: rgb(theme::ACCENT, 1.0) });
        let knob =
            Rect { x: track.x + track.w * t - 7.0 * s, y: rect.y + rect.h / 2.0 - 7.0 * s, w: 14.0 * s, h: 14.0 * s };
        self.out.push(Draw::Box { rect: knob, radius: 7.0 * s, color: rgb(theme::TEXT, 1.0) });
        let shown = if step < 1.0 { format!("{value:.2}") } else { format!("{value:.0}") };
        self.text(rect, 13.0, theme::MUTED, Align::Right, shown);
        changed
    }

    /// Farbauswahl aus festen Feldern.
    pub fn colors(&mut self, rect: Rect, text: &str, value: &mut u32, choices: &[u32]) -> bool {
        let s = self.scale;
        self.text(rect, 13.0, theme::TEXT, Align::Left, text);
        let mut changed = false;
        for (i, c) in choices.iter().enumerate() {
            let x = rect.x + rect.w * 0.42 + i as f32 * 26.0 * s;
            let r = Rect { x, y: rect.y + (rect.h - 20.0 * s) / 2.0, w: 20.0 * s, h: 20.0 * s };
            self.out.push(Draw::Box { rect: r, radius: 4.0 * s, color: rgb(*c, 1.0) });
            let (width, line) = if c == value { (2.0, theme::ACCENT_HOVER) } else { (1.0, theme::BORDER) };
            self.out.push(Draw::Outline { rect: r, radius: 4.0 * s, width: width * s, color: rgb(line, 1.0) });
            if self.clicked(r) && c != value {
                *value = *c;
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rect = Rect { x: 0.0, y: 0.0, w: 100.0, h: 20.0 };

    fn run<T>(input: Input, active: &mut Option<u32>, f: impl FnOnce(&mut Ui) -> T) -> T {
        f(&mut Ui { input: &input, active, out: &mut vec![], scale: 1.0 })
    }

    #[test]
    fn regler_folgt_der_maus_auch_ausserhalb_und_laesst_los() {
        let mut a = None;
        let mut v = 0.0;
        let at = |x, pressed, down| Input { mouse: (x, 10.0), pressed, down, ..Default::default() };
        run(at(64.0, true, true), &mut a, |u| u.slider(1, R, "", &mut v, (0.0, 10.0), 1.0));
        assert_eq!(v, 5.0);
        run(at(999.0, false, true), &mut a, |u| u.slider(1, R, "", &mut v, (0.0, 10.0), 1.0));
        assert_eq!(v, 10.0);
        run(at(0.0, false, false), &mut a, |u| u.slider(1, R, "", &mut v, (0.0, 10.0), 1.0));
        assert_eq!((v, a), (10.0, None));
    }

    #[test]
    fn schalter_reagiert_nur_auf_klick_im_feld() {
        let mut a = None;
        let mut v = false;
        let at = |x, pressed| Input { mouse: (x, 10.0), pressed, ..Default::default() };
        run(at(10.0, false), &mut a, |u| u.toggle(R, "", &mut v));
        run(at(500.0, true), &mut a, |u| u.toggle(R, "", &mut v));
        assert!(!v);
        assert!(run(at(10.0, true), &mut a, |u| u.toggle(R, "", &mut v)));
        assert!(v);
    }

    #[test]
    fn auswahl_laeuft_im_kreis() {
        let mut a = None;
        let r = Rect { x: 0.0, y: 0.0, w: 400.0, h: 30.0 };
        let mut at = |x, now| {
            let i = Input { mouse: (x, 10.0), pressed: true, ..Default::default() };
            run(i, &mut a, |u| u.stepper(r, "", &["a", "b", "c"], now))
        };
        assert_eq!(at(390.0, Some(2)), Some(0));
        assert_eq!(at(190.0, Some(0)), Some(2));
        assert_eq!(at(390.0, None), Some(0));
        assert_eq!(at(290.0, Some(1)), None);
    }
}
