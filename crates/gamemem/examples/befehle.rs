//! Liest die Befehlsliste des laufenden Spiels (nur lesen) und gibt eine Zusammenfassung aus. Aufruf: befehle [Befehl]
use gamemem::{Reader, commands};

fn main() -> windows::core::Result<()> {
    let name = std::env::args().nth(1);
    for pid in gamemem::find_processes("Minecraft.Windows.exe")? {
        let r = Reader::open_for_scan(pid)?;
        let start = std::time::Instant::now();
        let place = commands::find(&r);
        println!("Stelle: {place:x?} nach {} ms", start.elapsed().as_millis());
        let Some(p) = place else { continue };
        let start = std::time::Instant::now();
        let Some(c) = commands::load(&r, &p) else {
            println!("nicht lesbar");
            continue;
        };
        let values: usize = c.lists.iter().map(|l| l.values.len()).sum();
        println!(
            "{} Befehle, {} Listen, {values} Werte, gelesen in {} ms",
            c.commands.len(),
            c.lists.len(),
            start.elapsed().as_millis()
        );
        if let Some(n) = &name {
            println!("{:#?}", c.commands.get(n));
        }
    }
    Ok(())
}
