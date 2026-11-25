//! Nur lesen: eigenes Inventar, Rüstung und Effekte.
use gamemem::{Reader, inventory};
use std::time::Instant;

fn main() {
    let pid = gamemem::find_processes("Minecraft.Windows.exe").expect("Prozesse")[0];
    let r = Reader::open_for_scan(pid).expect("Lesen");
    let range = gamemem::image_range(pid).expect("Abbild");
    let t = Instant::now();
    let Some(k) = inventory::kinds(&r, &range) else { return println!("Arten nicht erkannt, {:?}", t.elapsed()) };
    println!("Arten nach {:?}", t.elapsed());
    let t = Instant::now();
    let p = inventory::find(&r, &k);
    println!("Plätze nach {:?}", t.elapsed());
    println!("{:?}", inventory::Watch::default().read(&r, &k, &p));
    let t = Instant::now();
    let e = inventory::effects(&r);
    println!("Effektlisten {e:x?} nach {:?}: {:?}", t.elapsed(), inventory::own_effects(&r, &e));
}
