//! M0: Lässt sich Bedrock mit normalem Leserecht öffnen? Liest nur die Kennbytes "MZ" des Programmabbilds.
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::*;

fn main() -> windows::core::Result<()> {
    let exe = std::env::args().nth(1).unwrap_or("Minecraft.Windows.exe".into());
    let pids = gamemem::find_processes(&exe)?;
    if pids.is_empty() {
        println!("{exe} läuft nicht.");
    }
    for pid in pids {
        let reader = match gamemem::Reader::open(pid) {
            Ok(r) => r,
            Err(e) => {
                println!("PID {pid}: Öffnen fehlgeschlagen: {e}");
                continue;
            }
        };
        println!("PID {pid}: Öffnen ok");
        unsafe {
            let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid) {
                Ok(s) => s,
                Err(e) => {
                    println!("Modulliste fehlgeschlagen: {e}");
                    continue;
                }
            };
            let mut m = MODULEENTRY32W { dwSize: size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
            let mut ok = Module32FirstW(snap, &mut m).is_ok();
            let mut b = [0; 2];
            if ok {
                let r = reader.read(m.modBaseAddr as usize, &mut b);
                println!("Lesen: {r:?}, Kennung MZ: {}", &b == b"MZ");
            }
            let mut mett = 0;
            while ok {
                let name = String::from_utf16_lossy(&m.szModule);
                let name = name.split('\0').next().unwrap_or_default();
                if name.to_lowercase().contains("mett") {
                    println!("Mett-Modul geladen: {name}");
                    mett += 1;
                }
                ok = Module32NextW(snap, &mut m).is_ok();
            }
            println!("Mett-Module: {mett}");
            let _ = CloseHandle(snap);
        }
    }
    Ok(())
}
