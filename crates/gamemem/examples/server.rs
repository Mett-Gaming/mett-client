//! Nur lesen: ist eine Serververbindung erkennbar?
use gamemem::{Reader, server};
use std::time::Instant;

fn main() {
    let pid = gamemem::find_processes("Minecraft.Windows.exe").expect("Prozesse")[0];
    let r = Reader::open_for_scan(pid).expect("Lesen");
    let (base, bytes) = gamemem::image(pid).expect("Abbild");
    let t = Instant::now();
    let image = compat::image::Image::new(base, &bytes).expect("Abbild");
    let tables: Vec<u64> = image.vtables(server::CLASS).into_iter().map(|t| base + t as u64).collect();
    println!("{} Tabellen nach {:?}", tables.len(), t.elapsed());
    let t = Instant::now();
    let objects = server::find(&r, &tables);
    println!("{} Verbindungen nach {:?}", objects.len(), t.elapsed());
    println!("Server {:?}", server::address(&r, &tables, &objects));
}
