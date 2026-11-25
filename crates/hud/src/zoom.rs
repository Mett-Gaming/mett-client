//! Zoom mit dem Verhalten von Zoomify (Werte und Verlauf nachgelesen, eigene Umsetzung): 20 Takte je Sekunde, dazwischen
//! wird gemittelt. Ohne Windows und ohne Spiel.
use serde::{Deserialize, Serialize};
use std::f64::consts::{LN_2, PI};

/// Ein Takt in Sekunden.
pub const TICK: f64 = 0.05;
const SMALLEST: f64 = 1.0;
const BIGGEST: f64 = 500.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transition {
    Instant,
    Linear,
    InSine,
    OutSine,
    InOutSine,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InExp,
    #[default]
    OutExp,
    InOutExp,
}

use Transition::*;

impl Transition {
    pub const ALL: [Transition; 14] = [
        Instant, Linear, InSine, OutSine, InOutSine, InQuad, OutQuad, InOutQuad, InCubic, OutCubic, InOutCubic, InExp,
        OutExp, InOutExp,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Instant => "Sofort",
            Linear => "Linear",
            InSine => "Sinus hinein",
            OutSine => "Sinus hinaus",
            InOutSine => "Sinus beides",
            InQuad => "Quadrat hinein",
            OutQuad => "Quadrat hinaus",
            InOutQuad => "Quadrat beides",
            InCubic => "Kubisch hinein",
            OutCubic => "Kubisch hinaus",
            InOutCubic => "Kubisch beides",
            InExp => "Exponentiell hinein",
            OutExp => "Exponentiell hinaus",
            InOutExp => "Exponentiell beides",
        }
    }

    pub fn apply(self, t: f64) -> f64 {
        match self {
            Instant | Linear => t,
            InSine => 1.0 - (t * PI / 2.0).cos(),
            OutSine => (t * PI / 2.0).sin(),
            InOutSine => -((PI * t).cos() - 1.0) / 2.0,
            InQuad => t * t,
            OutQuad => 1.0 - (1.0 - t).powi(2),
            InOutQuad if t < 0.5 => 2.0 * t * t,
            InOutQuad => 1.0 - (2.0 - 2.0 * t).powi(2) / 2.0,
            InCubic => t.powi(3),
            OutCubic => 1.0 - (1.0 - t).powi(3),
            InOutCubic if t < 0.5 => 4.0 * t.powi(3),
            InOutCubic => 1.0 - (2.0 - 2.0 * t).powi(3) / 2.0,
            InExp => ((10.0 * t).exp2() - 1.0) / 1023.0,
            OutExp => 1.0 - InExp.apply(1.0 - t),
            InOutExp if t < 0.5 => InExp.apply(2.0 * t) / 2.0,
            InOutExp => 1.0 - InExp.apply(2.0 - 2.0 * t) / 2.0,
        }
    }

    /// Umkehrung, wo es sie gibt: So geht es beim Umdrehen ohne Sprung weiter.
    fn inverse(self, x: f64) -> Option<f64> {
        let x = x.clamp(0.0, 1.0);
        Some(match self {
            InSine => (1.0 - x).acos() * 2.0 / PI,
            OutSine => x.asin() * 2.0 / PI,
            InQuad => x.sqrt(),
            OutQuad => 1.0 - (1.0 - x).sqrt(),
            InCubic => x.cbrt(),
            OutCubic => 1.0 - (1.0 - x).cbrt(),
            InExp => (1023.0 * x + 1.0).ln() / (10.0 * LN_2),
            OutExp => 1.0 - (1023.0 * (1.0 - x) + 1.0).ln() / (10.0 * LN_2),
            _ => return None,
        })
    }

    /// Beim Herauszoomen läuft die Kurve rückwärts.
    fn opposite(self) -> Self {
        match self {
            InSine => OutSine,
            OutSine => InSine,
            InQuad => OutQuad,
            OutQuad => InQuad,
            InCubic => OutCubic,
            OutCubic => InCubic,
            InExp => OutExp,
            OutExp => InExp,
            t => t,
        }
    }
}

