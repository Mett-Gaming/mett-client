//! Einstellungen mit Profilen, versioniert und atomar geschrieben.
use hud::{Anchor, Crosshair, Kind, Placement, Style};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

pub const VERSION: u32 = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub enabled: bool,
    pub place: Placement,
    #[serde(default)]
    pub style: Style,
}

impl Module {
    fn style(kind: Kind) -> Style {
        match kind {
            Kind::Chat => Style { background: 0, opacity: 0.5, text: 0xFFFFFF, ..Style::default() },
            _ => Style::default(),
        }
    }

    pub fn standard(kind: Kind) -> Self {
        let (anchor, x, y) = match kind {
            Kind::Keystrokes => (Anchor::BottomLeft, 16.0, -16.0),
            // Wie in der Java Edition: ganz links, 40 Pixel über dem unteren Rand.
            Kind::Chat => (Anchor::BottomLeft, 0.0, -hud::java_chat::BOTTOM),
            // Übrige Module untereinander, damit sie sich beim Einschalten nicht verdecken.
            k => (Anchor::TopLeft, 16.0, 16.0 + 34.0 * (k as u8 - 1) as f32),
        };
        // Nur das Wichtigste ist von Anfang an sichtbar.
        let enabled = matches!(kind, Kind::Keystrokes | Kind::Cps | Kind::Clock);
        Self { enabled, place: Placement { anchor, x, y, scale: 1.0, w: 1.0, h: 1.0 }, style: Self::style(kind) }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub modules: BTreeMap<Kind, Module>,
    pub crosshair_enabled: bool,
    /// Server für den Ping.
    pub ping_host: String,
    pub note: String,
    pub crosshair: Crosshair,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Standard".into(),
            modules: Kind::ALL.map(|k| (k, Module::standard(k))).into(),
            crosshair_enabled: false,
            ping_host: String::new(),
            note: String::new(),
            crosshair: Crosshair { length: 6.0, gap: 3.0, thickness: 2.0, dot: false, color: 0xE8E0D1, alpha: 0.9 },
        }
    }
}

impl Profile {
    /// Ergänzt Module, die es beim Speichern noch nicht gab.
    fn complete(&mut self) {
        for k in Kind::ALL {
            self.modules.entry(k).or_insert_with(|| Module::standard(k));
        }
    }

    pub fn apply_to_all(&mut self, style: Style) {
        for m in self.modules.values_mut() {
            m.style = style;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub hide_in_menus: bool,
    pub snap: bool,
    /// So viele Nachrichten zeigt Minecrafts Anzeige höchstens.
    pub chat_entries: u32,
    /// Nachrichten sanft ausblenden statt sie einfach verschwinden zu lassen.
    pub chat_fade: bool,
    /// Chat wie in der Java Edition.
    pub chat_java: bool,
    /// Früher ein eigener Schalter, heute das Modul „Spielerliste“.
    #[serde(skip_serializing)]
    pub tab_list: bool,
    pub zoom: hud::zoom::Settings,
    /// AppleSkin: Sättigung und Erschöpfung zeigen.
    pub apple_saturation: bool,
    pub apple_exhaustion: bool,
    /// Eigene Chat-Eingabe; lädt dafür das Chat-Plugin ins Spiel.
    pub chat_input: bool,
    pub active: usize,
    pub profiles: Vec<Profile>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: VERSION,
            hide_in_menus: true,
            snap: true,
            chat_entries: 10,
            chat_fade: true,
            chat_java: false,
            tab_list: false,
            zoom: Default::default(),
            apple_saturation: true,
            apple_exhaustion: true,
            chat_input: false,
            active: 0,
            profiles: vec![Profile::default()],
        }
    }
}

impl Config {
    pub fn profile(&self) -> &Profile {
        &self.profiles[self.active]
    }

    pub fn profile_mut(&mut self) -> &mut Profile {
        &mut self.profiles[self.active]
    }

    pub fn chat_entries(&self) -> u32 {
        self.chat_entries.clamp(1, 40)
    }

    /// Hängt ein Profil mit freiem Namen an und wählt es aus.
    pub fn add(&mut self, mut p: Profile) {
        p.complete();
        let base = p.name.clone();
        let mut n = 2;
        while self.profiles.iter().any(|o| o.name == p.name) {
            p.name = format!("{base} {n}");
            n += 1;
        }
        self.profiles.push(p);
        self.active = self.profiles.len() - 1;
    }

    /// Alles auf Anfang und nichts eingeschaltet, auch was sonst von Anfang an sichtbar ist. Ein einziges Profil bleibt.
    pub fn reset(&mut self) {
        *self = Self::default();
        self.profile_mut().modules.values_mut().for_each(|m| m.enabled = false);
    }

