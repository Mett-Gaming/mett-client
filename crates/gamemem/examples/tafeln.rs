//! Welche Symboltafeln nennt das laufende Spiel? Nur lesen; gibt Nummer, Kante und Zahl der Symbole aus, kein Bild.
use gamemem::{Reader, sheets};
use std::time::Instant;

fn main() -> windows::core::Result<()> {
    for pid in gamemem::find_processes("Minecraft.Windows.exe")? {
        let r = Reader::open_for_scan(pid)?;
        let t = Instant::now();
        let found = sheets::find(&r);
        println!("{} Tafeln nach {} ms", found.len(), t.elapsed().as_millis());
        for s in found {
            let (side, cell) = (s.side as usize, s.side as usize / 16);
            let solid = |i: usize| {
                let (col, row) = (i % 16, i / 16);
                (0..cell * cell).any(|k| {
                    let (x, y) = (col * cell + k % cell, row * cell + k / cell);
                    s.pixels[(y * side + x) * 4 + 3] > 8
                })
            };
            let used = (0..256).filter(|i| solid(*i)).count();
            println!("Tafel {:X}: Kante {side}, {used} Symbole, DF belegt: {}", s.id, solid(0xDF));
        }
    }
    Ok(())
}
