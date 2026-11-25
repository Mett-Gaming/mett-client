//! Genau ein Ladeversuch mit Fehlerauskunft. Aufruf: diagnose <PID> <Pfad zur DLL>
fn main() {
    let mut args = std::env::args().skip(1);
    let pid: u32 = args.next().and_then(|a| a.parse().ok()).expect("PID");
    let dll = std::path::PathBuf::from(args.next().expect("DLL"));
    match mett_inject::diagnose(pid, &dll) {
        Ok(d) => {
            println!("sichtbar={:?} geladen={}", d.attributes, d.loaded);
            d.errors.iter().for_each(|e| println!("  {e}: {}", std::io::Error::from_raw_os_error(*e as i32)));
        }
        Err(e) => println!("Versuch nicht möglich: {e}"),
    }
}
