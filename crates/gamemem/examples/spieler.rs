//! Nur lesen: Zahl der Einträge und Gesichter in Minecrafts Spielerliste (keine Namen, keine Bilder).
use gamemem::{Reader, players};
use std::time::Instant;

fn main() {
    let pid = gamemem::find_processes("Minecraft.Windows.exe").expect("Prozesse")[0];
    let r = Reader::open_for_scan(pid).expect("Lesen");
    let t = Instant::now();
    let heads = players::heads(&r);
    println!("Listen {} nach {:?}", heads.len(), t.elapsed());
    let t = Instant::now();
    let p = players::players(&r, &heads);
    let faces = p.as_ref().map(|p| p.iter().filter(|p| p.face.is_some()).count());
    println!("Spieler {:?}, Gesichter {faces:?} nach {:?}", p.map(|p| p.len()), t.elapsed());
}
