//! Lädt Kern und leeres Prüf-Plugin fünf Sekunden ins Spiel und entfernt sie wieder. Nur in einer Offline-Welt benutzen.
//! Aufruf: spieltest <PID> <Ordner mit mett_core.dll und plugins> [Plugin]
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let pid: u32 = args.next().and_then(|a| a.parse().ok()).expect("PID");
    let dir = PathBuf::from(args.next().expect("Ordner"));
    let core = dir.join("mett_core.dll");
    let plugin = args.next().unwrap_or("probe".into());
    match mett_inject::start(pid, &core, &[plugin.as_str()]) {
        Ok(n) => println!("Geladen: {n} Plugin(s)"),
        // Kein zweiter Versuch und kein Umweg.
        Err(e) => return println!("Laden nicht möglich, Abbruch: {e}; Kern da: {:?}", mett_inject::find(pid, &core)),
    }
    match mett_inject::status(pid) {
        Ok(list) => list.iter().for_each(|(n, s)| println!("  {n}: {}", mett_plugin::state::text(*s))),
        Err(e) => println!("  Status nicht lesbar: {e}"),
    }
    std::thread::sleep(Duration::from_secs(5));
    let mut b = [0u8; 24];
    let dll = dir.join(format!("plugins/{plugin}.dll"));
    if mett_inject::read_export(pid, &dll, windows::core::s!("METT_DEBUG"), &mut b).is_ok() {
        let n = |i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
        println!(
            "Aufrufe {}, im Thread {}, Fenster-Thread {}, Rückrufe {}, Art {}, angenommen {}",
            n(0),
            n(1),
            n(2),
            n(3),
            n(4),
            n(5)
        );
    }
    println!("Stopp: {:?}", mett_inject::stop(pid, &core));
    println!("Kern noch da: {:?}", mett_inject::find(pid, &core).map(|m| m.is_some()));
    println!(
        "Plugin noch da: {:?}",
        mett_inject::find(pid, &dir.join(format!("plugins/{plugin}.dll"))).map(|m| m.is_some())
    );
}
