//! Prüft Loader, Kern und Plugins an einem eigenen Testprozess (ping.exe), nie an Minecraft.
//! Aufruf: loadertest <Ordner mit mett_core.dll und plugins\>
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("Ordner"));
    let core = dir.join("mett_core.dll");
    for refuse in [false, true] {
        let mut cmd = Command::new("ping.exe");
        cmd.args(["-n", "30", "127.0.0.1"]).stdout(Stdio::null());
        if refuse {
            cmd.env("METT_PROBE_REFUSE", "1");
        }
        let mut child = cmd.spawn().expect("Testprozess");
        let pid = child.id();
        std::thread::sleep(Duration::from_millis(500));
        println!("== Testprozess {pid}, Plugin lehnt ab: {refuse}");
        let names = ["kaputt", "fremd", "fehlt", "../probe", "probe"];
        println!("Start: {:?}", mett_inject::start(pid, &core, &names));
        match mett_inject::status(pid) {
            Ok(list) => list.iter().for_each(|(n, s)| println!("  {n}: {}", mett_plugin::state::text(*s))),
            Err(e) => println!("  Status: {e}"),
        }
        println!("Zweiter Start: {:?}", mett_inject::start(pid, &core, &names));
        println!("Stopp: {:?}", mett_inject::stop(pid, &core));
        println!("Kern noch da: {:?}", mett_inject::find(pid, &core).map(|m| m.is_some()));
        println!("Plugin noch da: {:?}", mett_inject::find(pid, &dir.join("plugins/probe.dll")).map(|m| m.is_some()));
        println!("Status danach: {}", mett_inject::status(pid).is_err());
        println!("Testprozess lebt: {}", child.try_wait().is_ok_and(|s| s.is_none()));
        let _ = child.kill();
        let _ = child.wait();
    }
}
