//! Zeigt um Treffer eines Testworts herum, wo Zeichenketten liegen: nur Abstände und Längen, keine fremden Texte.
//! Aufruf: aufbau <Testwort, 6 bis 15 Zeichen>
fn main() -> windows::core::Result<()> {
    let word = std::env::args().nth(1).unwrap_or_default();
    if !(6..=15).contains(&word.len()) {
        println!("Testwort muss 6 bis 15 Zeichen haben.");
        return Ok(());
    }
    for pid in gamemem::find_processes("Minecraft.Windows.exe")? {
        let reader = gamemem::Reader::open_for_scan(pid)?;
        let read = |at: usize, out: &mut [u8]| reader.read(at, out).is_ok();
        for (start, size) in reader.writable_regions() {
            let mut buf = vec![0u8; size];
            if !read(start, &mut buf) {
                continue;
            }
            for (i, w) in buf.windows(word.len()).enumerate() {
                if w == word.as_bytes() {
                    show(start + i, &word, &read);
                }
            }
        }
    }
    Ok(())
}

fn show(at: usize, word: &str, read: &dyn Fn(usize, &mut [u8]) -> bool) {
    let mut head = [0u8; 32];
    if !read(at, &mut head) {
        return;
    }
    let num = |b: &[u8], i: usize| u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
    if num(&head, 16) != word.len() as u64 || num(&head, 24) != 15 {
        println!("{at:#x}: kein eigenständiger Text (Teil eines längeren oder Puffer)");
        return;
    }
    const BACK: usize = 320;
    let mut b = [0u8; 2 * BACK + 32];
    let Some(base) = at.checked_sub(BACK) else { return };
    if !read(base, &mut b) {
        return println!("{at:#x}: eigenständiger Text, Umgebung nicht lesbar");
    }
    print!("{at:#x}:");
    let mut i = 0;
    while i + 32 <= b.len() {
        let (len, cap) = (num(&b, i + 16), num(&b, i + 24));
        let plausible = (cap == 15 || (cap > 15 && cap < 4096 && cap % 16 == 15 && num(&b, i) > 0x10000)) && len <= cap;
        if plausible {
            // Nur verraten, ob das Testwort darin vorkommt.
            let mut text = vec![0u8; len as usize];
            let ok = if cap == 15 {
                text.copy_from_slice(&b[i..i + len as usize]);
                true
            } else {
                read(num(&b, i) as usize, &mut text)
            };
            let has = ok && text.windows(word.len()).any(|w| w == word.as_bytes());
            print!(" [{:+} L{len}{}]", i as i64 - BACK as i64, if has { " TESTWORT" } else { "" });
            i += 32;
        } else {
            i += 8;
        }
    }
    let small = |o: i64| {
        let k = (BACK as i64 + o) as usize;
        u32::from_le_bytes(b[k..k + 4].try_into().unwrap())
    };
    println!("\n    Zahlen davor: -8={} -16={} -24={} -32={}", small(-8), small(-16), small(-24), small(-32));
    // Felder einer Nachricht, falls sie 8 Byte vor dem Text beginnt (nur Zahlen).
    let byte = |o: usize| b[BACK - 8 + o];
    let float = |o: usize| f32::from_le_bytes(b[BACK - 8 + o..BACK - 4 + o].try_into().unwrap());
    if small(-8) <= 9 {
        println!("    Art={} Marken 72:{} 208:{}", small(-8), byte(72), byte(208));
        println!("    Bytes 248..272: {:?}", (248..272).map(byte).collect::<Vec<_>>());
        println!("    Zahlen: 248={} 252={} 256={}", float(248), float(252), float(256));
        // Wie viele Nachbarn sehen im Abstand von 8 Byte-Schritten wie Nachrichten aus?
        for step in (240..=320).step_by(8) {
            let at_next = at - 8 + step;
            let mut n = [0u8; 48];
            if read(at_next, &mut n) && (1..=9).contains(&u32::from_le_bytes(n[0..4].try_into().unwrap())) {
                println!("    Möglicher Nachbar nach {step} Byte: Textlänge {}", num(&n, 24));
            }
        }
    }
}
