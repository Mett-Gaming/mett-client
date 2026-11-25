//! Nur Entwicklung: Zeichenzeit des HUDs je Bild. Aufruf: zeichenzeit <PID> <hud.dll> <Sekunden>
use std::time::Duration;
use windows::core::s;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (pid, dll) = (a[0].parse().expect("PID"), std::path::PathBuf::from(&a[1]));
    let mut last = None;
    for _ in 0..a[2].parse::<u32>().expect("Sekunden") {
        let mut b = [0u8; 24];
        if mett_inject::read_export(pid, &dll, s!("METT_HUD"), &mut b).is_err() {
            return println!("nicht lesbar");
        }
        let v = |i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
        let frames = last.map_or(0, |l| v(0).wrapping_sub(l));
        last = Some(v(0));
        println!("{frames} Bilder/s, {:.1} µs je Bild, längstes {:.1} µs", v(4) as f32 / 1e3, v(5) as f32 / 1e3);
        std::thread::sleep(Duration::from_secs(1));
    }
}