/// Einstellungen mit Zoomifys Vorgaben.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Windows-Tastennummer, Vorgabe C.
    pub key: u8,
    /// Umschalten statt halten.
    pub toggle: bool,
    pub initial: f32,
    pub time_in: f32,
    pub time_out: f32,
    pub ease_in: Transition,
    pub ease_out: Transition,
    pub scroll: bool,
    pub steps: i32,
    /// Prozent je Stufe.
    pub per_step: i32,
    /// Prozent.
    pub smoothness: i32,
    pub retain: bool,
    /// Prozent, um die die Maus mit dem Zoom langsamer wird.
    pub sensitivity: i32,
    /// Film-Kamera, 0 aus, sonst Prozent.
    pub cinematic: i32,
    /// Stufen auch mit zwei Tasten.
    pub scroll_keys: bool,
    pub key_in: u8,
    pub key_out: u8,
    /// Zweiter, langsamer Zoom zum Umschalten.
    pub second: bool,
    pub second_key: u8,
    pub second_amount: f32,
    pub second_in: f32,
    pub second_out: f32,
    pub second_hides_hud: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            key: b'C',
            toggle: false,
            initial: 4.0,
            time_in: 1.0,
            time_out: 0.5,
            ease_in: OutExp,
            ease_out: OutExp,
            scroll: true,
            steps: 10,
            per_step: 150,
            smoothness: 70,
            retain: false,
            sensitivity: 100,
            cinematic: 0,
            scroll_keys: false,
            // Wie bei Zoomify anfangs ohne Taste; der Zweit-Zoom auf Wunsch des Nutzers auch.
            key_in: 0,
            key_out: 0,
            second: true,
            second_key: 0,
            second_amount: 4.0,
            second_in: 10.0,
            second_out: 1.0,
            second_hides_hud: true,
        }
    }
}

impl Settings {
    /// Unsinnige Werte aus einer Datei in brauchbare Grenzen.
    pub fn sane(mut self) -> Self {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.initial = f(self.initial, 1.0, 50.0, 4.0);
        self.time_in = f(self.time_in, 0.0, 10.0, 1.0);
        self.time_out = f(self.time_out, 0.0, 10.0, 0.5);
        self.steps = self.steps.clamp(0, 50);
        self.per_step = self.per_step.clamp(100, 500);
        self.smoothness = self.smoothness.clamp(0, 100);
        self.sensitivity = self.sensitivity.clamp(0, 100);
        self.cinematic = self.cinematic.clamp(0, 200);
        self.second_amount = f(self.second_amount, 1.0, 50.0, 4.0);
        self.second_in = f(self.second_in, 0.0, 30.0, 10.0);
        self.second_out = f(self.second_out, 0.0, 30.0, 1.0);
        self
    }

    /// Nach unten geht es höchstens bis zum normalen Blick.
    pub fn least_steps(&self) -> i32 {
        let step = self.per_step as f64 / 100.0;
        if step <= 1.0 {
            return 0;
        }
        -(((self.initial as f64).ln() / step.ln()).ceil() as i32).clamp(0, self.steps)
    }

    /// Faktor für die Mausbewegung.
    pub fn mouse(&self, divisor: f64) -> f64 {
        1.0 / (1.0 + (divisor - 1.0) * self.sensitivity as f64 / 100.0)
    }

    /// Nur Zahlen, für den Auftrag ans Plugin. Tasten gehen einen anderen Weg.
    pub fn encode(&self) -> String {
        let n = |t: Transition| Transition::ALL.iter().position(|a| *a == t).unwrap_or(0) as f32;
        let b = |v: bool| v as u8 as f32;
        let list = [
            b(self.toggle),
            self.initial,
            self.time_in,
            self.time_out,
            n(self.ease_in),
            n(self.ease_out),
            self.steps as f32,
            self.per_step as f32,
            self.smoothness as f32,
            b(self.retain),
            self.sensitivity as f32,
            self.cinematic as f32,
            b(self.second),
            self.second_amount,
            self.second_in,
            self.second_out,
            b(self.second_hides_hud),
        ];
        list.map(|v| v.to_string()).join(" ")
    }

