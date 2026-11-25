//! Farben umrechnen: Farbton, Sättigung und Helligkeit, jeweils 0 bis 1.

pub fn to_hsv(hex: u32) -> (f32, f32, f32) {
    let c = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
    let (r, g, b) = (c(16), c(8), c(0));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, if max == 0.0 { 0.0 } else { d / max }, max)
}

pub fn from_hsv(h: f32, s: f32, v: f32) -> u32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let f = |n: f32| {
        let k = (n + h).rem_euclid(6.0);
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    c(f(5.0)) << 16 | c(f(3.0)) << 8 | c(f(1.0))
}

/// `t` = 0 ist `a`, 1 ist `b`.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let c = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        ((x + (y - x) * t).round() as u32) << shift
    };
    c(16) | c(8) | c(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hin_und_zurueck() {
        for c in [0, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xE8E0D1, 0x120E0B, 0x9AA36E, 0xD2615A, 0x8D97A1] {
            let (h, s, v) = to_hsv(c);
            assert_eq!(from_hsv(h, s, v), c, "{c:06x}");
        }
        assert_eq!(from_hsv(1.0, 1.0, 1.0), 0xFF0000);
    }

    #[test]
    fn mischen() {
        assert_eq!(mix(0x102030, 0x304050, 0.0), 0x102030);
        assert_eq!(mix(0x102030, 0x304050, 1.0), 0x304050);
        assert_eq!(mix(0x000000, 0x646464, 0.5), 0x323232);
    }
}
