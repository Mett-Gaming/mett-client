//! Auftrag an das Chat-Plugin. Adressen gelten nur für den laufenden Prozess und werden dort noch einmal geprüft.

pub const PLUGIN: &str = "chat";
/// UTF-8, reichlich über den 256 Zeichen der Eingabe.
pub const MAX_BYTES: usize = 1024;

pub mod answer {
    pub const OK: u32 = 0;
    /// Minecraft meldet: eingeschränkt.
    pub const RESTRICTED: u32 = 1;
    pub const BAD_ORDER: u32 = 2;
    /// Die Adressen passen nicht mehr, nichts gesendet.
    pub const STALE: u32 = 3;
    /// Das Spiel hat nicht rechtzeitig reagiert, nichts gesendet.
    pub const NOT_REACHED: u32 = 4;
    /// Läuft noch; ob es ankam, ist offen.
    pub const UNKNOWN: u32 = 5;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Nur prüfen, nichts senden.
    Check,
    Chat(String),
    /// Mit dem Schrägstrich.
    Command(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Order {
    pub function: u64,
    pub controller: u64,
    pub table: u64,
    pub offset: u64,
    pub model: u64,
    pub action: Action,
}

impl Order {
    /// Fünf Adressen in der ersten Zeile, danach der Text. Ob es ein Befehl ist, sagt sein erstes Zeichen.
    pub fn encode(&self) -> String {
        let a = [self.function, self.controller, self.table, self.offset, self.model].map(|v| format!("{v:x}"));
        let head = a.join(" ");
        match &self.action {
            Action::Check => head,
            Action::Chat(t) | Action::Command(t) => format!("{head}\n{t}"),
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        let (head, text) = s.split_once('\n').map_or((s, None), |(h, t)| (h, Some(t)));
        let v: Vec<u64> = head.split(' ').map(|p| u64::from_str_radix(p, 16).ok()).collect::<Option<_>>()?;
        let [function, controller, table, offset, model] = v[..] else { return None };
        let action = match text {
            None => Action::Check,
            Some(t) if t.is_empty() || t == "/" || t.len() > MAX_BYTES || t.chars().any(char::is_control) => {
                return None;
            }
            Some(t) if t.starts_with('/') => Action::Command(t.into()),
            Some(t) => Action::Chat(t.into()),
        };
        Some(Self { function, controller, table, offset, model, action })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hin_und_zurueck() {
        let mut o = Order { function: 1, controller: u64::MAX, table: 3, offset: 4, model: 5, action: Action::Check };
        assert_eq!(Order::decode(&o.encode()), Some(o.clone()));
        for a in [Action::Chat("Grüße aus 1 2 3 😀".into()), Action::Command("/help".into())] {
            o.action = a;
            assert_eq!(Order::decode(&o.encode()), Some(o.clone()));
        }
    }

    #[test]
    fn unsinn_wird_abgelehnt() {
        for bad in ["", "1 2 3 4", "1 2 3 4 5 6", "1 2 x 4 5", "1 2 3 4 5\n", "1 2 3 4 5\n/", "1 2 3 4 5\na\nb"] {
            assert_eq!(Order::decode(bad), None, "{bad}");
        }
        assert_eq!(Order::decode(&format!("1 2 3 4 5\n{}", "a".repeat(MAX_BYTES + 1))), None);
    }
}
