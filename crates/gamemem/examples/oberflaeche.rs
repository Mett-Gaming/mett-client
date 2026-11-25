//! Nur lesen: leitet den Aufbau der Oberfläche ab und sucht Minecrafts Chat-Anzeige.
use gamemem::{Reader, image, uitree};
use std::time::Instant;

fn main() {
    let pid = gamemem::find_processes("Minecraft.Windows.exe").expect("Prozesse")[0];
    let r = Reader::open_for_scan(pid).expect("Lesen");
    let (base, bytes) = image(pid).expect("Abbild");
    let t = Instant::now();
    let l = uitree::layout(&r, &(base..base + bytes.len() as u64));
    println!("{l:x?} nach {:?}", t.elapsed());
    let t = Instant::now();
    println!("Fadenkreuz {:x?}", l.and_then(|l| uitree::crosshair(&r, &l)));
    println!("{:?} nach {:?}", l.and_then(|l| uitree::panel(&r, &l)), t.elapsed());
}
