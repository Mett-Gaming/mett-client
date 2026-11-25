//! Findet Nachrichtenlisten ohne Testwort. Gibt nur Lage und Anzahl aus, keine Texte.
use gamemem::chat;

fn main() -> windows::core::Result<()> {
    for pid in gamemem::find_processes("Minecraft.Windows.exe")? {
        let start = std::time::Instant::now();
        let reader = gamemem::Reader::open_for_scan(pid)?;
        let read = |at: usize, out: &mut [u8]| reader.read(at, out).is_ok();
        for (at, size) in reader.writable_regions() {
            let mut buf = vec![0u8; size];
            if read(at, &mut buf) {
                for list in chat::lists_in(at, &buf, &read) {
                    println!("Liste bei {list:#x}: {} Nachrichten", chat::messages(list, &read).map_or(0, |m| m.len()));
                }
            }
        }
        println!("Dauer: {} ms", start.elapsed().as_millis());
    }
    Ok(())
}
