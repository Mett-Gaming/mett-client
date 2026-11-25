//! Lässt das Debug-Plugin genau eine Testnachricht senden. Nur über tools/sendetest.ps1 aufrufen (prüft die Welt).
//! Aufruf: sendetest <PID> <Ordner> <Funktion hex> <Modell hex>
use std::path::PathBuf;
use std::time::Duration;
use windows::core::s;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pid: u32 = a[0].parse().expect("PID");
    let dir = PathBuf::from(&a[1]);
    let hex = |t: &str| u64::from_str_radix(t.trim_start_matches("0x"), 16).expect("Adresse");
    let (f, model) = (hex(&a[2]), hex(&a[3]));
    let (core, dll) = (dir.join("mett_core.dll"), dir.join("plugins/debug.dll"));
    match mett_inject::start(pid, &core, &["debug"]) {
        Ok(1) => {}
        other => {
            println!("Nicht geladen: {other:?}");
            let _ = mett_inject::stop(pid, &core);
            return;
        }
    }
    let order: Vec<u8> = [1u64, f, model, u64::MAX].iter().flat_map(|v| v.to_le_bytes()).collect();
    println!("Auftrag: {:?}", mett_inject::write_export(pid, &dll, s!("METT_ORDER"), &order));
    println!("Anstoß: {:?}", mett_inject::call_export(pid, &dll, s!("mett_debug_go")));
    let mut b = [0u8; 32];
    let n = |b: &[u8], i: usize| u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
    for _ in 0..80 {
        std::thread::sleep(Duration::from_millis(100));
        if mett_inject::read_export(pid, &dll, s!("METT_ORDER"), &mut b).is_err() || n(&b, 0) >= 3 {
            break;
        }
    }
    println!("Schritt {} (3 fertig, 4 abgelehnt), Rückgabe {}", n(&b, 0), n(&b, 3));
    let mut d = [0u8; 32];
    let _ = mett_inject::read_export(pid, &dll, s!("METT_DEBUG"), &mut d);
    let word = |i: usize| u32::from_le_bytes(d[i * 4..i * 4 + 4].try_into().unwrap());
    println!("Aufrufe {}, Thread {}, Fenster-Thread {}, Rückrufe {}", word(0), word(1), word(2), word(3));
    println!("Stopp: {:?}", mett_inject::stop(pid, &core));
    println!("Kern noch da: {:?}", mett_inject::find(pid, &core).map(|m| m.is_some()));
}