    pub fn decode(s: &str) -> Option<Self> {
        let v: Vec<f32> =
            s.split(' ').map(|t| t.parse().ok().filter(|v: &f32| v.is_finite())).collect::<Option<_>>()?;
        let [toggle, initial, ti, to, ei, eo, steps, per, smooth, retain, sens, film, second, sa, si, so, sh] = v[..]
        else {
            return None;
        };
        let flag = |v: f32| (v == 0.0 || v == 1.0).then_some(v == 1.0);
        let ease = |v: f32| (v >= 0.0).then(|| Transition::ALL.get(v as usize).copied()).flatten();
        let s = Self {
            toggle: flag(toggle)?,
            initial,
            time_in: ti,
            time_out: to,
            ease_in: ease(ei)?,
            ease_out: ease(eo)?,
            steps: steps as i32,
            per_step: per as i32,
            smoothness: smooth as i32,
            retain: flag(retain)?,
            sensitivity: sens as i32,
            cinematic: film as i32,
            second: flag(second)?,
            second_amount: sa,
            second_in: si,
            second_out: so,
            second_hides_hud: flag(sh)?,
            ..Self::default()
        };
        Some(s.sane())
    }
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + (b - a) * t
}

/// Verlauf des Zooms. Je Takt `tick`, je Bild `divisor`.
#[derive(Clone, Debug)]
pub struct Curve {
    /// Fortschritt hinein, jetzt und im Takt davor.
    t: f64,
    before: f64,
    active: Transition,
    target: f64,
    smooth: bool,
    was: bool,
    /// Mausrad, -1 bis 1.
    scroll: f64,
    scroll_before: f64,
    last_steps: i32,
    /// Losgelassen nach dem Mausrad: von dort aus zurück, wo der Zoom gerade stand.
    resetting: bool,
    from: f64,
}

impl Default for Curve {
    fn default() -> Self {
        Self {
            t: 0.0,
            before: 0.0,
            active: OutExp,
            target: 0.0,
            smooth: true,
            was: false,
            scroll: 0.0,
            scroll_before: 0.0,
            last_steps: 0,
            resetting: false,
            from: 1.0,
        }
    }
}

impl Curve {
    pub fn tick(&mut self, s: &Settings, zooming: bool, steps: i32) {
        if zooming && !self.was {
            self.resetting = false;
        }
        self.was = zooming;
        let target = if zooming { 1.0 } else { 0.0 };
        self.before = self.t;
        let mut now = self.t;
        let time = if target > now { s.time_in } else { s.time_out } as f64;
        if target != now {
            let (new, old) = match target > now {
                true => (s.ease_in, s.ease_out.opposite()),
                false => (s.ease_out.opposite(), s.ease_in),
            };
            // Umgedreht: dieselbe Vergrößerung auf der anderen Kurve suchen.
            if self.target != target && new.inverse(0.0).is_some() {
                let same = |v: f64| new.inverse(old.apply(v)).unwrap_or(v);
                now = same(now);
                self.before = same(self.before);
            }
            self.active = new;
        }
        self.target = target;
        self.smooth = self.active != Instant && time > 0.0;
        if self.smooth {
            let by = TICK / time;
            self.t = if target > now { (now + by).min(target) } else { (now - by).max(target) };
        } else {
            self.t = target;
            self.before = target;
        }
        // Rundungsreste nicht ewig mitschleppen.
        if self.t < 1e-9 {
            self.t = 0.0;
        }

        if steps.abs() > self.last_steps.abs() {
            self.resetting = false;
        }
        self.last_steps = steps;
        let aim = if s.steps > 0 { steps as f64 / s.steps as f64 } else { 0.0 };
        self.scroll_before = self.scroll;
        let speed = lerp(s.smoothness as f64 / 100.0, 1.0, 0.1);
        self.scroll += (aim - self.scroll) * speed;
        if speed >= 1.0 {
            self.scroll_before = self.scroll;
        }
    }

    /// Vergrößerung zwischen zwei Takten; `part` von 0 bis 1. `zooming` wie beim Takt.
    pub fn divisor(&mut self, s: &Settings, zooming: bool, part: f64) -> f64 {
        if !zooming && !self.resetting && self.scroll != 0.0 {
            self.resetting = true;
            (self.scroll, self.scroll_before) = (0.0, 0.0);
        }
        let t = if self.smooth { self.active.apply(lerp(part, self.before, self.t)) } else { self.t };
        let full = if self.resetting { self.from } else { 1.0 / s.initial as f64 };
        let base = 1.0 / lerp(t, 1.0, full);
        let scroll = if self.resetting { 0.0 } else { lerp(part, self.scroll_before, self.scroll) };
        let d = (base * (s.per_step as f64 / 100.0).powf(scroll * s.steps as f64)).clamp(SMALLEST, BIGGEST);
        if self.t == 0.0 && self.scroll == 0.0 {
            self.resetting = false;
        }
        if !self.resetting {
            self.from = 1.0 / d;
        }
        d
    }

