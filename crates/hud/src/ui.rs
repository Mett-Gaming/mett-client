//! Kleine Bedienelemente für das Einstellungsmenü. Werden in jedem Bild neu berechnet.
use crate::icons::{self, Icon};
use crate::{Align, Draw, Rect, color, lang, rgb, theme};
use serde::{Deserialize, Serialize};

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

/// Aussehen des Menüfensters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub background: u32,
    pub opacity: f32,
    pub card: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub radius: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            background: theme::APP,
            opacity: 0.58,
            card: theme::RAISED,
            border: theme::BORDER,
            text: theme::TEXT,
            muted: theme::MUTED,
            accent: theme::ACCENT,
            radius: 4.0,
        }
    }
}

impl Look {
    /// Hervorgehobener Akzent.
    pub fn bright(&self) -> u32 {
        if self.accent == theme::ACCENT { theme::ACCENT_HOVER } else { color::mix(self.accent, 0xFFFFFF, 0.25) }
    }

    pub fn hover(&self) -> u32 {
        if self.card == theme::RAISED { theme::HOVER } else { color::mix(self.card, self.text, 0.08) }
    }

    /// Schrift auf der Akzentfarbe.
    fn on_accent(&self) -> u32 {
        let (_, _, v) = color::to_hsv(self.accent);
        if v > 0.45 { color::mix(self.background, 0, 0.3) } else { self.text }
    }
}

pub struct Ui<'a> {
    pub look: Look,
    /// Nichts reagiert.
    pub locked: bool,
    pub input: &'a Input,
    /// Gerade gezogener Regler.
    pub active: &'a mut Option<u32>,
    pub out: &'a mut Vec<Draw>,
    pub scale: f32,
}

