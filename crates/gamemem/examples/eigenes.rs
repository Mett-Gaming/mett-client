//! Nur lesen: die eigenen Werte und die Lage der Hungerleiste.
use gamemem::{Reader, attributes, image, uitree};
use std::time::Instant;

fn main() {
    let pid = gamemem::find_processes("Minecraft.Windows.exe").expect("Prozesse")[0];
    let r = Reader::open_for_scan(pid).expect("Lesen");
    let (base, bytes) = image(pid).expect("Abbild");
    let t = Instant::now();
    let names = attributes::names(&r, base, &bytes);
    println!("Namen {names:x?} nach {:?}", t.elapsed());
    let t = Instant::now();
    let own = names.and_then(|n| attributes::find(&r, &attributes::blocks(&r, &n)));
    println!("gefunden {} nach {:?}", own.is_some(), t.elapsed());
    println!("Werte {:?}", own.and_then(|o| o.read(&r)));
    let l = uitree::layout(&r, &(base..base + bytes.len() as u64));
    let spot = l.and_then(|l| uitree::spot(&r, &l, "hunger_rend").and_then(|s| s.now(&r, &l)));
    println!("Leiste {spot:?}");
}
