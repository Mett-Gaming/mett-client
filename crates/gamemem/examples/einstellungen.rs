//! Nur lesen: findet Zeilenabstand und Hintergrund-Deckkraft des Chats. Aufruf: einstellungen <options.txt>
use gamemem::{Reader, options};
use std::time::Instant;

fn main() {
    let file = std::fs::read_to_string(std::env::args().nth(1).expect("options.txt")).expect("Datei");
    let pid = gamemem::find_processes("Minecraft.Windows.exe").expect("Prozesse")[0];
    let r = Reader::open_for_scan(pid).expect("Lesen");
    let range = gamemem::image_range(pid).expect("Abbild");
    let t = Instant::now();
    let Some(o) = options::find(&r, &range, &options::parse(&file)) else {
        return println!("nicht eindeutig, {:?}", t.elapsed());
    };
    println!("{o:x?} nach {:?}", t.elapsed());
    println!("Abstand {:?}", options::get(&r, &o, o.spacing, options::SPACING));
    println!("Deckkraft {:?}", options::get(&r, &o, o.opacity, options::OPACITY));
}
