//! Zählt GameInput-Aufrufe des Spiels (nur Entwicklung, nur in „Mett-Debug“). Aufruf: inputtest <PID> <Ordner> <Sekunden>
use std::path::PathBuf;
use std::time::Duration;
use windows::core::s;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pid: u32 = a[0].parse().expect("PID");
    let dir = PathBuf::from(&a[1]);
    let seconds: usize = a[2].parse().expect("Sekunden");
    let (core, dll) = (dir.join("mett_core.dll"), dir.join("plugins/inputdiag.dll"));
    println!("Start: {:?}", mett_inject::start(pid, &core, &["inputdiag"]));
    for (n, s) in mett_inject::status(pid).unwrap_or_default() {
        println!("  {n}: {}", mett_plugin::state::text(s));
    }
    let mut last = [0u32; 42];
    for i in 0..seconds {
        std::thread::sleep(Duration::from_secs(1));
        let mut b = [0u8; 168];
        if mett_inject::read_export(pid, &dll, s!("METT_COUNT"), &mut b).is_err() {
            continue;
        }
        let now: [u32; 42] = std::array::from_fn(|k| u32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap()));
        if i == 0 {
            println!("Stand {}, Messwert der Maus {:#x}", now[40], now[41]);
        }
        let diff = |from: usize| -> Vec<String> {
            let changed = (from..from + 20).filter(|k| now[*k] != last[*k]);
            changed.map(|k| format!("{}:{}", k - from, now[k] - last[k])).collect()
        };
        println!("{i} s: Eingabe {:?} Messwert {:?}", diff(0), diff(20));
        last = now;
    }
    println!("Stopp: {:?}", mett_inject::stop(pid, &core));
}
