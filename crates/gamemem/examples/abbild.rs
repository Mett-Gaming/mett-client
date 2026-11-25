//! Sucht Namen im geladenen Programmabbild. Liest nur, schreibt nichts auf die Platte, gibt nur Fundzahlen und Adressen aus.
//! Aufruf: abbild <Wort> [<Wort> ...]
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::*;

fn main() -> windows::core::Result<()> {
    let words: Vec<String> = std::env::args().skip(1).collect();
    for pid in gamemem::find_processes("Minecraft.Windows.exe")? {
        let reader = gamemem::Reader::open(pid)?;
        let mut m = MODULEENTRY32W { dwSize: size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid)?;
            Module32FirstW(snap, &mut m)?;
            let _ = CloseHandle(snap);
        }
        let base = m.modBaseAddr as usize;
        let num = |at: usize, n: usize| {
            let mut b = [0u8; 8];
            reader.read(base + at, &mut b[..n]).ok().map(|_| u64::from_le_bytes(b) as usize)
        };
        let pe = num(0x3c, 4).unwrap_or(0);
        let count = num(pe + 6, 2).unwrap_or(0);
        let table = pe + 24 + num(pe + 20, 2).unwrap_or(0);
        println!("Abbild {} MB, {count} Abschnitte", m.modBaseSize >> 20);
        let mut wanted: Vec<usize> = vec![];
        let mut hits = vec![vec![]; words.len()];
        for i in 0..count {
            let s = table + i * 40;
            let mut name = [0u8; 8];
            let _ = reader.read(base + s, &mut name);
            let (size, at) = (num(s + 8, 4).unwrap_or(0), num(s + 12, 4).unwrap_or(0));
            let (mut read, mut off) = (0usize, 0usize);
            // Überlappend lesen, damit kein Wort an einer Grenze verloren geht.
            while off < size {
                let mut b = vec![0u8; (1 << 20).min(size - off)];
                if reader.read(base + at + off, &mut b).is_ok() {
                    read += b.len();
                    for (k, w) in words.iter().enumerate() {
                        for (j, x) in b.windows(w.len()).enumerate() {
                            if x == w.as_bytes() {
                                hits[k].push(at + off + j);
                            }
                        }
                    }
                }
                off += (1 << 20) - 64;
            }
            println!(
                "  {:<8} {} MB, gelesen {} MB",
                String::from_utf8_lossy(&name).trim_end_matches('\0'),
                size >> 20,
                read >> 20
            );
        }
        for (w, h) in words.iter().zip(&mut hits) {
            h.sort_unstable();
            h.dedup();
            println!("{w}: {} {:x?}", h.len(), &h[..h.len().min(4)]);
            // Ganzer Programmtext um die ersten Treffer und Stellen im Code, die darauf zeigen (lea).
            for at in h.iter().take(if h.len() <= 40 { 12 } else { 0 }) {
                let mut b = vec![0u8; 600];
                let from = at.saturating_sub(300);
                if reader.read(base + from, &mut b).is_err() {
                    continue;
                }
                let mid = at - from;
                let start = b[..mid].iter().rposition(|c| *c == 0).map_or(0, |i| i + 1);
                let end = b[mid..].iter().position(|c| *c == 0).map_or(b.len(), |i| mid + i);
                println!("   {:x} {}", from + start, String::from_utf8_lossy(&b[start..end]));
                wanted.push(from + start);
            }
        }
        wanted.sort_unstable();
        wanted.dedup();
        // .text ist der erste Abschnitt.
        let (size, at) = (num(table + 8, 4).unwrap_or(0), num(table + 12, 4).unwrap_or(0));
        let mut off = 0usize;
        while off < size {
            let mut b = vec![0u8; (1 << 20).min(size - off)];
            if reader.read(base + at + off, &mut b).is_ok() {
                for j in 0..b.len().saturating_sub(7) {
                    // lea r64, [rip+rel32]
                    if (b[j] == 0x48 || b[j] == 0x4c) && b[j + 1] == 0x8d && b[j + 2] & 0xc7 == 5 {
                        let rel = i32::from_le_bytes(b[j + 3..j + 7].try_into().unwrap()) as i64;
                        let target = (at + off + j + 7) as i64 + rel;
                        if wanted.binary_search(&(target as usize)).is_ok() {
                            println!("Verweis aus Code {:x} auf {target:x}", at + off + j);
                        }
                    }
                }
            }
            off += (1 << 20) - 8;
        }
    }
    Ok(())
}
