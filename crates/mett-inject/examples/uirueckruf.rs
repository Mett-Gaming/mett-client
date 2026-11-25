//! Versuch, nur in „Mett-Debug“ und ohne laufenden Client: zählt Rückrufe an der Chat-Anzeige und stellt alles zurück.
//! Aufruf: uirueckruf <PID> <Ordner> <Sekunden>
use gamemem::{Reader, uitree};
use std::path::PathBuf;
use std::time::Duration;
use windows::core::s;

fn debug_world_open() -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    let files = |d: PathBuf| std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path());
    let Ok(app) = std::env::var("APPDATA") else { return false };
    files(PathBuf::from(app).join("Minecraft Bedrock").join("Users"))
        .flat_map(|u| files(u.join("games/com.mojang/minecraftWorlds/MettDebug1/db")))
        .filter(|f| f.extension().is_some_and(|e| e == "log"))
        .any(|f| {
            let open = std::fs::OpenOptions::new().read(true).share_mode(0).open(f);
            open.is_err_and(|e| e.raw_os_error() == Some(32))
        })
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pid: u32 = a[0].parse().expect("PID");
    let dir = PathBuf::from(&a[1]);
    let seconds: u64 = a[2].parse().expect("Sekunden");
    let r = Reader::open_for_scan(pid).expect("Lesen");
    let range = gamemem::image_range(pid).expect("Abbild");
    let Some(l) = uitree::layout(&r, &range) else { return println!("Aufbau nicht eindeutig, nichts geladen") };
    let Some(p) = uitree::panel(&r, &l) else { return println!("Anzeige nicht gefunden, nichts geladen") };
    let word = |at: u64| {
        let mut b = [0u8; 8];
        r.read(at as usize, &mut b).ok().map(|_| u64::from_le_bytes(b)).unwrap_or(0)
    };
    let (begin, end) = (word(p.at + l.children), word(p.at + l.children + 8));
    if end.wrapping_sub(begin) != 16 {
        return println!("Liste nicht eindeutig, nichts geladen");
    }
    let list = word(begin + l.half);
    if !debug_world_open() {
        return println!("„Mett-Debug“ ist nicht offen, nichts geladen");
    }
    let (core, dll) = (dir.join("mett_core.dll"), dir.join("plugins").join("uiprobe.dll"));
    if !matches!(mett_inject::find(pid, &core), Ok(None)) {
        return println!("Der Kern läuft schon (Client an?), nichts geladen");
    }
    println!("Laden: {:?}", mett_inject::start(pid, &core, &["uiprobe"]));
    let order = format!("{:x} {:x} {:x}", p.at, list, l.table);
    println!("Auftrag: {:?}", mett_inject::call_export_text(pid, &dll, s!("mett_uiprobe_order"), &order));
    std::thread::sleep(Duration::from_secs(seconds));
    // Zähler: 8 × 16, ersetzt 8, Hauptthread, zurück, benutzt, Rest; danach Ereignisse zu 40 Byte.
    let mut b = vec![0u8; 8 * 16 * 4 + 8 * 4 + 16 + 512 * 40];
    println!("Auslesen: {:?}", mett_inject::read_export(pid, &dll, s!("METT_UIPROBE"), &mut b));
    let stop = mett_inject::stop(pid, &core);
    let int = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let float = |o: usize| f32::from_bits(int(o));
    for part in 0..8 {
        let counts: Vec<u32> = (0..16).map(|s| int((part * 16 + s) * 4)).collect();
        println!("Baustein {part}: ersetzt {:016b}, Aufrufe {counts:?}", int(512 + part * 4));
    }
    println!("Hauptthread {}, Ereignisse {}", int(544), int(552));
    let mut first = 0i64;
    for i in 0..(int(552) as usize).min(512) {
        let o = 560 + i * 40;
        let time = i64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        first = if i == 0 { time } else { first };
        println!(
            "{:9.2} ms  Baustein {} Platz {} Thread {} Merker {:x} Höhe {:.1} Lage {:.1} Liste {:.1} fertig {:.1}",
            (time - first) as f64 / 10_000.0,
            int(o + 8),
            int(o + 12),
            int(o + 16),
            int(o + 20),
            float(o + 24),
            float(o + 28),
            float(o + 32),
            float(o + 36)
        );
    }
    let mut now = [0u8; 4];
    let _ = mett_inject::read_export(pid, &dll, s!("METT_UIPROBE"), &mut now);
    let mut all = vec![0u8; 552];
    let _ = mett_inject::read_export(pid, &dll, s!("METT_UIPROBE"), &mut all);
    println!("Stopp: {stop:?}, zurückgestellt {}", u32::from_le_bytes(all[548..552].try_into().unwrap()));
}