impl Ui<'_> {
    fn hover(&self, r: Rect) -> bool {
        !self.locked && self.active.is_none() && r.contains(self.input.mouse)
    }

    fn clicked(&self, r: Rect) -> bool {
        !self.locked && self.input.pressed && r.contains(self.input.mouse)
    }

    pub fn panel(&mut self, rect: Rect) {
        let radius = self.look.radius * 2.0 * self.scale;
        self.out.push(Draw::Box { rect, radius, color: rgb(self.look.background, 0.97) });
        self.out.push(Draw::Outline { rect, radius, width: self.scale, color: rgb(self.look.border, 1.0) });
    }

    /// Halbdurchsichtige Fläche des Menüs.
    pub fn glass(&mut self, rect: Rect) {
        let radius = self.look.radius * self.scale;
        self.out.push(Draw::Box { rect, radius, color: rgb(self.look.background, self.look.opacity) });
        self.out.push(Draw::Outline { rect, radius, width: self.scale, color: rgb(self.look.border, 0.9) });
    }

    pub fn fill(&mut self, rect: Rect, color: u32, alpha: f32) {
        self.out.push(Draw::Box { rect, radius: 0.0, color: rgb(color, alpha) });
    }

    /// Reiter: ohne Fläche, aktiv mit Rahmen.
    pub fn tab(&mut self, rect: Rect, text: &str, on: bool) -> bool {
        let radius = self.look.radius * self.scale;
        let alpha = match (on, self.hover(rect)) {
            (true, _) => 0.45,
            (false, true) => 0.2,
            (false, false) => 0.0,
        };
        self.out.push(Draw::Box { rect, radius, color: rgb(self.look.accent, alpha) });
        if on {
            let width = 2.0 * self.scale;
            self.out.push(Draw::Outline { rect, radius, width, color: rgb(self.look.bright(), 1.0) });
        }
        self.text(rect, 14.0, self.look.text, Align::Center, text);
        self.clicked(rect)
    }

    /// Eingabefeld; liefert `true` beim Anklicken.
    pub fn field(&mut self, rect: Rect, text: &str, on: bool) -> bool {
        let radius = self.look.radius * self.scale;
        self.out.push(Draw::Box { rect, radius, color: rgb(self.look.background, 0.6) });
        let line = if on { self.look.bright() } else { self.look.border };
        self.out.push(Draw::Outline { rect, radius, width: 2.0 * self.scale, color: rgb(line, 1.0) });
        let pad = 10.0 * self.scale;
        let inner = Rect { x: rect.x + pad, w: rect.w - 2.0 * pad, ..rect };
        self.raw(inner, 13.0, if on { self.look.text } else { self.look.muted }, Align::Left, text);
        self.clicked(rect)
    }

    /// Eintrag der Seitenleiste: Symbol über Text.
    pub fn rail_item(&mut self, rect: Rect, icon: Icon, text: &str, on: bool) -> bool {
        let s = self.scale;
        if on || self.hover(rect) {
            let alpha = if on { 0.35 } else { 0.18 };
            self.out.push(Draw::Box { rect, radius: self.look.radius * s, color: rgb(self.look.accent, alpha) });
        }
        let top = Rect { h: rect.h * 0.65, ..rect };
        let bottom = Rect { y: rect.y + rect.h * 0.62, h: rect.h * 0.3, ..rect };
        self.icon(top, 30.0, self.look.text, icon);
        self.text(bottom, 10.0, self.look.text, Align::Center, text);
        self.clicked(rect)
    }

    /// Modulkarte; eingeschaltete sind heller.
    pub fn card(&mut self, rect: Rect, icon: Icon, title: &str, text: &str, on: bool) {
        let s = self.scale;
        let radius = self.look.radius * s;
        let hover = if self.hover(rect) { 0.12 } else { 0.0 };
        let (color, alpha) = if on { (self.look.accent, 0.4) } else { (self.look.card, 0.6) };
        self.out.push(Draw::Box { rect, radius, color: rgb(color, alpha + hover) });
        let line = if on { self.look.bright() } else { self.look.border };
        self.out.push(Draw::Outline { rect, radius, width: 2.0 * s, color: rgb(line, 0.9) });
        let left = Rect { x: rect.x + 14.0 * s, y: rect.y + 20.0 * s, w: 52.0 * s, h: 52.0 * s };
        self.icon(left, 34.0, if on { self.look.text } else { self.look.muted }, icon);
        let x = rect.x + 78.0 * s;
        let w = rect.w - 78.0 * s - 52.0 * s;
        let title_at = Rect { x, y: rect.y + 16.0 * s, w, h: 24.0 * s };
        self.text(title_at, 15.0, if on { self.look.text } else { self.look.muted }, Align::Left, title);
        let text_at = Rect { x, y: rect.y + 40.0 * s, w, h: 38.0 * s };
        self.text(text_at, 12.0, self.look.muted, Align::Left, text);
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
        let reset = self.icon_button(x, Icon::Reset);
        if reset {
            *value = standard;
        }
        self.slider(id, Rect { w: rect.w - 40.0 * s, ..rect }, text, value, range, step);
        reset
    }

    /// Auswahl mit Pfeilen; liefert die neue Wahl. Ohne passende Wahl steht „Eigener“ da.
    pub fn stepper(&mut self, rect: Rect, text: &str, names: &[&str], now: Option<usize>) -> Option<usize> {
        let s = self.scale;
        self.text(rect, 13.0, self.look.text, Align::Left, text);
        let b = Rect { x: rect.x + rect.w - 220.0 * s, w: 220.0 * s, ..rect };
        let left = Rect { w: 32.0 * s, ..b };
        let right = Rect { x: b.x + b.w - 32.0 * s, ..left };
        self.field(b, "", false);
        let shown = now.and_then(|i| names.get(i)).copied().unwrap_or("Eigener");
        self.text(b, 13.0, self.look.text, Align::Center, shown);
        let n = names.len();
        if self.icon_button(left, Icon::Previous) {
            return Some(now.map_or(n - 1, |i| (i + n - 1) % n));
        }
        if self.icon_button(right, Icon::Next) {
            return Some(now.map_or(0, |i| (i + 1) % n));
        }
        None
    }

    /// Text in der Sprache des Spiels.
    pub fn text(&mut self, rect: Rect, size: f32, color: u32, align: Align, text: impl Into<String>) {
        let text = text.into();
        self.raw(rect, size, color, align, lang::t(&text));
    }

    /// Text, wie er ist: Eingaben und Namen des Nutzers.
    pub fn raw(&mut self, rect: Rect, size: f32, color: u32, align: Align, text: impl Into<String>) {
        let size = size * self.scale;
        self.out.push(Draw::Text { rect, size, color: rgb(color, 1.0), align, text: text.into() });
    }

    pub fn icon(&mut self, rect: Rect, size: f32, color: u32, icon: Icon) {
        icons::draw(icon, rect, size * self.scale, rgb(color, 1.0), self.out);
    }

    pub fn icon_button(&mut self, rect: Rect, icon: Icon) -> bool {
        if self.hover(rect) {
            let radius = self.look.radius * self.scale;
            self.out.push(Draw::Box { rect, radius, color: rgb(self.look.accent, 0.25) });
        }
        let size = (rect.h / self.scale * 0.6).min(18.0);
        self.icon(rect, size, self.look.text, icon);
        self.clicked(rect)
    }

    /// Knopf; `on` hebt ihn hervor.
    pub fn button(&mut self, rect: Rect, text: &str, on: bool) -> bool {
        let color = match (on, self.hover(rect)) {
            (true, false) => self.look.accent,
            (true, true) => self.look.bright(),
            (false, true) => self.look.hover(),
            (false, false) => self.look.card,
        };
        self.out.push(Draw::Box { rect, radius: self.look.radius * 1.5 * self.scale, color: rgb(color, 1.0) });
        self.text(rect, 13.0, if on { self.look.on_accent() } else { self.look.text }, Align::Center, text);
        self.clicked(rect)
    }

    /// Schalter mit Beschriftung links; liefert `true`, wenn umgeschaltet wurde.
    pub fn toggle(&mut self, rect: Rect, text: &str, value: &mut bool) -> bool {
        let s = self.scale;
        self.text(rect, 13.0, self.look.text, Align::Left, text);
        let track =
            Rect { x: rect.x + rect.w - 36.0 * s, y: rect.y + (rect.h - 20.0 * s) / 2.0, w: 36.0 * s, h: 20.0 * s };
        let color = if *value { self.look.accent } else { self.look.hover() };
        self.out.push(Draw::Box { rect: track, radius: 10.0 * s, color: rgb(color, 1.0) });
        let x = if *value { track.x + 18.0 * s } else { track.x + 2.0 * s };
        let knob = Rect { x, y: track.y + 2.0 * s, w: 16.0 * s, h: 16.0 * s };
        self.out.push(Draw::Box { rect: knob, radius: 8.0 * s, color: rgb(self.look.text, 1.0) });
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
        self.text(rect, 13.0, self.look.text, Align::Left, text);
        self.out.push(Draw::Box { rect: track, radius: 2.0 * s, color: rgb(self.look.hover(), 1.0) });
        let fill = Rect { w: track.w * t, ..track };
        self.out.push(Draw::Box { rect: fill, radius: 2.0 * s, color: rgb(self.look.accent, 1.0) });
        let knob =
            Rect { x: track.x + track.w * t - 7.0 * s, y: rect.y + rect.h / 2.0 - 7.0 * s, w: 14.0 * s, h: 14.0 * s };
        self.out.push(Draw::Box { rect: knob, radius: 7.0 * s, color: rgb(self.look.text, 1.0) });
        let shown = if step < 1.0 { format!("{value:.2}") } else { format!("{value:.0}") };
        self.text(rect, 13.0, self.look.muted, Align::Right, shown);
        changed
    }

    /// Feld mit der aktuellen Farbe; `true` beim Anklicken. Liefert auch, wo es liegt.
    pub fn color(&mut self, rect: Rect, text: &str, value: u32) -> (bool, Rect) {
        let s = self.scale;
        self.text(rect, 13.0, self.look.text, Align::Left, text);
        let at = Rect { x: rect.x + rect.w - 120.0 * s, w: 120.0 * s, ..rect };
        let radius = self.look.radius * s;
        let line = if self.hover(at) { self.look.bright() } else { self.look.border };
        self.out.push(Draw::Box { rect: at, radius, color: rgb(value, 1.0) });
        self.out.push(Draw::Outline { rect: at, radius, width: 2.0 * s, color: rgb(line, 1.0) });
        let (_, sat, v) = color::to_hsv(value);
        let ink = if v > 0.6 && sat < 0.75 { 0x000000 } else { 0xFFFFFF };
        self.raw(at, 12.0, ink, Align::Center, format!("#{value:06X}"));
        (self.clicked(at), at)
    }

    /// Ziehfläche: Lage der Maus darin, 0 bis 1.
    pub fn drag(&mut self, id: u32, rect: Rect) -> Option<(f32, f32)> {
        if self.clicked(rect) {
            *self.active = Some(id);
        }
        if *self.active != Some(id) {
            return None;
        }
        if !self.input.down {
            *self.active = None;
            return None;
        }
        let (x, y) = self.input.mouse;
        Some((((x - rect.x) / rect.w).clamp(0.0, 1.0), ((y - rect.y) / rect.h).clamp(0.0, 1.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rect = Rect { x: 0.0, y: 0.0, w: 100.0, h: 20.0 };

    fn run<T>(input: Input, active: &mut Option<u32>, f: impl FnOnce(&mut Ui) -> T) -> T {
        f(&mut Ui { look: Look::default(), locked: false, input: &input, active, out: &mut vec![], scale: 1.0 })
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
