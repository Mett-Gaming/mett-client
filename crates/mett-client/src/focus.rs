//! Einstellungen von Minecraft, die nur für die Dauer einer Launcher-Sitzung feststehen: kein Pausenmenü bei
//! Fokusverlust, und im Chat Minecrafts Pixelschrift (Wunsch des Nutzers: kein Noto Sans).
//!
//! Je Einstellung hält ein Merkzettel neben der Datei fest, dass wir sie geändert haben. So wird auch nach einem Absturz
//! zurückgestellt, und eine Wahl des Nutzers bleibt unangetastet.
use crate::zoom_link::FOV;
use mett_plugin::style::JAVA;
use mett_plugin::values::ALLOWED;
use std::path::{Path, PathBuf};
use std::{fs, io};

struct Forced {
    name: &'static str,
    note: &'static str,
}

/// Beide werden von 1 auf 0 gestellt.
const PAUSE: Forced = Forced { name: "pause_menu_on_focus_lost:", note: "mett_pause_menu_restore" };
const FONT: Forced = Forced { name: "chat_typeface:", note: "mett_chat_typeface_restore" };
const ALL: [Forced; 2] = [PAUSE, FONT];

fn users() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Minecraft Bedrock").join("Users"))
}

/// Einstellungsdateien aller Konten.
fn files(users: &Path) -> Vec<PathBuf> {
    let Ok(dirs) = fs::read_dir(users) else { return vec![] };
    let files =
        dirs.flatten().map(|e| e.path().join("games").join("com.mojang").join("minecraftpe").join("options.txt"));
    files.filter(|p| p.is_file()).collect()
}

/// Wie viele Konten eine Einstellungsdatei haben.
pub fn accounts() -> usize {
    users().map_or(0, |u| files(&u).len())
}

/// Die zuletzt benutzte.
pub fn options_file() -> Option<PathBuf> {
    let found = files(&users()?).into_iter().filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)));
    found.max().map(|f| f.1)
}

/// Im Zweifel ja.
#[cfg(test)]
fn pauses(options: &str) -> bool {
    !options.lines().any(|l| l.trim() == format!("{}0", PAUSE.name))
}

/// Tauscht genau diesen Wert, alles andere bleibt Byte für Byte. `None`: steht nicht eindeutig so da.
fn swap(options: &str, name: &str, from: char, to: char) -> Option<String> {
    let line = format!("{name}{from}");
    let mut at = options.match_indices(&line).map(|m| m.0).filter(|i| {
        let end = i + line.len();
        (*i == 0 || options[..*i].ends_with('\n'))
            && options[end..].chars().next().is_none_or(|c| c == '\r' || c == '\n')
    });
    let (first, None) = (at.next()?, at.next()) else { return None };
    let value = first + name.len();
    Some(format!("{}{to}{}", &options[..value], &options[value + 1..]))
}

