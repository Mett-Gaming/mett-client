//! Findet die Liste, in der eine Testnachricht liegt, und zeigt deren Lage. Liest nur; fremde Texte werden nicht ausgegeben.
//! Aufruf: liste <Testwort, 6 bis 15 Zeichen>
use gamemem::chat;

fn main() -> windows::core::Result<()> {
    let word = std::env::args().nth(1).unwrap_or_default();
    if !(6..=15).contains(&word.len()) {
        println!("Testwort muss 6 bis 15 Zeichen haben.");
        return Ok(());
    }
    for pid in gamemem::find_processes("Minecraft.Windows.exe")? {
        let reader = gamemem::Reader::open_for_scan(pid)?;
        let read = |at: usize, out: &mut [u8]| reader.read(at, out).is_ok();
        let regions = reader.writable_regions();
        let mut hits = vec![];
        each(&regions, &read, |start, buf| {
            for (i, w) in buf.windows(word.len()).enumerate() {
                if w == word.as_bytes() && start + i >= chat::TEXT_AT {
                    let at = start + i - chat::TEXT_AT;
                    if chat::parse(at, &read).is_some_and(|m| m.text == word) {
                        hits.push(at);
                    }
                }
            }
        });
        println!("Nachrichten mit Testwort: {}", hits.len());
        // Eine Liste besteht aus Anfang, Ende und Kapazitätsende; der Treffer muss darin auf einem Platz liegen.
        each(&regions, &read, |start, buf| {
            for (i, w) in buf.chunks(8).enumerate() {
                let n = |k: usize| {
                    buf.get((i + k) * 8..(i + k) * 8 + 8).map(|b| usize::from_le_bytes(b.try_into().unwrap()))
                };
                let (Some(a), Some(b), Some(c)) = (n(0), n(1), n(2)) else { continue };
                if w.len() < 8 || a == 0 || b <= a || c < b || c - a > chat::SIZE * 4096 {
                    continue;
                }
                for hit in hits.iter().filter(|h| (a..b).contains(*h) && (**h - a) % chat::SIZE == 0) {
                    let whole = (b - a).is_multiple_of(chat::SIZE);
                    let count = (b - a) / chat::SIZE;
                    let good = (0..count).filter(|k| chat::parse(a + k * chat::SIZE, &read).is_some()).count();
                    println!(
                        "Liste bei {:#x}: {count} Nachrichten, davon gültig {good}, Platz {}, Größe passt={whole}",
                        start + i * 8,
                        (hit - a) / chat::SIZE
                    );
                }
            }
        });
    }
    Ok(())
}

fn each(regions: &[(usize, usize)], read: &dyn Fn(usize, &mut [u8]) -> bool, mut f: impl FnMut(usize, &[u8])) {
    for &(start, size) in regions {
        let mut buf = vec![0u8; size];
        if read(start, &mut buf) {
            f(start, &buf);
        }
    }
}