    /// Nichts mehr zu tun?
    pub fn idle(&self) -> bool {
        !self.was && self.t == 0.0 && self.before == 0.0 && self.scroll == 0.0
    }
}

/// Zweit-Zoom: gleichmäßig in Sekunden, ohne Mausrad.
#[derive(Clone, Debug, Default)]
pub struct Second {
    t: f64,
    before: f64,
}

impl Second {
    pub fn tick(&mut self, s: &Settings, on: bool) {
        self.before = self.t;
        let time = if on { s.second_in } else { s.second_out } as f64;
        let by = if time > 0.0 { TICK / time } else { 1.0 };
        self.t = if on { (self.t + by).min(1.0) } else { (self.t - by).max(0.0) };
        if time <= 0.0 {
            self.before = self.t;
        }
    }

    pub fn divisor(&self, s: &Settings, part: f64) -> f64 {
        1.0 / lerp(lerp(part, self.before, self.t), 1.0, 1.0 / s.second_amount as f64)
    }

    pub fn idle(&self) -> bool {
        self.t == 0.0 && self.before == 0.0
    }
}

/// Film-Kamera wie in Java: Die Bewegung wird gesammelt und läuft weich nach.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Smooth {
    target: f64,
    reached: f64,
    last: f64,
}

impl Smooth {
    pub const NEW: Smooth = Smooth { target: 0.0, reached: 0.0, last: 0.0 };

    pub fn next(&mut self, delta: f64, speed: f64) -> f64 {
        self.target += delta;
        let mut way = self.target - self.reached;
        let half = lerp(0.5, self.last, way);
        if way.abs() > self.last.abs() {
            way = half;
        }
        self.last = half;
        self.reached += way * speed;
        way * speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, e: f64) -> bool {
        (a - b).abs() < e
    }

    #[test]
    fn kurven_gehen_von_null_bis_eins_und_zurueck() {
        for t in Transition::ALL {
            assert!(near(t.apply(0.0), 0.0, 1e-9) && near(t.apply(1.0), 1.0, 1e-9), "{t:?}");
            for i in 1..20 {
                let x = i as f64 / 20.0;
                if let Some(back) = t.inverse(t.apply(x)) {
                    assert!(near(back, x, 1e-6), "{t:?} {x} {back}");
                }
                // Die Gegenkurve läuft spiegelbildlich.
                let m = 1.0 - t.opposite().apply(1.0 - x);
                assert!(
                    !matches!(t, InSine | OutSine | InQuad | OutQuad | InCubic | OutCubic | InExp | OutExp)
                        || near(m, t.apply(x), 1e-9)
                );
            }
        }
        // Bekannte Werte: Exponentiell hinaus ist nach einem Fünftel schon bei gut drei Vierteln.
        assert!(near(OutExp.apply(0.2), 0.7507, 0.001));
        assert!(near(InExp.apply(0.5), 0.0303, 0.001));
    }

    #[test]
    fn vierfach_in_einer_sekunde() {
        let s = Settings::default();
        let mut c = Curve::default();
        let mut last = 1.0;
        for _ in 0..20 {
            c.tick(&s, true, 0);
            for part in [0.0, 0.5, 1.0] {
                let d = c.divisor(&s, true, part);
                assert!(d >= last - 1e-9, "{d} {last}");
                last = d;
            }
        }
        assert!(near(last, 4.0, 1e-9));
        // Hinaus in einer halben Sekunde, ohne Sprung.
        for i in 0..10 {
            c.tick(&s, false, 0);
            let d = c.divisor(&s, false, 0.0);
            assert!(d <= last + 1e-6 && (i > 0 || near(d, 4.0, 1e-6)), "{i} {d} {last}");
            last = d;
        }
        assert!(near(c.divisor(&s, false, 1.0), 1.0, 1e-9));
        c.tick(&s, false, 0);
        assert!(c.idle());
    }

