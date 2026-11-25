//! Deutsch und Englisch, je nach Sprache des Spiels. Im Programm stehen die deutschen Texte, hier ihre Übersetzung.
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Aus Minecrafts Einstellungen. Alles außer Deutsch wird Englisch.
pub fn set_from(options: &str) {
    ENGLISH.store(!is_german(options), Relaxed);
}

fn is_german(options: &str) -> bool {
    let value = options.lines().find_map(|l| l.trim().strip_prefix("game_language:"));
    value.is_some_and(|v| v.trim().to_lowercase().starts_with("de"))
}

/// Text in der Sprache des Spiels. Unbekanntes bleibt, wie es ist.
pub fn t(german: &str) -> &str {
    if ENGLISH.load(Relaxed) { english(german).unwrap_or(german) } else { german }
}

pub fn english(german: &str) -> Option<&'static str> {
    static MAP: OnceLock<HashMap<&str, &str>> = OnceLock::new();
    MAP.get_or_init(|| TEXTS.iter().copied().collect()).get(german).copied()
}

#[rustfmt::skip]
const TEXTS: &[(&str, &str)] = &[
    // Module
    ("Tasten", "Keystrokes"),
    ("Uhr", "Clock"),
    ("System", "System"),
    ("Akku", "Battery"),
    ("Musik", "Music"),
    ("Spielzeit", "Playtime"),
    ("Stoppuhr", "Stopwatch"),
    ("Notiz", "Note"),
    ("Spielerliste", "Player list"),
    ("Leertaste", "Space"),
    ("Schnelligkeit", "Speed"),
    ("Langsamkeit", "Slowness"),
    ("Eile", "Haste"),
    ("Abbaulähmung", "Mining Fatigue"),
    ("Stärke", "Strength"),
    ("Direktheilung", "Instant Health"),
    ("Direktschaden", "Instant Damage"),
    ("Sprungkraft", "Jump Boost"),
    ("Übelkeit", "Nausea"),
    ("Resistenz", "Resistance"),
    ("Feuerschutz", "Fire Resistance"),
    ("Wasseratmung", "Water Breathing"),
    ("Unsichtbarkeit", "Invisibility"),
    ("Blindheit", "Blindness"),
    ("Nachtsicht", "Night Vision"),
    ("Schwäche", "Weakness"),
    ("Vergiftung", "Poison"),
    ("Ausdörrung", "Wither"),
    ("Extraleben", "Health Boost"),
    ("Schwebkraft", "Levitation"),
    ("Tödliches Gift", "Fatal Poison"),
    ("Meereskraft", "Conduit Power"),
    ("Sanfter Fall", "Slow Falling"),
    ("Böses Omen", "Bad Omen"),
    ("Held des Dorfes", "Village Hero"),
    ("Dunkelheit", "Darkness"),
    ("Effekt", "Effect"),
    ("Rüstung", "Armor"),
    ("Effekte", "Effects"),
    ("Gegenstandszähler", "Item counter"),
    ("Gegenstände", "Items"),
    ("Deine Rüstung neben der Schnellleiste.", "Your armor beside the hotbar."),
    ("An der Schnellleiste", "Sticky HUD"),
    ("Senkrecht", "Vertical"),
    ("Umgekehrte Reihenfolge", "Reversed"),
    ("Symbole leerer Plätze", "Show icons"),
    ("Warnzeichen", "Show warning"),
    ("Seite", "Side"),
    ("Links", "Left"),
    ("Rechts", "Right"),
    ("Beide Seiten", "Both sides"),
    ("Anzeigen", "Show widget"),
    ("Immer", "Always"),
    ("Wenn etwas angelegt ist", "If any present"),
    ("Nur angelegte Teile", "Not empty"),
    ("Nur fast kaputte", "Damaged pieces"),
    ("Haltbarkeit", "Durability"),
    ("Balken", "Bar"),
    ("Zahl", "Numeric"),
    ("Prozent", "Percentage"),
    ("Wippen der Warnung", "Bob intensity"),
    ("Warnen ab Haltbarkeit", "Min durability"),
    ("Warnen ab Prozent", "Min durability %"),
    ("Deine Effekte und wie lange sie noch laufen.", "Your effects and how long they last."),
    ("Zählt Gegenstände in deinem Inventar.", "Counts items in your inventory."),
    ("Liest nur deine eigenen Werte aus dem Spielspeicher.", "Reads only your own values from game memory."),
    ("eigenes Inventar nicht eindeutig gefunden", "own inventory not found reliably"),
    ("keine Effekte gefunden", "no effects found"),
    ("LMT", "LMB"),
    ("RMT", "RMB"),
    ("Auch im Dunkeln alles hell.", "Everything bright, even in the dark."),
    ("Helligkeit", "Brightness"),
    ("Stellt eine Einstellung von Minecraft um und am Ende zurück. Serverregeln können das verbieten.", "Changes a Minecraft setting and restores it afterwards. Server rules may forbid this."),
    ("Zeigt gedrückte Tasten und Maustasten.", "Shows pressed keys and mouse buttons."),
    ("Klicks pro Sekunde, links und rechts.", "Clicks per second, left and right."),
    ("Aktuelle Uhrzeit.", "Current time."),
    ("Bilder pro Sekunde.", "Frames per second."),
    ("Auslastung von CPU, RAM und Grafikkarte.", "CPU, RAM and graphics card usage."),
    ("Ladestand des Akkus.", "Battery level."),
    ("Was gerade läuft, z. B. in Spotify.", "What is playing, e.g. in Spotify."),
    ("Antwortzeit des Servers, auf dem du spielst.", "Response time of the server you are on."),
    ("Wie lange Minecraft schon läuft.", "How long Minecraft has been running."),
    ("Stoppuhr: F9 Start und Pause, F10 zurück.", "Stopwatch: F9 start and pause, F10 reset."),
    ("Eigener Text auf dem Bildschirm.", "Your own text on screen."),
    ("Zoom mit weichem Verlauf und Mausrad.", "Smooth zoom with mouse wheel."),
    ("Sättigung und Erschöpfung an der Hungerleiste.", "Saturation and exhaustion on the hunger bar."),
    ("Spielerliste wie in Java, solange Tab gehalten wird.", "Java style player list while Tab is held."),
    ("Minecrafts Chat, frei platziert, mit eigener Eingabe.", "Minecraft's chat, freely placed, with its own input."),
    // Anzeigen
    ("System …", "System …"),
    ("Akku …", "Battery …"),
    ("Musik …", "Music …"),
    ("Ping …", "Ping …"),
    ("Kein Akku", "No battery"),
    ("Keine Musik", "No music"),
    ("Musik nicht lesbar", "Music unavailable"),
    ("Kein Server", "No server"),
    ("Keine Antwort", "No response"),
    ("FPS: Recht fehlt", "FPS: no permission"),
    ("FPS nicht messbar", "FPS unavailable"),
    ("Chat nicht lesbar", "Chat not readable"),
    ("Chat nicht eindeutig", "Chat not unique"),
    // Menü
    ("MODULE", "MODULES"),
    ("OPTIONEN", "OPTIONS"),
    ("FERTIG", "DONE"),
    ("PROFILE", "PROFILES"),
    ("MENÜ", "MENU"),
    ("AN", "ON"),
    ("AUS", "OFF"),
    ("Suchen…", "Search…"),
    ("Nichts gefunden.", "Nothing found."),
    ("Profil", "Profile"),
    ("Größe", "Size"),
    ("Stil", "Style"),
    ("Deckkraft", "Opacity"),
    ("Ecken", "Corners"),
    ("Hintergrund", "Background"),
    ("Schriftfarbe", "Text color"),
    ("Akzentfarbe", "Accent color"),
    ("Umriss", "Outline"),
    ("Karten", "Cards"),
    ("Rahmen", "Borders"),
    ("Nebentext", "Secondary text"),
    ("Farbe", "Color"),
    ("Text", "Text"),
    ("Hell", "Light"),
    ("Kontrast", "Contrast"),
    ("Nur Text", "Text only"),
    ("Eigener", "Custom"),
    ("Globales Design benutzen", "Use global"),
    ("Gilt für jedes Modul, das das globale Design benutzt.", "Applies to every module that uses the global design."),
    ("Aussehen dieses Fensters.", "Appearance of this window."),
    ("Klicken und tippen", "Click and type"),
    ("Misst zu dem Server, mit dem du gerade verbunden bist. Liest dafür den Spielspeicher.", "Measures to the server you are connected to. Reads game memory for that."),
    // Chat
    ("Liest den Spielspeicher. Mit eigener Eingabe läuft Code im Spiel. Serverregeln können beides verbieten.", "Reads game memory. Custom input runs code in the game. Server rules may forbid both."),
    ("Eigene Eingabe mit T (lädt ein Plugin ins Spiel)", "Custom input with T (loads a plugin into the game)"),
    ("Eingabe", "Input"),
    ("Chat-Anzeige", "Chat display"),
    ("Höchstens Nachrichten", "Maximum messages"),
    ("Nachrichten sanft ausblenden", "Fade messages out"),
    ("Aussehen wie in der Java Edition", "Java Edition look"),
    ("Java-Aussehen", "Java look"),
    ("Größe zurücksetzen", "Reset size"),
    ("Nachrichten zeigt Minecraft selbst, hierher verschoben. Breite und Höhe ziehst du am Rand.", "Minecraft shows the messages itself, moved here. Drag the edges for width and height."),
    // AppleSkin, Spielerliste
    ("Sättigung", "Saturation"),
    ("Erschöpfung", "Exhaustion"),
    ("Liest nur deine eigenen Werte. Nach dem Betreten erscheint es, sobald sich einer davon ändert.", "Reads only your own values. After joining it appears once one of them changes."),
    ("Liest den Spielspeicher. Serverregeln können das verbieten.", "Reads game memory. Server rules may forbid this."),
    ("Liest Namen und Gesichter aus Minecrafts Spielerliste, nur solange Tab gehalten wird.", "Reads names and faces from Minecraft's player list, only while Tab is held."),
    ("Tab erreicht Minecraft beim Spielen dann nicht. Serverregeln können das verbieten.", "Tab then does not reach Minecraft while playing. Server rules may forbid this."),
    // Zoom
    ("Allgemein", "General"),
    ("Übergang", "Transition"),
    ("Mausrad", "Mouse wheel"),
    ("Zweit-Zoom", "Secondary zoom"),
    ("Taste", "Key"),
    ("Umschalten statt halten", "Toggle instead of hold"),
    ("Anfangs-Zoom", "Initial zoom"),
    ("Maus langsamer mit dem Zoom (%)", "Slow mouse with zoom (%)"),
    ("Film-Kamera (%, 0 ist aus)", "Cinematic camera (%, 0 is off)"),
    ("Hand, Gehbewegung und Fernrohr lassen sich in Bedrock nicht getrennt steuern.", "Hand, view bobbing and spyglass cannot be controlled separately in Bedrock."),
    ("Hinein (s)", "Zoom in (s)"),
    ("Hinaus (s)", "Zoom out (s)"),
    ("Verlauf hinein", "Zoom in transition"),
    ("Verlauf hinaus", "Zoom out transition"),
    ("Mit dem Mausrad weiter zoomen", "Zoom further with the mouse wheel"),
    ("Stufen", "Steps"),
    ("Zoom je Stufe (%)", "Zoom per step (%)"),
    ("Weichheit (%)", "Smoothness (%)"),
    ("Stufe behalten", "Retain steps"),
    ("Stufen auch mit Tasten", "Steps with keys too"),
    ("Taste näher", "Key closer"),
    ("Taste weiter", "Key further"),
    ("Taste (schaltet um)", "Key (toggles)"),
    ("Mett-HUD dabei ausblenden", "Hide Mett HUD meanwhile"),
    ("Wie Zoomify", "Like Zoomify"),
    ("keine", "none"),
    ("Bild auf", "Page up"),
    ("Bild ab", "Page down"),
    ("Sofort", "Instant"),
    ("Sinus hinein", "Sine in"),
    ("Sinus hinaus", "Sine out"),
    ("Sinus beides", "Sine both"),
    ("Quadrat hinein", "Quad in"),
    ("Quadrat hinaus", "Quad out"),
    ("Quadrat beides", "Quad both"),
    ("Kubisch hinein", "Cubic in"),
    ("Kubisch hinaus", "Cubic out"),
    ("Kubisch beides", "Cubic both"),
    ("Exponentiell hinein", "Exponential in"),
    ("Exponentiell hinaus", "Exponential out"),
    ("Exponentiell beides", "Exponential both"),
    // Crosshair, Profile, Optionen
    ("Minecrafts Crosshair", "Minecraft's crosshair"),
    ("Eigenes Crosshair statt dem von Minecraft.", "Your own crosshair instead of Minecraft's."),
    ("Umkehren wie Vanilla", "Invert like vanilla"),
    ("Breite", "Width"),
    ("Höhe", "Height"),
    ("Drehung", "Rotation"),
    ("Form", "Shape"),
    ("Farben", "Colors"),
    ("Kreuz", "Cross"),
    ("T-Form", "T shape"),
    ("Kreis", "Circle"),
    ("Quadrat", "Square"),
    ("Pfeil", "Arrow"),
    ("Punkt", "Dot"),
    ("Regenbogen", "Rainbow"),
    ("Größe des Punkts", "Dot size"),
    ("Farbe des Punkts", "Dot color"),
    ("Farbe des Umrisses", "Outline color"),
    ("Lücke", "Gap"),
    ("Dicke", "Thickness"),
    ("Punkt in der Mitte", "Center dot"),
    ("Neu", "New"),
    ("Kopieren", "Copy"),
    ("Umbenennen", "Rename"),
    ("Löschen", "Delete"),
    ("Sicher?", "Sure?"),
    ("Exportieren", "Export"),
    ("Importieren", "Import"),
    ("Ordner öffnen", "Open folder"),
    ("Das letzte Profil bleibt.", "The last profile stays."),
    ("Profil liegt im Ordner.", "Profile is in the folder."),
    ("Fehler", "Error"),
    ("Übernommen", "Imported"),
    ("nicht lesbar", "not readable"),
    ("Noch keine Profile im Ordner.", "No profiles in the folder yet."),
    ("HUD in Menüs ausblenden", "Hide HUD in menus"),
    ("Beim Ziehen einrasten", "Snap while dragging"),
    ("Rechte Umschalttaste: dieses Menü öffnen und schließen", "Right Shift: open and close this menu"),
    ("Karte: Klick schaltet, Regler oder Rechtsklick öffnet die Einstellungen", "Card: click toggles, the knobs or a right click open its settings"),
    ("HUD: ziehen verschiebt, am Rand ziehen ändert die Größe, Pfeiltasten schieben genau", "HUD: drag to move, drag the edges to resize, arrow keys move precisely"),
    ("Strg+Z: rückgängig   Esc: fertig", "Ctrl+Z: undo   Esc: done"),
    ("Alles zurücksetzen", "Reset everything"),
    ("Sicher? Schaltet alles aus", "Sure? Turns everything off"),
    ("Zurückgesetzt. Sicherung liegt im Profil-Ordner.", "Reset. A backup is in the profile folder."),
    ("Sicherung nicht möglich, nichts geändert.", "Backup failed, nothing changed."),
    // Zustände
    ("aus", "off"),
    ("läuft", "running"),
    ("bereit", "ready"),
    ("ausgeblendet", "hidden"),
    ("wird gesucht", "searching"),
    ("wird in der Welt gesucht", "searching in the world"),
    ("nicht eindeutig gefunden", "not found reliably"),
    ("Spiel nicht lesbar", "Game not readable"),
    ("nur beim Start über den Mett Launcher", "only when started through the Mett Launcher"),
    ("Chat-Modul ist aus", "Chat module is off"),
    ("ab dem nächsten Spielstart", "from the next game start"),
    ("nur Entwicklung, keine Anzeige", "development only, no display"),
    ("eigene Werte werden gesucht", "searching your own values"),
    ("wartet, bis sich Hunger oder Sättigung ändert", "waiting for hunger or saturation to change"),
    ("keine Hungerleiste zu sehen", "no hunger bar visible"),
    ("Minecraft wird ausgewertet", "analysing Minecraft"),
    ("Nicht gesendet: Minecraft war nicht rechtzeitig bereit", "Not sent: Minecraft was not ready in time"),
    ("Nicht gesendet: Befehle gehen mit dieser Spielversion nicht", "Not sent: commands do not work with this game version"),
    ("Nicht gesendet: Minecraft meldet eine Einschränkung", "Not sent: Minecraft reports a restriction"),
    ("Nicht gesendet: Minecraft reagiert gerade nicht", "Not sent: Minecraft is not responding"),
    ("Nicht gesendet: Text nicht erlaubt", "Not sent: text not allowed"),
    ("Unklar, ob die Nachricht ankam", "Unclear whether the message arrived"),
    ("HUD-Controller nicht eindeutig gefunden", "HUD controller not found reliably"),
    ("Aufbau der Oberfläche nicht eindeutig erkannt", "UI structure not recognised reliably"),
    ("Merker „sichtbar“ nicht eindeutig erkannt", "Visibility flag not recognised reliably"),
    ("Plugin nimmt das Fadenkreuz nicht an", "Plugin does not accept the crosshair"),
    ("Minecrafts Chat-Anzeige nicht eindeutig gefunden", "Minecraft's chat display not found reliably"),
    ("Chat-Anzeige hat gewechselt", "Chat display has changed"),
    ("Plugin antwortet nicht", "Plugin does not respond"),
    ("Spielerliste nicht gefunden", "Player list not found"),
    ("eigener Ordner unbekannt", "own folder unknown"),
    ("Plugin-Dateien fehlen", "Plugin files are missing"),
    ("Plugin ließ sich nicht laden", "Plugin could not be loaded"),
    ("Plugin nicht gestartet", "Plugin not started"),
    ("wartet auf Minecraft", "waiting for Minecraft"),
    ("keine Verbindung zum Plugin", "no connection to the plugin"),
    ("Chat-Einstellungen nicht eindeutig gefunden", "Chat settings not found reliably"),
    ("Einstellungen nicht eindeutig gefunden", "Settings not found reliably"),
    ("Sichtfeld nicht eindeutig gefunden", "Field of view not found reliably"),
    ("Einstellungsdatei nicht lesbar", "Settings file not readable"),
    ("Merkzettel ließ sich nicht schreiben", "Restore note could not be written"),
    ("Plugin nimmt die Einstellungen nicht an", "Plugin does not accept the settings"),
    ("Plugin nimmt das Sichtfeld nicht an", "Plugin does not accept the field of view"),
    ("Datei nicht ladbar", "file cannot be loaded"),
    ("kein Mett-Plugin", "not a Mett plugin"),
    ("falsche Plugin-Version", "wrong plugin version"),
    ("passt nicht zu dieser Spielversion", "does not fit this game version"),
    ("Start fehlgeschlagen", "start failed"),
    ("ungültiger Name", "invalid name"),
    ("Senden-Knopf nicht eindeutig gefunden", "Send button not found reliably"),
    ("Funktionstabelle des Knopfs nicht eindeutig", "Button table not unique"),
    ("Funktion des Knopfs nicht erkannt", "Button function not recognised"),
    ("Senden des Chatfensters nicht erkannt", "Chat screen sending not recognised"),
    ("Prüfung auf / nicht eindeutig", "Check for / not unique"),
    ("Sendeaufrufe nicht eindeutig", "Send calls not unique"),
    ("Lage des Modells nicht erkannt", "Model position not recognised"),
    // Fenster und Tray
    ("HUD anzeigen", "Show HUD"),
    ("Mett-Client beenden", "Quit Mett-Client"),
    ("Minecraft Bedrock Edition wurde nicht gefunden.\n\nJa: Microsoft Store öffnen\nNein: warten, bis du Minecraft selbst startest\nAbbrechen: Mett-Client beenden", "Minecraft Bedrock Edition was not found.\n\nYes: open the Microsoft Store\nNo: wait until you start Minecraft yourself\nCancel: quit Mett-Client"),
    ("Minecraft ließ sich nicht automatisch starten. Bitte starte es selbst, Mett-Client wartet.", "Minecraft could not be started automatically. Please start it yourself, Mett-Client is waiting."),
    ("Einstellungen nicht lesbar, es gelten Standardwerte.", "Settings not readable, using defaults."),
    ("Neuer als dieses Programm", "Newer than this program"),
    ("Mett-Client läuft bereits.", "Mett-Client is already running."),
    ("Bitte starte Mett-Client über den Mett Launcher.", "Please start Mett-Client through the Mett Launcher."),
    ("Die Einstellungen konnten nicht gespeichert werden.", "The settings could not be saved."),
    ("Minecraft ist nicht gestartet. Mett-Client wird beendet.", "Minecraft is not running. Mett-Client will quit."),
    ("Mett-Client konnte nicht starten:", "Mett-Client could not start:"),
    // Beispiele im Chat
    ("<Alex> Ist noch jemand wach?", "<Alex> Anyone still awake?"),
    ("<Steve> Gleich da", "<Steve> Almost there"),
    ("§eSteve ist dem Spiel beigetreten", "§eSteve joined the game"),
    ("<Steve> Hallo zusammen!", "<Steve> Hello everyone!"),
    ("<Alex> Hi! Kommst du mit in die Mine?", "<Alex> Hi! Coming to the mine?"),
    ("§aDu hast einen Diamanten gefunden", "§aYou found a diamond"),
    ("<Steve> Ja, ich hole nur noch schnell Fackeln und etwas zu essen, dann können wir los", "<Steve> Yes, let me just grab some torches and something to eat, then we can go"),
    ("§7[Server] Neustart in 10 Minuten", "§7[Server] Restart in 10 minutes"),
    ("§cDu hast keine Rechte für diesen Befehl", "§cYou don't have permission for this command"),
    ("<Alex> Bis gleich!", "<Alex> See you soon!"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprache_aus_den_einstellungen() {
        assert!(is_german("a:1\r\ngame_language:de_DE\r\n"));
        for other in ["game_language:en_US", "game_language:fr_FR", "game_language:", ""] {
            assert!(!is_german(other), "{other}");
        }
    }

    #[test]
    fn jeder_text_steht_einmal_da() {
        let mut seen = std::collections::HashSet::new();
        for (g, e) in TEXTS {
            assert!(seen.insert(g), "{g}");
            assert!(!e.is_empty(), "{g}");
        }
        assert_eq!(english("Uhr"), Some("Clock"));
        assert_eq!(english("mein Profil"), None);
    }

    /// Texte aus Quelltext, ohne Kommentare und Tests.
    fn literals(source: &str) -> Vec<String> {
        let source = source.split("#[cfg(test)]").next().unwrap_or_default();
        let mut out = vec![];
        for line in source.lines().filter(|l| !l.trim_start().starts_with("//") && !l.contains(".expect(")) {
            let mut parts = line.split('"');
            parts.next();
            while let (Some(text), Some(_)) = (parts.next(), parts.next()) {
                out.push(text.to_string());
            }
        }
        out
    }

    #[test]
    fn sichtbare_texte_sind_uebersetzt() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let files = [
            "mett-client/src/menu.rs",
            "mett-client/src/apple_link.rs",
            "mett-client/src/chat_link.rs",
            "mett-client/src/cross_link.rs",
            "mett-client/src/panel_link.rs",
            "mett-client/src/players_link.rs",
            "mett-client/src/plugins.rs",
            "mett-client/src/style_link.rs",
            "mett-client/src/value_link.rs",
            "mett-client/src/gear_link.rs",
            "hud/src/gear.rs",
            "mett-client/src/zoom_link.rs",
            "mett-plugin/src/lib.rs",
            "compat/src/chat_send.rs",
            "hud/src/java_chat.rs",
            "hud/src/zoom.rs",
        ];
        let mut missing = vec![];
        for f in files {
            for text in literals(&std::fs::read_to_string(dir.join(f)).unwrap()) {
                let words = text.contains(' ') || text.chars().any(|c| "äöüÄÖÜß".contains(c));
                let plain = !text.contains('{') && text.chars().filter(|c| c.is_alphabetic()).count() > 4;
                if words && plain && english(&text).is_none() {
                    missing.push(format!("{f}: {text}"));
                }
            }
        }
        assert!(missing.is_empty(), "{missing:#?}");
    }
}
