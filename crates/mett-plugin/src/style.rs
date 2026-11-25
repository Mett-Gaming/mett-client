//! Auftrag, den Chat wie in der Java Edition aussehen zu lassen: zwei benannte Einstellungen des laufenden Spiels.

pub const SPACING: &str = "chat_line_spacing";
pub const OPACITY: &str = "gfx_chat_background_opacity";
/// Java: 9 Pixel je Zeile und halb durchsichtiger Hintergrund. Minecraft rechnet 10 plus Abstand plus 0,001; das Letzte
/// muss weg, sonst überlappen sich die Hintergründe und es entstehen Trennlinien.
pub const JAVA: [(&str, f32); 2] = [(SPACING, -1.001), (OPACITY, 0.5)];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Order {
    /// Endet dieser Prozess, stellt das Plugin alles zurück.
    pub client: u32,
    pub on: bool,
    pub table: u64,
    pub name: u64,
    pub value: u64,
    /// (Einstellung, Beschreibung), in der Reihenfolge von `JAVA`.
    pub options: [(u64, u64); 2],
}

impl Order {
    pub fn encode(&self) -> String {
        let [a, b] = self.options;
        let list = [self.client as u64, self.on as u64, self.table, self.name, self.value, a.0, a.1, b.0, b.1];
        list.map(|v| format!("{v:x}")).join(" ")
    }

    pub fn decode(s: &str) -> Option<Self> {
        let v: Vec<u64> = s.split(' ').map(|t| u64::from_str_radix(t, 16).ok()).collect::<Option<_>>()?;
        let [client, on, table, name, value, a, ai, b, bi] = v[..] else { return None };
        let ok = on < 2
            && name.is_multiple_of(8)
            && name < 0x800
            && value.is_multiple_of(4)
            && (0x10..0x30).contains(&value);
        ok.then_some(Self {
            client: u32::try_from(client).ok()?,
            on: on == 1,
            table,
            name,
            value,
            options: [(a, ai), (b, bi)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hin_und_zurueck() {
        let o = Order { client: 7, on: true, table: u64::MAX, name: 0x188, value: 0x18, options: [(1, 2), (3, 4)] };
        assert_eq!(Order::decode(&o.encode()), Some(o));
        for bad in ["", "1 2 3", &o.encode().replace(" 188 ", " 189 "), &o.encode().replace(" 18 ", " 8 ")] {
            assert_eq!(Order::decode(bad), None, "{bad}");
        }
        assert_eq!(Order::decode(&format!("{} 1", o.encode())), None);
    }
}
