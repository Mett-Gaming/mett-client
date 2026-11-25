//! Prüft die Namenssuche am laufenden Spiel. Liest nur, das Abbild bleibt im Arbeitsspeicher.
use compat::{chat_send, image::Image};

fn main() -> windows::core::Result<()> {
    let pids = gamemem::find_processes("Minecraft.Windows.exe")?;
    if pids.is_empty() {
        println!("Minecraft läuft nicht.");
    }
    for pid in pids {
        let (base, bytes) = gamemem::image(pid)?;
        let start = std::time::Instant::now();
        let Some(image) = Image::new(base, &bytes) else {
            println!("Abbild nicht lesbar");
            continue;
        };
        for name in ["ChatScreenController", "ClientInstanceScreenModel", "HudScreenController"] {
            println!("{name}: {} Klassennamen", image.classes(name).len());
        }
        match chat_send::find(&image) {
            Ok(s) => {
                println!("Senden gefunden: Chat {:#x}, Befehl {:x?}, Versatz {:#x}", s.chat, s.command, s.model_offset);
                let t = compat::hud_model::tables(&image);
                println!("HUD-Tabellen: {t:x?} nach {} ms", start.elapsed().as_millis());
                let tables = t.unwrap_or_default().into_iter().map(|t| base + t as u64).collect();
                let range = base..base + bytes.len() as u64;
                let scan = gamemem::Reader::open_for_scan(pid)?;
                println!("Modell: {:x?}", gamemem::model::find(&scan, &tables, s.model_offset, &range));
            }
            Err(e) => println!("Senden aus: {}", e.text()),
        }
        println!("Dauer: {} ms", start.elapsed().as_millis());
    }
    Ok(())
}