    /// Löscht das aktive Profil; das letzte bleibt.
    pub fn remove_active(&mut self) -> bool {
        if self.profiles.len() < 2 {
            return false;
        }
        self.profiles.remove(self.active);
        self.active = self.active.min(self.profiles.len() - 1);
        true
    }

    fn repair(mut self) -> Self {
        if self.profiles.is_empty() {
            self.profiles.push(Profile::default());
        }
        self.active = self.active.min(self.profiles.len() - 1);
        self.profiles.iter_mut().for_each(Profile::complete);
        self.version = VERSION;
        self
    }
}

/// Sicherung vor dem Zurücksetzen, neben der Datei im Ordner `folder`.
pub fn backup(folder: &Path, c: &Config) -> io::Result<PathBuf> {
    fs::create_dir_all(folder)?;
    let path = folder.join("vor-dem-zuruecksetzen.bak");
    save(&path, c)?;
    Ok(path)
}

pub fn default_path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("MettGaming").join("MettClient").join("config.json"))
}

/// Fehlt die Datei, gelten Standardwerte. Eine kaputte oder neuere Datei ist ein Fehler.
pub fn load(path: &Path) -> io::Result<Config> {
    let text = match fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        r => r?,
    };
    let value: serde_json::Value = serde_json::from_str(&text).map_err(io::Error::other)?;
    let version = value["version"].as_u64().unwrap_or(1);
    if version > VERSION as u64 {
        return Err(io::Error::other(format!("Version {version} ist neuer als dieses Programm")));
    }
    let value = if version < 2 { from_v1(value) } else { value };
    let mut c = serde_json::from_value::<Config>(value).map(Config::repair).map_err(io::Error::other)?;
    if version < 3 {
        // Der Chat rechnet seit Version 3 in Java-Pixeln; alte Lagen passen nicht mehr.
        for m in c.profiles.iter_mut().filter_map(|p| p.modules.get_mut(&Kind::Chat)) {
            *m = Module { enabled: m.enabled, ..Module::standard(Kind::Chat) };
        }
    }
    if version < 4 {
        // Seit Version 4 sind alle Textfelder anfangs gleich groß; ihr Platz bleibt.
        for (_, m) in c.profiles.iter_mut().flat_map(|p| &mut p.modules).filter(|(k, _)| k.label()) {
            (m.place.scale, m.place.w, m.place.h) = (1.0, 1.0, 1.0);
        }
    }
    if c.tab_list {
        // Aus dem Schalter wurde ein Modul.
        c.tab_list = false;
        c.profiles.iter_mut().filter_map(|p| p.modules.get_mut(&Kind::Players)).for_each(|m| m.enabled = true);
    }
    c.zoom = c.zoom.sane();
    Ok(c)
}

/// Version 1 kannte keine Profile; die Module standen direkt in der Datei.
fn from_v1(old: serde_json::Value) -> serde_json::Value {
    let mut profile = serde_json::Map::new();
    let mut modules = serde_json::Map::new();
    let mut new = serde_json::Map::new();
    if let serde_json::Value::Object(o) = old {
        for (k, v) in o {
            match k.as_str() {
                "keystrokes" | "cps" | "clock" => modules.insert(k, v),
                "crosshair" | "crosshair_enabled" => profile.insert(k, v),
                "hide_in_menus" => new.insert(k, v),
                _ => None,
            };
        }
    }
    profile.insert("modules".into(), modules.into());
    new.insert("profiles".into(), vec![serde_json::Value::Object(profile)].into());
    new.into()
}

pub fn save(path: &Path, c: &Config) -> io::Result<()> {
    write(path, c)
}

fn write(path: &Path, value: &impl Serialize) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value).map_err(io::Error::other)?)?;
    fs::rename(tmp, path)
}

/// Schreibt ein Profil zum Teilen in den Ordner.
pub fn export(dir: &Path, p: &Profile) -> io::Result<PathBuf> {
    let safe: String = p.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
    let path = dir.join(format!("{}.json", safe.trim()));
    write(&path, p)?;
    Ok(path)
}