fn write(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("txt.mett");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

const STYLE_NOTE: &str = "mett_chat_style_restore";

/// Zeile `name:wert`.
fn line(options: &str, name: &str) -> Option<String> {
    let start = format!("{name}:");
    let mut it = options.lines().filter(|l| l.starts_with(&start));
    let (first, None) = (it.next()?, it.next()) else { return None };
    Some(first.trim().to_string())
}

const ZOOM_NOTE: &str = "mett_zoom_restore";
/// Kleiner lässt Minecraft das Sichtfeld nicht einstellen; so ein Wert in der Datei kann nur vom Zoom sein.
const LEAST_FOV: f32 = 30.0;

/// Vor dem Zoom: das Sichtfeld des Nutzers festhalten, falls Minecraft mitten im Zoom speichert.
pub fn remember_zoom() -> io::Result<()> {
    for p in users().map(|u| files(&u)).unwrap_or_default() {
        remember_zoom_in(&p)?;
    }
    Ok(())
}

fn fov(l: &str) -> Option<f32> {
    l.split_once(':')?.1.parse().ok()
}

fn remember_zoom_in(path: &Path) -> io::Result<()> {
    let Some(now) = line(&fs::read_to_string(path)?, FOV) else { return Ok(()) };
    // Ein schon verkleinerter Wert ist nicht der des Nutzers.
    if fov(&now).is_some_and(|v| v >= LEAST_FOV) {
        fs::write(path.with_file_name(ZOOM_NOTE), now)?;
    }
    Ok(())
}

fn restore_zoom(path: &Path) -> io::Result<()> {
    let note = path.with_file_name(ZOOM_NOTE);
    if !note.is_file() {
        return Ok(());
    }
    let (user, text) = (fs::read_to_string(&note)?, fs::read_to_string(path)?);
    if let Some(now) = line(&text, FOV)
        && fov(&now).is_some_and(|v| v < LEAST_FOV)
        && fov(&user).is_some_and(|v| v >= LEAST_FOV)
    {
        let ends = text
            .split_inclusive('\n')
            .map(|l| if l.trim_end() == now { l.replace(&now, &user) } else { l.to_string() });
        write(path, &ends.collect::<String>())?;
    }
    fs::remove_file(note)
}

/// Vor dem Java-Stil: die Werte des Nutzers festhalten, falls Minecraft zwischendurch unsere speichert.
pub fn remember_style() -> io::Result<()> {
    for p in users().map(|u| files(&u)).unwrap_or_default() {
        remember_style_in(&p)?;
    }
    Ok(())
}

fn remember_style_in(path: &Path) -> io::Result<()> {
    let note = path.with_file_name(STYLE_NOTE);
    // Ein vorhandener Zettel ist älter und kennt die echten Werte.
    if note.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(path)?;
    let lines: Vec<String> = JAVA.iter().filter_map(|(n, _)| line(&text, n)).collect();
    fs::write(note, lines.join("\n"))
}

/// Nach dem Spiel: Stehen noch unsere Werte da, kommen die gemerkten zurück.
fn restore_style(path: &Path) -> io::Result<()> {
    let note = path.with_file_name(STYLE_NOTE);
    if !note.is_file() {
        return Ok(());
    }
    let (old, mut text) = (fs::read_to_string(&note)?, fs::read_to_string(path)?);
    let before = text.clone();
    for (name, ours) in JAVA {
        let (Some(user), Some(now)) = (line(&old, name), line(&text, name)) else { continue };
        let value = now.split_once(':').and_then(|(_, v)| v.parse::<f32>().ok());
        if value.is_some_and(|v| (v - ours).abs() < 0.0005) && user != now {
            let ends = text
                .split_inclusive('\n')
                .map(|l| if l.trim_end() == now { l.replace(&now, &user) } else { l.to_string() });
            text = ends.collect();
        }
    }
    if text != before {
        write(path, &text)?;
    }
    fs::remove_file(note)
}

/// Merkzettel einer Zahlen-Einstellung: erst die Zeile des Nutzers, dann unsere Werte.
fn value_note(path: &Path, name: &str) -> PathBuf {
    path.with_file_name(format!("mett_{name}_restore"))
}

/// Vor dem Umstellen: den Wert des Nutzers festhalten, falls Minecraft zwischendurch unseren speichert.
pub fn remember_value(name: &str, ours: f32) -> io::Result<()> {
    for p in users().map(|u| files(&u)).unwrap_or_default() {
        remember_value_in(&p, name, ours)?;
    }
    Ok(())
}

fn remember_value_in(path: &Path, name: &str, ours: f32) -> io::Result<()> {
    let note = value_note(path, name);
    // Ein vorhandener Zettel ist älter und kennt den echten Wert.
    let mut text = match fs::read_to_string(&note) {
        Ok(t) => t,
        Err(_) => match line(&fs::read_to_string(path)?, name) {
            Some(user) => user,
            None => return Ok(()),
        },
    };
    let ours = ours.to_string();
    if !text.lines().skip(1).any(|l| l == ours) {
        text += &format!("\n{ours}");
    }
    fs::write(note, text)
}

/// Nach dem Spiel: Steht noch einer unserer Werte da, kommt der gemerkte zurück.
fn restore_value(path: &Path, name: &str) -> io::Result<()> {
    let note = value_note(path, name);
    if !note.is_file() {
        return Ok(());
    }
    let (old, text) = (fs::read_to_string(&note)?, fs::read_to_string(path)?);
    let mut lines = old.lines();
    let user = lines.next().unwrap_or_default();
    let ours: Vec<f32> = lines.filter_map(|l| l.parse().ok()).collect();
    if let Some(now) = line(&text, name)
        && fov(&now).is_some_and(|v| ours.iter().any(|o| (o - v).abs() < 0.0005))
        && user.starts_with(&format!("{name}:"))
    {
        let ends =
            text.split_inclusive('\n').map(|l| if l.trim_end() == now { l.replace(&now, user) } else { l.to_string() });
        write(path, &ends.collect::<String>())?;
    }
    fs::remove_file(note)
}

/// Vor dem Spielstart. Fehlt ein Eintrag, wird er nicht angefasst.
pub fn force() {
    each(force_file);
}

/// Nach dem Spiel, nur was wir selbst geändert haben.
pub fn restore() {
    each(restore_file);
    for p in users().map(|u| files(&u)).unwrap_or_default() {
        let _ = restore_style(&p);
        let _ = restore_zoom(&p);
        for (name, ..) in ALLOWED {
            let _ = restore_value(&p, name);
        }
    }
}

fn each(f: fn(&Path, &Forced) -> io::Result<()>) {
    for p in users().map(|u| files(&u)).unwrap_or_default() {
        // Eine Einstellung kann die andere nicht verhindern.
        for o in &ALL {
            let _ = f(&p, o);
        }
    }
}

fn force_file(path: &Path, o: &Forced) -> io::Result<()> {
    if let Some(new) = swap(&fs::read_to_string(path)?, o.name, '1', '0') {
        // Erst merken, dann ändern.
        fs::write(path.with_file_name(o.note), "")?;
        write(path, &new)?;
    }
    Ok(())
}

fn restore_file(path: &Path, o: &Forced) -> io::Result<()> {
    let note = path.with_file_name(o.note);
    if !note.is_file() {
        return Ok(());
    }
    if let Some(old) = swap(&fs::read_to_string(path)?, o.name, '0', '1') {
        write(path, &old)?;
    }
    // Erst wenn alles wieder stimmt.
    fs::remove_file(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: &str = "a:1\r\npause_menu_on_focus_lost:1\r\nb_pause_menu_on_focus_lost:1\r\nname:Grüße\r\n";

    fn account(name: &str, text: &str) -> PathBuf {
        let users = std::env::temp_dir().join(format!("mett-focus-{}-{name}", std::process::id()));
        let dir = users.join("123").join("games").join("com.mojang").join("minecraftpe");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("options.txt"), text).unwrap();
        assert_eq!(files(&users).len(), 1);
        dir.join("options.txt")
    }

    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap()
    }

    #[test]
    fn aus_und_wieder_an_ohne_rest() {
        let p = account("rund", ON);
        force_file(&p, &PAUSE).unwrap();
        assert!(!pauses(&read(&p)));
        assert_eq!(read(&p), ON.replacen("lost:1", "lost:0", 1));
        // Nach einem Absturz erneut gestartet: weiter aus, der Merkzettel bleibt.
        force_file(&p, &PAUSE).unwrap();
        restore_file(&p, &PAUSE).unwrap();
        assert_eq!(read(&p), ON);
        restore_file(&p, &PAUSE).unwrap();
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn eigene_wahl_des_nutzers_bleibt() {
        let off = ON.replacen("lost:1", "lost:0", 1);
        let p = account("aus", &off);
        force_file(&p, &PAUSE).unwrap();
        restore_file(&p, &PAUSE).unwrap();
        assert_eq!(read(&p), off);
        // Im Spiel wieder eingeschaltet: nichts zu tun, nur aufräumen.
        fs::write(&p, ON).unwrap();
        force_file(&p, &PAUSE).unwrap();
        fs::write(&p, ON).unwrap();
        restore_file(&p, &PAUSE).unwrap();
        assert_eq!(read(&p), ON);
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn gespeicherter_java_stil_kommt_zurueck() {
        let user = "a:1\r\nchat_line_spacing:1\r\ngfx_chat_background_opacity:0.7\r\nb:-1\r\n";
        let ours = user.replace("spacing:1", "spacing:-1.001").replace("0.7", "0.5");
        let p = account("stil", user);
        remember_style_in(&p).unwrap();
        fs::write(&p, &ours).unwrap();
        // Nach einem Absturz: Der alte Zettel bleibt.
        remember_style_in(&p).unwrap();
        restore_style(&p).unwrap();
        assert_eq!(read(&p), user);
        // Der Nutzer hat selbst etwas anderes gewählt.
        remember_style_in(&p).unwrap();
        let chosen = user.replace("0.7", "0.2");
        fs::write(&p, &chosen).unwrap();
        restore_style(&p).unwrap();
        assert_eq!(read(&p), chosen);
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn verkleinertes_sichtfeld_kommt_zurueck() {
        let user = "a:1\r\ngfx_field_of_view:110\r\n";
        let p = account("zoom", user);
        remember_zoom_in(&p).unwrap();
        fs::write(&p, user.replace("110", "27.5")).unwrap();
        // Noch ein Start nach einem Absturz: Der verkleinerte Wert wird nicht gemerkt.
        remember_zoom_in(&p).unwrap();
        restore_zoom(&p).unwrap();
        assert_eq!(read(&p), user);
        // Der Nutzer hat selbst umgestellt.
        remember_zoom_in(&p).unwrap();
        fs::write(&p, user.replace("110", "80")).unwrap();
        restore_zoom(&p).unwrap();
        assert_eq!(read(&p), user.replace("110", "80"));
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn umgestellte_zahl_kommt_zurueck() {
        let user = "a:1\r\ngfx_gamma:0.8\r\n";
        let p = account("gamma", user);
        remember_value_in(&p, "gfx_gamma", 10.0).unwrap();
        fs::write(&p, user.replace("0.8", "10")).unwrap();
        // Regler bewegt, oder neuer Start nach einem Absturz: Der Wert des Nutzers bleibt der erste.
        remember_value_in(&p, "gfx_gamma", 5.5).unwrap();
        fs::write(&p, user.replace("0.8", "5.5")).unwrap();
        restore_value(&p, "gfx_gamma").unwrap();
        assert_eq!(read(&p), user);
        // Der Nutzer hat selbst umgestellt.
        remember_value_in(&p, "gfx_gamma", 10.0).unwrap();
        fs::write(&p, user.replace("0.8", "0.2")).unwrap();
        restore_value(&p, "gfx_gamma").unwrap();
        assert_eq!(read(&p), user.replace("0.8", "0.2"));
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn jede_einstellung_fuer_sich() {
        let text = "chat_typeface:1
pause_menu_on_focus_lost:0
";
        let p = account("schrift", text);
        ALL.iter().for_each(|o| force_file(&p, o).unwrap());
        assert_eq!(read(&p), text.replacen("typeface:1", "typeface:0", 1));
        // Die schon ausgeschaltete Pause hat keinen Merkzettel und bleibt aus.
        ALL.iter().for_each(|o| restore_file(&p, o).unwrap());
        assert_eq!(read(&p), text);
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn unklares_wird_nicht_angefasst() {
        for t in ["", "a:1", "pause_menu_on_focus_lost:12\n", "pause_menu_on_focus_lost:1\npause_menu_on_focus_lost:1"]
        {
            assert_eq!(swap(t, PAUSE.name, '1', '0'), None, "{t}");
            assert!(pauses(t));
        }
        assert_eq!(
            swap("pause_menu_on_focus_lost:1", PAUSE.name, '1', '0').as_deref(),
            Some("pause_menu_on_focus_lost:0")
        );
    }
}
