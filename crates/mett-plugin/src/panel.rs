//! Auftrag an das Plugin, das Minecrafts Chat-Anzeige verschiebt. Adressen und Abstände gelten nur für den laufenden
//! Prozess und werden dort vor jedem Schreiben geprüft.

pub const PLUGIN: &str = "chatui";
pub const NAME: &str = "chat_panel";
pub const PARENT: &str = "chat_stack";
/// Liste der Nachrichten und ihre Einträge.
pub const LIST: &str = "stack_panel";
pub const ROW: &str = "chat_grid_item";
pub const BACKGROUND: &str = "chat_background";
pub const TEXT: &str = "chat_text";
pub const MAX_ROWS: u32 = 200;

pub mod answer {
    pub const OK: u32 = 0;
    pub const BAD_ORDER: u32 = 2;
    /// Das Element passt nicht mehr, nichts geschrieben.
    pub const STALE: u32 = 3;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wish {
    /// Alles zurück an den Platz des Spiels.
    Release,
    /// Linker und unterer Rand in Einheiten der Oberfläche, und wie viele der neuesten Nachrichten zu sehen sind.
    Place {
        x: f32,
        bottom: f32,
        rows: u32,
        /// Nicht sanft ausblenden: sobald es beginnt, ist die Nachricht weg. Braucht `Offsets::alpha`.
        sudden: bool,
        /// Größe in Einheiten der Oberfläche, 0: die des Spiels. Braucht `Offsets::layout`.
        size: (f32, f32),
        /// Text wie in Java eingerückt.
        java: bool,
    },
    Hide,
}

/// Wo in einem Element was steht.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Offsets {
    pub name: u64,
    pub place: u64,
    pub size: u64,
    pub flags: u64,
    pub bit: u64,
    pub parent: u64,
    pub children: u64,
    pub half: u64,
    /// 0: unbekannt.
    pub alpha: u64,
    /// Liste der Bausteine, 0: unbekannt.
    pub parts: u64,
    pub owner: u64,
    pub delta: u64,
    /// Tabelle des Bausteins mit den Maßen, 0: unbekannt.
    pub layout: u64,
    /// Wo im Baustein der Zeiger auf die Breite steht, und wo dort der Wert.
    pub width: u64,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Order {
    /// Endet dieser Prozess, stellt das Plugin alles zurück.
    pub client: u32,
    pub at: u64,
    pub parent: u64,
    pub table: u64,
    pub offsets: Offsets,
    pub wish: Wish,
}

const LIMIT: f32 = 10_000.0;

impl Order {
    pub fn encode(&self) -> String {
        let o = &self.offsets;
        let list = [
            o.name, o.place, o.size, o.flags, o.bit, o.parent, o.children, o.half, o.alpha, o.parts, o.owner, o.delta,
            o.layout, o.width, o.value,
        ];
        let head = [self.client as u64, self.at, self.parent, self.table].into_iter().chain(list);
        let head = head.map(|v| format!("{v:x}")).collect::<Vec<_>>().join(" ");
        match self.wish {
            Wish::Release => format!("{head} r"),
            Wish::Hide => format!("{head} h"),
            Wish::Place { x, bottom, rows, sudden, size: (w, h), java } => {
                format!("{head} p {x} {bottom} {rows} {} {w} {h} {}", sudden as u8, java as u8)
            }
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        let p: Vec<&str> = s.split(' ').collect();
        let v: Vec<u64> = p.get(..19)?.iter().map(|t| u64::from_str_radix(t, 16).ok()).collect::<Option<_>>()?;
        let number = |t: &str| t.parse::<f32>().ok().filter(|n| n.is_finite() && n.abs() < LIMIT);
        let wish = match p[19..] {
            ["r"] => Wish::Release,
            ["h"] => Wish::Hide,
            ["p", x, bottom, rows, sudden @ ("0" | "1"), w, h, java @ ("0" | "1")] => {
                let size = (number(w)?, number(h)?);
                if size.0 < 0.0 || size.1 < 0.0 {
                    return None;
                }
                let rows = rows.parse().ok().filter(|r| (1..=MAX_ROWS).contains(r))?;
                Wish::Place {
                    x: number(x)?,
                    bottom: number(bottom)?,
                    rows,
                    sudden: sudden == "1",
                    size,
                    java: java == "1",
                }
            }
            _ => return None,
        };
        let offsets = Offsets {
            name: v[4],
            place: v[5],
            size: v[6],
            flags: v[7],
            bit: v[8],
            parent: v[9],
            children: v[10],
            half: v[11],
            alpha: v[12],
            parts: v[13],
            owner: v[14],
            delta: v[15],
            layout: v[16],
            width: v[17],
            value: v[18],
        };
        let o = &offsets;
        let pointers = [o.name, o.parent, o.children].iter().all(|a| a.is_multiple_of(8) && (8..0x180).contains(a));
        let numbers = [o.place, o.size, o.flags]
            .iter()
            .chain([&o.alpha].into_iter().filter(|a| **a != 0))
            .all(|a| a.is_multiple_of(4) && (8..0x180).contains(a));
        let bit = o.bit.is_power_of_two() && o.bit <= u32::MAX as u64;
        let parts = o.parts.is_multiple_of(8)
            && o.parts < 0x180
            && (o.parts == 0 || matches!(o.owner, 8 | 16 | 24))
            && o.delta < 0x40;
        let width = o.width.is_multiple_of(8) && o.width < 0xB0;
        let value = o.value.is_multiple_of(4) && o.value < 0x50;
        let layout = o.layout == 0 || (width && value);
        let ok = layout && parts && pointers && numbers && bit && matches!(o.half, 0 | 8);
        ok.then_some(Self { client: u32::try_from(v[0]).ok()?, at: v[1], parent: v[2], table: v[3], offsets, wish })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(wish: Wish) -> Order {
        let offsets = Offsets {
            name: 0x30,
            place: 0x50,
            size: 0x58,
            flags: 0x28,
            bit: 1,
            parent: 0xA0,
            children: 0xA8,
            half: 8,
            alpha: 0x70,
            parts: 0xC8,
            owner: 8,
            delta: 0x10,
            layout: 9,
            width: 0x30,
            value: 0x20,
        };
        Order { client: 7, at: u64::MAX, parent: 2, table: 3, offsets, wish }
    }

    #[test]
    fn hin_und_zurueck() {
        for w in [
            Wish::Release,
            Wish::Hide,
            Wish::Place { x: 1.5, bottom: -20.25, rows: 7, sudden: true, size: (500.5, 90.0), java: true },
        ] {
            assert_eq!(Order::decode(&order(w).encode()), Some(order(w)));
        }
    }

    #[test]
    fn unsinn_wird_abgelehnt() {
        let good = order(Wish::Hide).encode();
        for bad in ["", "1 2", &good.replace(" h", ""), &good.replace(" h", " x"), &good.replace(" h", " p 1")] {
            assert_eq!(Order::decode(bad), None, "{bad}");
        }
        for end in [
            " p nan 1 5 0 0 0 0",
            " p 1 99999 5 0 0 0 0",
            " p inf 1 5 0 0 0 0",
            " p 1 1 5 0 0 0",
            " p 1 1 0 0 0 0 0",
            " p 1 1 201 0 0 0 0",
            " p 1 1 5 2 0 0 0",
            " p 1 1 5 0 -1 0 0",
            " p 1 1 5 0 nan 0 0",
            " p 1 1 5 0 1 1 2",
            " p 1 1 5 0 1 1",
            " h 1",
        ] {
            assert_eq!(Order::decode(&good.replace(" h", end)), None, "{end}");
        }
        let spoil: [fn(&mut Offsets); 6] = [
            |o| o.name = 0x31,
            |o| o.parent = 0x1000,
            |o| o.place = 2,
            |o| o.bit = 3,
            |o| o.half = 4,
            |o| o.value = 0x1000,
        ];
        for s in spoil {
            let mut o = order(Wish::Hide);
            s(&mut o.offsets);
            assert_eq!(Order::decode(&o.encode()), None);
        }
    }
}
