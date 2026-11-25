//! Zeigt die Befehlshinweise zu einem Text mit den Daten des laufenden Spiels (nur lesen).
//! Aufruf: hinweise "/enchant @s sh" ["/zweiter Text" ...]
use hud::chat_input::ChatInput;
use hud::hints::Hints;
use sources::{Hub, Wanted};
use std::time::{Duration, Instant};

fn main() {
    let Some(pid) = gamemem::find_processes("Minecraft.Windows.exe").ok().and_then(|p| p.first().copied()) else {
        return println!("Minecraft läuft nicht.");
    };
    let hub = Hub::start();
    hub.want(&Wanted { commands_pid: Some(pid), ..Default::default() });
    let start = Instant::now();
    let commands = loop {
        if let Some(c) = hub.snapshot().commands {
            break c;
        }
        if start.elapsed() > Duration::from_secs(20) {
            return println!("keine Befehlsliste");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    println!(
        "bereit nach {} ms, {} Befehle, andere Namen: {:?}",
        start.elapsed().as_millis(),
        commands.commands.len(),
        commands.aliases
    );
    for text in std::env::args().skip(1) {
        let (mut input, mut h) = (ChatInput::default(), Hints::default());
        input.open(&text);
        let t = Instant::now();
        h.update(&commands, &input);
        let took = t.elapsed();
        let mut out = vec![];
        h.draw(&input, (1920.0, 1080.0), 2.0, &|_| 6.0, &mut out);
        let mut shown: Vec<String> = out
            .iter()
            .filter_map(|d| if let hud::Draw::Game { text, .. } = d { Some(text.clone()) } else { None })
            .collect();
        shown.dedup();
        let colors: Vec<String> = (0..text.chars().count())
            .map(|i| {
                h.color(i).map_or("-".into(), |c| if c == 0xFF5555 { "R".into() } else { format!("{:x}", c >> 20) })
            })
            .collect();
        println!("{text:?} ({took:?}): {shown:?}\n      Farben {}", colors.concat());
    }
}
