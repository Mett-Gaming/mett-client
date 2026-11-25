//! Resource Pack für den Knopf im Pausenmenü: nur da, solange über den Mett Launcher gespielt wird.
//!
//! Alles ist mehrfach ausführbar und fasst ausschließlich eigene Einträge an.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::{fs, io};

const MANIFEST: &str = include_str!("../../../pack/manifest.json");
const PAUSE: &str = include_str!("../../../pack/ui/pause_screen.json");
const HOVER: &[u8] = include_bytes!("../../../pack/textures/ui/mett_hover.png");
const HOVER_SLICE: &str = include_str!("../../../pack/textures/ui/mett_hover.json");
/// Blendet nur die Anzeige im HUD aus; mit T bleibt der Chat erreichbar.
const HIDE_CHAT: &str = include_str!("../../../pack/ui/hud_screen.json");
const FOLDER: &str = "MettClient";

fn users() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Minecraft Bedrock").join("Users"))
}

pub fn install() -> io::Result<()> {
    install_in(&users().ok_or(io::ErrorKind::NotFound)?, false)
}

/// Blendet das liegende Pack Minecrafts Chat-Anzeige aus?
pub fn hides_chat() -> bool {
    users().is_some_and(|u| pack_dir(&u).join("ui").join("hud_screen.json").is_file())
}

pub fn remove() -> io::Result<()> {
    remove_in(&users().ok_or(io::ErrorKind::NotFound)?)
}

fn header() -> Value {
    serde_json::from_str::<Value>(MANIFEST).expect("eigenes Manifest")["header"].clone()
}

/// Mit ausgeblendetem Chat zählt die letzte Versionsstelle eins höher, damit Minecraft nie den falschen Stand behält.
/// Neue Pack-Versionen deshalb immer um zwei erhöhen.
fn manifest(hide_chat: bool) -> Value {
    let mut m: Value = serde_json::from_str(MANIFEST).expect("eigenes Manifest");
    if hide_chat {
        for path in ["/header/version/2", "/modules/0/version/2"] {
            if let Some(v) = m.pointer_mut(path) {
                *v = json!(v.as_u64().unwrap_or(0) + 1);
            }
        }
    }
    m
}

fn pack_dir(users: &Path) -> PathBuf {
    users.join("Shared").join("games").join("com.mojang").join("resource_packs").join(FOLDER)
}

/// Aktive Packs jedes Kontos.
fn lists(users: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = vec![];
    for e in fs::read_dir(users)? {
        let e = e?;
        let dir = e.path().join("games").join("com.mojang").join("minecraftpe");
        if e.file_name() != "Shared" && dir.is_dir() {
            found.push(dir.join("global_resource_packs.json"));
        }
    }
    Ok(found)
}

/// `hide_chat`: nur noch für Prüfungen, Minecrafts Anzeige wird im Spiel verschoben statt ausgeblendet.
fn install_in(users: &Path, hide_chat: bool) -> io::Result<()> {
    let dir = pack_dir(users);
    if dir.exists() && !is_ours(&dir) {
        return Err(io::Error::other("Ein fremder Ordner heißt wie das Pack"));
    }
    fs::create_dir_all(dir.join("ui"))?;
    fs::write(dir.join("ui").join("pause_screen.json"), PAUSE)?;
    let hud = dir.join("ui").join("hud_screen.json");
    if hide_chat {
        fs::write(hud, HIDE_CHAT)?;
    } else if hud.exists() {
        fs::remove_file(hud)?;
    }
    let textures = dir.join("textures").join("ui");
    fs::create_dir_all(&textures)?;
    fs::write(textures.join("mett_hover.png"), HOVER)?;
    fs::write(textures.join("mett_hover.json"), HOVER_SLICE)?;
    let m = manifest(hide_chat);
    fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&m).map_err(io::Error::other)?)?;
    let h = &m["header"];
    for list in lists(users)? {
        edit(&list, |packs| {
            packs.retain(|p| p["pack_id"] != h["uuid"]);
            packs.insert(0, json!({ "pack_id": h["uuid"], "version": h["version"] }));
        })?;
    }
    Ok(())
}

fn remove_in(users: &Path) -> io::Result<()> {
    // Alles versuchen, auch wenn ein Schritt scheitert.
    let mut result = Ok(());
    let uuid = header()["uuid"].clone();
    for list in lists(users).unwrap_or_default() {
        if list.exists() {
            result = edit(&list, |packs| packs.retain(|p| p["pack_id"] != uuid)).and(result);
        }
    }
    let dir = pack_dir(users);
    if is_ours(&dir) {
        result = fs::remove_dir_all(&dir).and(result);
    }
    result
}

/// Nur ein Ordner mit unserer Kennung darf überschrieben oder gelöscht werden.
fn is_ours(dir: &Path) -> bool {
    let manifest = fs::read_to_string(dir.join("manifest.json")).ok();
    let value = manifest.and_then(|t| serde_json::from_str::<Value>(&t).ok());
    value.is_some_and(|v| v["header"]["uuid"] == header()["uuid"])
}

