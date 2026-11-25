//! Lädt das HUD-Plugin für ein paar Sekunden ins Spiel und entfernt es wieder. Nur im Hauptmenü oder in „Mett-Debug“.
//! Aufruf: hudtest <PID> <Ordner> <Sekunden>
use std::path::PathBuf;
use std::time::Duration;
use windows::core::s;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pid: u32 = a[0].parse().expect("PID");
    let dir = PathBuf::from(&a[1]);
    let seconds: u32 = a[2].parse().expect("Sekunden");
    let (core, dll) = (dir.join("mett_core.dll"), dir.join("plugins/hud.dll"));
    println!("Start: {:?}", mett_inject::start(pid, &core, &["hud"]));
    for (n, s) in mett_inject::status(pid).unwrap_or_default() {
        println!("  {n}: {}", mett_plugin::state::text(s));
    }
    for i in 0..seconds {
        std::thread::sleep(Duration::from_secs(1));
        let mut b = [0u8; 16];
        if mett_inject::read_export(pid, &dll, s!("METT_HUD"), &mut b).is_ok() {
            let v = |k: usize| u32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap());
            println!("{i} s: {} Bilder, {} Fehler, letzter {:#x}, Format {}", v(0), v(1), v(2), v(3));
        }
    }
    let mut m = [0u8; 12];
    let _ = mett_inject::read_export(pid, &dll, s!("METT_MOUSE"), &mut m);
    let v = |k: usize| u32::from_le_bytes(m[k * 4..k * 4 + 4].try_into().unwrap());
    println!("Maus: eingehängt {} (2 = ganz), Aufbau {} (1 = passt), gesperrte Abfragen {}", v(0), v(1), v(2));
    println!("Stopp: {:?}", mett_inject::stop(pid, &core));
    for d in [&core, &dll] {
        println!("{} noch da: {:?}", d.display(), mett_inject::find(pid, d).map(|m| m.is_some()));
    }
}
