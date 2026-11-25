//! Sucht im laufenden Spiel nach einer selbst geschriebenen Testnachricht. Liest nur, schreibt nichts auf die Platte
//! und gibt nur Treffer mit dem Testwort aus, keine anderen Nachrichten.
//! Aufruf: chatsuche <Testwort, 6 bis 15 Zeichen> [Programm]
use gamemem::chat;

fn main() -> windows::core::Result<()> {
    let mut args = std::env::args().skip(1);
    let word = args.next().unwrap_or_default();
    let exe = args.next().unwrap_or("Minecraft.Windows.exe".into());
    if !(6..=15).contains(&word.len()) {
        println!("Testwort muss 6 bis 15 Zeichen haben.");
        return Ok(());
    }
    let pids = gamemem::find_processes(&exe)?;
    if pids.is_empty() {
        println!("{exe} läuft nicht.");
    }
    for pid in pids {
        let reader = gamemem::Reader::open_for_scan(pid)?;
        let read = |at: usize, out: &mut [u8]| reader.read(at, out).is_ok();
        let (mut bytes, mut seen, mut found) = (0usize, 0, 0);
        for (start, size) in reader.writable_regions() {
            let mut buf = vec![0u8; size];
            if !read(start, &mut buf) {
                continue;
            }
            bytes += size;
            for (i, w) in buf.windows(word.len()).enumerate() {
                if w != word.as_bytes() {
                    continue;
                }
                seen += 1;
                // Kurze Texte stehen direkt in der Nachricht.
                let Some(at) = (start + i).checked_sub(chat::TEXT_AT) else { continue };
                if let Some(m) = chat::parse(at, &read).filter(|m| m.text == word) {
                    found += 1;
                    let next = chat::parse(at + chat::SIZE, &read).is_some();
                    let before = chat::parse(at - chat::SIZE, &read).is_some();
                    println!("Nachricht bei {at:#x}: Art {}, Nachbarn davor={before} danach={next}", m.kind);
                }
            }
        }
        println!("PID {pid}: {} MB gelesen, Testwort {seen}-mal, als Nachricht erkannt {found}-mal", bytes >> 20);
    }
    Ok(())
}