/// Ändert die Liste; Unlesbares bleibt unberührt, Unverändertes wird nicht geschrieben.
fn edit(path: &Path, change: impl FnOnce(&mut Vec<Value>)) -> io::Result<()> {
    let text = match fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => "[]".into(),
        r => r?,
    };
    let text = if text.trim().is_empty() { "[]" } else { text.trim() };
    let old: Vec<Value> = serde_json::from_str(text).map_err(io::Error::other)?;
    let mut new = old.clone();
    change(&mut new);
    if new == old {
        return Ok(());
    }
    let tmp = path.with_extension("json.mett-tmp");
    fs::write(&tmp, serde_json::to_vec(&new).map_err(io::Error::other)?)?;
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> (PathBuf, PathBuf) {
        let users = std::env::temp_dir().join(format!("mett-pack-{}-{name}", std::process::id()));
        let mc = users.join("123").join("games").join("com.mojang").join("minecraftpe");
        fs::create_dir_all(&mc).unwrap();
        fs::create_dir_all(pack_dir(&users).parent().unwrap()).unwrap();
        (users, mc.join("global_resource_packs.json"))
    }

    #[test]
    fn fremde_packs_bleiben_erhalten() {
        let (u, list) = setup("a");
        let other = r#"[{"pack_id":"abc","version":[1,2,3]}]"#;
        fs::write(&list, other).unwrap();
        install_in(&u, false).unwrap();
        install_in(&u, false).unwrap();
        let v: Vec<Value> = serde_json::from_str(&fs::read_to_string(&list).unwrap()).unwrap();
        assert_eq!((v.len(), &v[1]["pack_id"]), (2, &json!("abc")));
        assert!(pack_dir(&u).join("ui").join("pause_screen.json").exists());
        remove_in(&u).unwrap();
        remove_in(&u).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&list).unwrap()).unwrap();
        assert_eq!(v, serde_json::from_str::<Value>(other).unwrap());
        assert!(!pack_dir(&u).exists());
    }

    #[test]
    fn ohne_liste_wird_eine_angelegt() {
        let (u, list) = setup("b");
        install_in(&u, false).unwrap();
        assert!(list.exists());
    }

    #[test]
    fn unlesbare_liste_und_fremder_ordner_bleiben() {
        let (u, list) = setup("c");
        fs::write(&list, "{kaputt").unwrap();
        assert!(install_in(&u, false).is_err());
        assert!(remove_in(&u).is_err());
        assert_eq!(fs::read_to_string(&list).unwrap(), "{kaputt");
        assert!(!pack_dir(&u).exists());

        let (u, _) = setup("d");
        fs::create_dir_all(pack_dir(&u)).unwrap();
        fs::write(pack_dir(&u).join("manifest.json"), r#"{"header":{"uuid":"fremd"}}"#).unwrap();
        assert!(install_in(&u, false).is_err());
        remove_in(&u).unwrap();
        assert!(pack_dir(&u).exists());
    }

    #[test]
    fn chat_ausblenden_nur_wenn_gewuenscht() {
        let (u, list) = setup("e");
        let hud = pack_dir(&u).join("ui").join("hud_screen.json");
        let version = || {
            let v: Value = serde_json::from_str(&fs::read_to_string(&list).unwrap()).unwrap();
            let m: Value =
                serde_json::from_str(&fs::read_to_string(pack_dir(&u).join("manifest.json")).unwrap()).unwrap();
            // Liste und Manifest müssen zusammenpassen, sonst lädt Minecraft das Pack nicht.
            assert_eq!(v[0]["version"], m["header"]["version"]);
            v[0]["version"].clone()
        };
        install_in(&u, true).unwrap();
        assert!(hud.exists() && is_ours(&pack_dir(&u)));
        let hidden = version();
        install_in(&u, false).unwrap();
        assert!(!hud.exists());
        assert_ne!(version(), hidden);
        remove_in(&u).unwrap();
        assert!(!pack_dir(&u).exists());
    }

    #[test]
    fn pack_dateien_sind_gueltig() {
        assert!(header()["uuid"].is_string());
        assert_eq!(serde_json::from_str::<Value>(HIDE_CHAT).unwrap()["chat_panel"]["ignored"], true);
        let p: Value = serde_json::from_str(PAUSE).unwrap();
        let button = &p["mett_client_button@common_buttons.light_text_button"];
        assert_eq!(button["$pressed_button_name"], "button.menu_continue");
        assert_eq!(button["$hover_button_texture"], "textures/ui/mett_hover");
        assert!(HOVER[1..].starts_with(b"PNG"));
        // Ohne Nineslice füllt die Textur nur ein Quadrat in der Knopfmitte.
        assert_eq!(serde_json::from_str::<Value>(HOVER_SLICE).unwrap()["nineslice_size"], 1);
    }
}
