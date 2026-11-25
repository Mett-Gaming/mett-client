//! Nur lesen: benannte Zahlen-Einstellungen mit Grenzen. Aufruf: zoomsuche <options.txt> <Namensteil>...
use gamemem::{Reader, options};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let file = options::parse(&std::fs::read_to_string(&a[0]).expect("Datei"));
    let pid = gamemem::find_processes("Minecraft.Windows.exe").expect("Prozesse")[0];
    let r = Reader::open_for_scan(pid).expect("Lesen");
    let range = gamemem::image_range(pid).expect("Abbild");
    let Some(o) = options::find(&r, &range, &file) else { return println!("nicht eindeutig") };
    for (n, f) in options::list(&r, &o) {
        if a[1..].iter().any(|p| n.contains(p.as_str())) {
            println!("{n}: {:?} Datei {:?}", &f[..5], file.get(&n));
        }
    }
}
