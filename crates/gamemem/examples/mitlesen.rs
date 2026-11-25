//! Liest eine gefundene Nachrichtenliste mit. Nur Texte, die mit METT beginnen, werden ausgegeben, sonst nur die Anzahl.
//! Aufruf: mitlesen <Adresse der Liste, hexadezimal> <Sekunden>
use gamemem::chat;
use std::time::{Duration, Instant};

fn main() -> windows::core::Result<()> {
    let mut args = std::env::args().skip(1);
    let hex = args.next().unwrap_or_default();
    let list = usize::from_str_radix(hex.trim_start_matches("0x"), 16).expect("Adresse");
    let seconds: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(60);
    let Some(pid) = gamemem::find_processes("Minecraft.Windows.exe")?.first().copied() else { return Ok(()) };
    let reader = gamemem::Reader::open(pid)?;
    let read = |at: usize, out: &mut [u8]| reader.read(at, out).is_ok();
    let (start, mut last) = (Instant::now(), None);
    while start.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_millis(50));
        let mut b = [0u8; 24];
        if !read(list, &mut b) {
            println!("Liste nicht mehr lesbar");
            break;
        }
        let n = |i: usize| usize::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
        let (from, to) = (n(0), n(1));
        let Some(size) = to.checked_sub(from).filter(|s| s % chat::SIZE == 0 && *s <= chat::SIZE * 4096) else {
            continue;
        };
        let count = size / chat::SIZE;
        if last == Some((from, count)) {
            continue;
        }
        let good = (0..count).filter(|k| chat::parse(from + k * chat::SIZE, &read).is_some()).count();
        let newest = count.checked_sub(1).and_then(|k| chat::parse(from + k * chat::SIZE, &read));
        let text = newest.filter(|m| m.text.starts_with("METT")).map(|m| format!("Art {}: {}", m.kind, m.text));
        let ms = start.elapsed().as_millis();
        println!("{ms} ms: {count} Nachrichten, gültig {good}, neueste: {}", text.unwrap_or("(nicht gezeigt)".into()));
        last = Some((from, count));
    }
    Ok(())
}