/// Liest alle Profile des Ordners. Kaputte Dateien werden nur gezählt.
pub fn import(dir: &Path) -> io::Result<(Vec<Profile>, usize)> {
    let (mut good, mut bad) = (vec![], 0);
    for e in fs::read_dir(dir)? {
        let p = e?.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        match fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Profile>(&t).ok()) {
            Some(mut x) => {
                x.complete();
                good.push(x)
            }
            None => bad += 1,
        }
    }
    Ok((good, bad))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mett-config-{}-{name}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d.join("config.json")
    }

    #[test]
    fn rundreise_und_ueberschreiben() {
        let p = file("a");
        let mut c = load(&p).unwrap();
        assert_eq!(c, Config::default());
        save(&p, &c).unwrap();
        c.profile_mut().crosshair_enabled = true;
        save(&p, &c).unwrap();
        assert_eq!(load(&p).unwrap(), c);
    }

    #[test]
    fn version_1_wird_uebernommen() {
        let p = file("b");
        let t = r#"{"version":1,"hide_in_menus":false,"crosshair_enabled":true,
            "clock":{"enabled":false,"place":{"anchor":"right","x":1.0,"y":2.0,"scale":1.5}}}"#;
        fs::write(&p, t).unwrap();
        let c = load(&p).unwrap();
        assert!(!c.hide_in_menus && c.profile().crosshair_enabled && c.version == VERSION);
        let clock = &c.profile().modules[&Kind::Clock];
        // Größen sind seit Version 4 anfangs überall gleich.
        assert!(!clock.enabled && clock.place.scale == 1.0);
        assert_eq!(c.profile().modules[&Kind::Cps], Module::standard(Kind::Cps));
    }

    #[test]
    fn neue_module_kommen_ausgeschaltet_dazu() {
        let p = file("neu");
        fs::write(&p, r#"{"version":2,"profiles":[{"name":"A","modules":{}}]}"#).unwrap();
        let c = load(&p).unwrap();
        assert_eq!(c.profile().modules.len(), Kind::ALL.len());
        assert!(!c.profile().modules[&Kind::Fps].enabled && c.profile().modules[&Kind::Clock].enabled);
    }

    #[test]
    fn textfelder_werden_einmal_gleich_gross() {
        let p = file("gross");
        let m = |h: f32| {
            format!(r#"{{"enabled":true,"place":{{"anchor":"top-left","x":7.0,"y":9.0,"scale":1.5,"w":1.2,"h":{h}}}}}"#)
        };
        let text =
            |v: u32| format!(r#"{{"version":{v},"profiles":[{{"modules":{{"ping":{},"chat":{}}}}}]}}"#, m(1.5), m(0.9));
        fs::write(&p, text(3)).unwrap();
        let c = load(&p).unwrap();
        let place = |c: &Config, k| c.profile().modules[&k].place;
        let a = place(&c, Kind::Ping);
        assert_eq!((a.x, a.y, a.scale, a.w, a.h), (7.0, 9.0, 1.0, 1.0, 1.0));
        // Der Chat behält seine Größe, und später gewählte Größen bleiben.
        assert_eq!(place(&c, Kind::Chat).h, 0.9);
        fs::write(&p, text(4)).unwrap();
        assert_eq!(place(&load(&p).unwrap(), Kind::Ping).h, 1.5);
    }

    #[test]
    fn alter_schalter_wird_zum_modul() {
        let p = file("liste");
        fs::write(&p, r#"{"version":3,"tab_list":true,"zoom":{"initial":-5.0}}"#).unwrap();
        let c = load(&p).unwrap();
        assert!(c.profile().modules[&Kind::Players].enabled && !c.profile().modules[&Kind::Zoom].enabled);
        assert_eq!((c.zoom.initial, c.zoom.time_in, c.zoom.second_key), (1.0, 1.0, 0));
        save(&p, &c).unwrap();
        assert!(!fs::read_to_string(&p).unwrap().contains("tab_list"));
        assert_eq!(load(&p).unwrap(), c);
    }

    #[test]
    fn alte_chatlage_wird_zurueckgesetzt() {
        let p = file("chat");
        let t = r#"{"version":2,"profiles":[{"modules":{"chat":
            {"enabled":true,"place":{"anchor":"left","x":16.0,"y":60.0,"scale":1.0}}}}]}"#;
        fs::write(&p, t).unwrap();
        let m = load(&p).unwrap().profile().modules[&Kind::Chat].clone();
        assert!(m.enabled && m.place == Module::standard(Kind::Chat).place);
    }

    #[test]
    fn kaputt_und_zu_neu_sind_fehler() {
        let p = file("c");
        for t in ["{", r#"{"version":99}"#] {
            fs::write(&p, t).unwrap();
            assert!(load(&p).is_err());
        }
    }

    #[test]
    fn unsinnige_profile_werden_repariert() {
        let p = file("d");
        fs::write(&p, r#"{"version":2,"active":9,"profiles":[]}"#).unwrap();
        assert_eq!(load(&p).unwrap(), Config::default());
    }

    #[test]
    fn profile_anlegen_und_loeschen() {
        let mut c = Config::default();
        assert!(!c.remove_active());
        c.add(Profile::default());
        assert_eq!((c.active, c.profile().name.as_str()), (1, "Standard 2"));
        assert!(c.remove_active());
        assert_eq!(c.active, 0);
    }

    #[test]
    fn teilen_und_kaputte_datei_ueberspringen() {
        let d = file("e").parent().unwrap().join("teilen");
        let p = Profile { name: "A/B: ..".into(), ..Profile::default() };
        assert_eq!(export(&d, &p).unwrap().parent().unwrap(), d);
        fs::write(d.join("kaputt.json"), "{").unwrap();
        assert_eq!(import(&d).unwrap(), (vec![p], 1));
    }
}
