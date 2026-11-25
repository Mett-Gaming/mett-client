//! Ende-zu-Ende ohne Minecraft: lädt die echten DLLs in das Ersatz-Spiel, zeichnet, tippt, verliert den Client, räumt auf.
//! Aufruf: durchlauf <Ordner mit mett_core.dll und plugins\>
use hud::{Draw, Rect, wire};
use hudlink::{Client, Event, TIMEOUT_MS, control};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

fn check(what: &str, ok: bool) {
    println!("{} {what}", if ok { "ok  " } else { "FEHL" });
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("Ordner"));
    let core = dir.join("mett_core.dll");
    let game = std::env::current_exe().unwrap().with_file_name("ersatzspiel.exe");
    let mut child = Command::new(game).arg("25").stdout(Stdio::piped()).spawn().expect("Ersatz-Spiel");
    let pid = child.id();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    let window = HWND(line.trim().strip_prefix("bereit ").unwrap().parse::<isize>().unwrap() as *mut _);
    std::thread::sleep(Duration::from_millis(500));

    check("Laden", mett_inject::start(pid, &core, &["hud"]) == Ok(1));
    check("Zweites Plugin nachladen", mett_inject::start(pid, &core, &["chat"]) == Ok(2));
    check("Nochmal laden verdoppelt nichts", mett_inject::start(pid, &core, &["hud", "chat"]) == Ok(2));
    let names: Vec<_> = mett_inject::status(pid).unwrap_or_default();
    check("Stand beider Plugins", names.len() == 2 && names.iter().all(|n| n.1 == mett_plugin::state::LOADED));

    let c = Client::open(pid).expect("Verbindung");
    let rect = Rect { x: 10.0, y: 10.0, w: 100.0, h: 40.0 };
    let scene = wire::encode(&[Draw::Box { rect, radius: 4.0, color: [1.0, 0.0, 0.0, 1.0] }]);
    c.set_keys(&[84], &[191]);
    let beat = |bits: u32, ms: u64| {
        for _ in 0..ms / 50 {
            c.beat(bits);
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    c.send(&scene);
    check(
        "Symboltafel angenommen",
        (0..Client::SHEETS).all(|i| c.send_sheet(i, 0xE0 + i as u8, 256, &vec![200; 256 * 256 * 4])),
    );
    beat(0, 1000);
    let (frames, failures) = c.counts();
    check(&format!("Zeichnet: {frames} Bilder, {failures} Fehler"), frames > 20 && failures == 0);
    check("Bildgröße", c.screen() == Some((400, 300)));

    let post = |m: u32, w: usize| unsafe {
        let _ = PostMessageW(Some(window), m, WPARAM(w), LPARAM(0));
    };
    c.events();
    c.beat(control::MENU);
    post(WM_KEYDOWN, 0x41);
    post(WM_LBUTTONDOWN, 0);
    beat(control::MENU, 300);
    let e = c.events();
    check(
        "Menü bekommt Tasten und Klicks",
        e.iter().any(|e| matches!(e, Event::Key { vk: 0x41, .. }))
            && e.contains(&Event::Button { which: 0, down: true }),
    );

    // Client verschwindet: Tasten gehen wieder ans Spiel.
    std::thread::sleep(Duration::from_millis(TIMEOUT_MS + 200));
    post(WM_KEYDOWN, 0x42);
    std::thread::sleep(Duration::from_millis(200));
    check("Ohne Client meldet das Plugin nichts", !c.events().iter().any(|e| matches!(e, Event::Key { .. })));

    // Neuer Client übernimmt ein noch geladenes Plugin.
    drop(c);
    let c = Client::open(pid).expect("neue Verbindung");
    c.beat(0);
    check("Neuer Client verbindet sich", c.connected());
    check("Entladen", mett_inject::stop(pid, &core).is_ok());
    check("Verbindung ist danach zu", !c.connected());
    for n in ["hud", "chat"] {
        let dll = dir.join("plugins").join(format!("{n}.dll"));
        check(&format!("{n} ist aus dem Prozess"), matches!(mett_inject::find(pid, &dll), Ok(None)));
    }
    check("Kern ist aus dem Prozess", matches!(mett_inject::find(pid, &core), Ok(None)));
    drop(c);
    check("Ersatz-Spiel lebt", child.try_wait().is_ok_and(|s| s.is_none()));
    let status = child.wait().unwrap();
    line.clear();
    let _ = out.read_line(&mut line);
    println!("{}", line.trim());
    check("Ersatz-Spiel endet normal", status.success());
    // Es selbst sah nur die Taste ohne Client.
    check("Nur eine Taste erreichte das Spiel", line.contains(", 1 Tasten"));
}
