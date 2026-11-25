//! Eingaben für den Zoom: meldet dem Zoom-Plugin Tasten und Mausrad und bremst die Maus, wie es das vorgibt. Den Verlauf
//! rechnet das Zoom-Plugin; ohne dessen Lebenszeichen ist hier alles wie sonst.
use hudlink::control;
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use windows::Win32::System::SystemInformation::GetTickCount64;

/// Vom Zoom-Plugin gelesen und beschrieben, siehe die Namen darunter.
#[unsafe(no_mangle)]
pub static METT_ZOOM: [AtomicU32; 12] = [const { AtomicU32::new(0) }; 12];

const HELD: usize = 0;
const PRESSES: usize = 1;
const STEPS: usize = 2;
const FACTOR: usize = 3;
const LEAST: usize = 4;
const MOST: usize = 5;
const ZOOMING: usize = 6;
const BEAT: usize = 7;
const SECOND: usize = 8;
const HIDE: usize = 9;
const CINEMATIC: usize = 10;
const TIMEOUT_MS: u32 = 500;

fn get(i: usize) -> u32 {
    METT_ZOOM[i].load(SeqCst)
}

/// Lebt das Zoom-Plugin?
fn alive() -> bool {
    let now = unsafe { GetTickCount64() } as u32;
    get(BEAT) != 0 && now.wrapping_sub(get(BEAT)) < TIMEOUT_MS
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Zoom,
    In,
    Out,
    Second,
}

/// Welche Zoom-Taste ist das, wenn sie gerade uns gehört?
pub fn key(vk: u8, control: u32, keys: [u8; 4]) -> Option<Key> {
    if vk == 0 || control & control::ZOOM == 0 || !alive() {
        return None;
    }
    let scroll = control & control::ZOOM_KEYS != 0;
    match vk {
        v if v == keys[0] => Some(Key::Zoom),
        v if scroll && v == keys[1] => Some(Key::In),
        v if scroll && v == keys[2] => Some(Key::Out),
        v if control & control::ZOOM_SECOND != 0 && v == keys[3] => Some(Key::Second),
        _ => None,
    }
}

pub fn press(k: Key) {
    match k {
        Key::Zoom => {
            METT_ZOOM[HELD].store(1, SeqCst);
            METT_ZOOM[PRESSES].fetch_add(1, SeqCst);
        }
        Key::In => step(1),
        Key::Out => step(-1),
        Key::Second => {
            METT_ZOOM[SECOND].fetch_add(1, SeqCst);
        }
    }
}

pub fn release() {
    METT_ZOOM[HELD].store(0, SeqCst);
}

pub fn zooming() -> bool {
    alive() && get(ZOOMING) != 0
}

/// Stufen weiter; die Grenzen gibt das Zoom-Plugin vor.
pub fn step(by: i32) {
    let (lo, hi) = (get(LEAST) as i32, get(MOST) as i32);
    if lo <= hi {
        let _ = METT_ZOOM[STEPS].try_update(SeqCst, SeqCst, |s| Some((s as i32 + by).clamp(lo, hi) as u32));
    }
}

/// Faktor für die Mausbewegung, Film-Kamera in Prozent, und ob das Mausrad dem Zoom gehört.
pub fn mouse(control: u32) -> (f64, f64, bool) {
    if !alive() || control & control::ZOOM == 0 {
        return (1.0, 0.0, false);
    }
    let f = f32::from_bits(get(FACTOR)) as f64;
    let f = if f.is_finite() && f > 0.0 && f <= 1.0 { f } else { 1.0 };
    (f, get(CINEMATIC).min(200) as f64, zooming() && control & control::ZOOM_WHEEL != 0)
}

/// Zweit-Zoom blendet unsere Anzeige aus.
pub fn hides() -> bool {
    alive() && get(HIDE) != 0
}

/// Nach dem Entladen des Plugins darf nichts stehen bleiben.
pub fn clear() {
    METT_ZOOM.iter().for_each(|v| v.store(0, SeqCst));
}

#[cfg(test)]
pub fn pretend(least: i32, most: i32, factor: f32, zooming: bool) {
    METT_ZOOM[LEAST].store(least as u32, SeqCst);
    METT_ZOOM[MOST].store(most as u32, SeqCst);
    METT_ZOOM[FACTOR].store(factor.to_bits(), SeqCst);
    METT_ZOOM[ZOOMING].store(zooming as u32, SeqCst);
    METT_ZOOM[BEAT].store((unsafe { GetTickCount64() } as u32).max(1), SeqCst);
}