    #[test]
    fn umdrehen_mittendrin_springt_nicht() {
        let s = Settings::default();
        for turn in 1..19 {
            let mut c = Curve::default();
            (0..turn).for_each(|_| c.tick(&s, true, 0));
            let before = c.divisor(&s, true, 1.0);
            c.tick(&s, false, 0);
            let after = c.divisor(&s, false, 0.0);
            assert!(near(before, after, 1e-6), "{turn} {before} {after}");
            (0..3).for_each(|_| c.tick(&s, false, 0));
            let before = c.divisor(&s, false, 1.0);
            c.tick(&s, true, 0);
            let after = c.divisor(&s, true, 0.0);
            assert!(near(before, after, 1e-6), "{turn} zurück {before} {after}");
        }
    }

    #[test]
    fn mausrad_in_gleichen_stufen() {
        let s = Settings::default();
        assert_eq!(s.least_steps(), -4);
        let mut c = Curve::default();
        (0..200).for_each(|_| c.tick(&s, true, 3));
        assert!(near(c.divisor(&s, true, 1.0), 4.0 * 1.5f64.powi(3), 1e-3));
        (0..200).for_each(|_| c.tick(&s, true, -4));
        assert!(near(c.divisor(&s, true, 1.0), 1.0, 1e-3));
        // Loslassen nach dem Mausrad: vom gezeigten Zoom aus zurück, ohne Sprung.
        (0..200).for_each(|_| c.tick(&s, true, 3));
        let before = c.divisor(&s, true, 1.0);
        let free = c.divisor(&s, false, 1.0);
        assert!(near(before, free, 1e-3), "{before} {free}");
        let mut last = free;
        for _ in 0..11 {
            c.tick(&s, false, 0);
            let d = c.divisor(&s, false, 1.0);
            assert!(d <= last + 1e-6);
            last = d;
        }
        assert!(near(last, 1.0, 1e-6));
    }

    #[test]
    fn sofort_und_ohne_zeit() {
        for s in [
            Settings { ease_in: Instant, ease_out: Instant, ..Settings::default() },
            Settings { time_in: 0.0, time_out: 0.0, ..Settings::default() },
        ] {
            let mut c = Curve::default();
            c.tick(&s, true, 0);
            assert_eq!(c.divisor(&s, true, 0.0), 4.0);
            c.tick(&s, false, 0);
            assert_eq!(c.divisor(&s, false, 0.0), 1.0);
        }
    }

    #[test]
    fn maus_wird_mit_dem_zoom_langsamer() {
        let s = Settings::default();
        assert_eq!((s.mouse(1.0), s.mouse(4.0)), (1.0, 0.25));
        assert_eq!(Settings { sensitivity: 0, ..s }.mouse(4.0), 1.0);
        assert_eq!(Settings { sensitivity: 50, ..s }.mouse(4.0), 0.4);
    }

    #[test]
    fn auftrag_hin_und_zurueck() {
        let s = Settings {
            toggle: true,
            ease_out: InOutSine,
            initial: 6.5,
            cinematic: 80,
            second: false,
            ..Settings::default()
        };
        assert_eq!(Settings::decode(&s.encode()), Some(s));
        for bad in [
            "",
            "1 2 3",
            &format!("{} 1", s.encode()),
            &s.encode().replacen("1 6.5", "2 6.5", 1),
            &s.encode().replace("6.5", "nan"),
        ] {
            assert_eq!(Settings::decode(bad), None, "{bad}");
        }
        let wild = Settings { initial: f32::NAN, steps: -5, per_step: 9, ..s }.sane();
        assert_eq!((wild.initial, wild.steps, wild.per_step), (4.0, 0, 100));
    }

    #[test]
    fn zweiter_zoom_laeuft_gleichmaessig() {
        let s = Settings::default();
        let mut c = Second::default();
        (0..100).for_each(|_| c.tick(&s, true));
        // Nach 5 von 10 Sekunden die Hälfte des Wegs.
        assert!(near(c.divisor(&s, 1.0), 1.0 / 0.625, 1e-6));
        (0..100).for_each(|_| c.tick(&s, true));
        assert!(near(c.divisor(&s, 1.0), 4.0, 1e-6));
        (0..21).for_each(|_| c.tick(&s, false));
        assert!(c.divisor(&s, 1.0) == 1.0 && c.idle());
    }

    #[test]
    fn film_kamera_verliert_keine_bewegung() {
        let mut m = Smooth::default();
        let mut sum = m.next(100.0, 0.05);
        assert!(sum < 10.0);
        for _ in 0..2000 {
            sum += m.next(0.0, 0.05);
        }
        assert!(near(sum, 100.0, 0.01), "{sum}");
    }
}
