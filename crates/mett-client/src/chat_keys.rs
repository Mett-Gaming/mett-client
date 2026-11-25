//! Chat- und Befehlstaste aus Minecrafts Einstellungen.
use crate::focus::options_file;

/// Wie in Minecraft voreingestellt.
const T: u16 = 0x54;

/// (Chat, Befehl). Ohne Angabe bleibt die Befehlstaste bei Minecraft.
pub fn read() -> (Vec<u16>, Vec<u16>) {
    let options = options_file().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    (parse_keys(&options, "chat").unwrap_or_else(|| vec![T]), parse_keys(&options, "command").unwrap_or_default())
}

fn parse_keys(options: &str, action: &str) -> Option<Vec<u16>> {
    let name = format!("keyboard_type_0_key.{action}:");
    let line = options.lines().find_map(|l| l.trim().strip_prefix(name.as_str()))?;
    // Nur echte Tasten, keine Maustasten (Minecraft zählt sie negativ).
    let keys: Vec<u16> =
        line.split(',').filter_map(|k| k.trim().parse().ok()).filter(|k| (8..255).contains(k)).collect();
    (!keys.is_empty()).then_some(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasten_aus_den_einstellungen() {
        let t = "ctrl_type_0_key.chat:8\nkeyboard_type_0_key.chat:84,13\r\nkeyboard_type_0_key.command:191";
        assert_eq!(parse_keys(t, "chat"), Some(vec![84, 13]));
        assert_eq!(parse_keys(t, "command"), Some(vec![191]));
        assert_eq!(parse_keys("keyboard_type_0_key.chat:-99", "chat"), None);
        assert_eq!(parse_keys("", "chat"), None);
    }
}
