//! Nur Entwicklung: liest die Zoom-Werte im HUD-Plugin mit. Aufruf: zoomwerte <PID> <hud.dll> <Sekunden>
use std::time::{Duration, Instant};
use windows::core::s;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (pid, dll) = (a[0].parse().expect("PID"), std::path::PathBuf::from(&a[1]));
    let (start, mut last) = (Instant::now(), [0u8; 48]);
    while start.elapsed() < Duration::from_secs(a[2].parse().expect("Sekunden")) {
        let mut b = [0u8; 48];
        if mett_inject::read_export(pid, &dll, s!("METT_ZOOM"), &mut b).is_err() {
            return println!("nicht lesbar");
        }
        // Lebenszeichen zählt nicht als Änderung.
        let v = |i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
        let mut same = b;
        same[28..32].fill(0);
        let f = f32::from_bits(v(3));
        if same[..12] != last[..12] || same[24..28] != last[24..28] {
            let t = start.elapsed().as_millis();
            println!(
                "{t} ms: gehalten {} Stufen {} Faktor {f:.3} Grenzen {}..{} zoomt {}",
                v(0),
                v(2) as i32,
                v(4) as i32,
                v(5) as i32,
                v(6)
            );
        }
        last = same;
        std::thread::sleep(Duration::from_millis(30));
    }
}
