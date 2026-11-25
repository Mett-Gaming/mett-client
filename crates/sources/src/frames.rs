//! FPS aus den Zeitpunkten der gezeigten Bilder (Einheit: 100 ns).
use std::collections::VecDeque;

const SECOND: i64 = 10_000_000;
const KEEP: i64 = 10 * SECOND;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fps {
    pub now: u32,
    /// Durchschnitt der langsamsten 1 % der letzten zehn Sekunden.
    pub low: u32,
}

#[derive(Default)]
pub struct Frames {
    times: VecDeque<i64>,
}

impl Frames {
    pub fn push(&mut self, t: i64) {
        // Ungeordnete Ereignisse würden negative Bildzeiten ergeben.
        if self.times.back().is_some_and(|last| t <= *last) {
            return;
        }
        self.times.push_back(t);
        while self.times.front().is_some_and(|f| t - f > KEEP) {
            self.times.pop_front();
        }
    }

    pub fn last(&self) -> Option<i64> {
        self.times.back().copied()
    }

    pub fn fps(&self) -> Option<Fps> {
        let last = self.last()?;
        let recent = self.times.iter().rev().take_while(|t| last - **t < SECOND).count();
        let span = last - self.times[self.times.len() - recent];
        if recent < 2 || span == 0 {
            return None;
        }
        let now = ((recent - 1) as i64 * SECOND + span / 2) / span;
        let mut d: Vec<i64> = self.times.iter().zip(self.times.iter().skip(1)).map(|(a, b)| b - a).collect();
        d.sort_unstable_by(|a, b| b.cmp(a));
        let n = d.len().div_ceil(100);
        let sum: i64 = d[..n].iter().sum();
        let low = (n as i64 * SECOND + sum / 2) / sum;
        Some(Fps { now: now as u32, low: low as u32 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gleichmaessige_bilder() {
        let mut f = Frames::default();
        for i in 0..600 {
            f.push(i * 100_000);
        }
        assert_eq!(f.fps(), Some(Fps { now: 100, low: 100 }));
    }

    #[test]
    fn ruckler_drueckt_nur_den_tiefwert() {
        let mut f = Frames::default();
        let mut t = 0;
        for i in 0..200 {
            t += if i == 50 { 1_000_000 } else { 100_000 };
            f.push(t);
        }
        let v = f.fps().unwrap();
        assert_eq!(v.now, 100);
        assert!(v.low < 20, "{v:?}");
    }

    #[test]
    fn zu_wenig_oder_ungeordnet() {
        let mut f = Frames::default();
        assert_eq!(f.fps(), None);
        f.push(10);
        f.push(5);
        assert_eq!(f.fps(), None);
    }
}
