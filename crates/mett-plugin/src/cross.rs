//! Auftrag, Minecrafts Fadenkreuz aus- oder einzublenden. Adressen und Abstände gelten nur für den laufenden Prozess und
//! werden dort vor jedem Schreiben geprüft.

/// Das Fadenkreuz und sein Bildschirm.
pub const NAME: &str = "cursor";
pub const SCREEN: &str = "hud_crosshair_screen";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Order {
    /// Endet dieser Prozess, blendet das Plugin wieder ein.
    pub client: u32,
    pub hide: bool,
    pub at: u64,
    pub parent: u64,
    pub table: u64,
    /// Wo im Element Name, Elternteil und Merker stehen.
    pub name: u64,
    pub up: u64,
    pub flags: u64,
    /// Merker „selbst sichtbar“ und „samt Eltern sichtbar“.
    pub own: u32,
    pub all: u32,
}

impl Order {
    pub fn encode(&self) -> String {
        let v = [
            self.client as u64,
            self.hide as u64,
            self.at,
            self.parent,
            self.table,
            self.name,
            self.up,
            self.flags,
            self.own as u64,
            self.all as u64,
        ];
        v.map(|v| format!("{v:x}")).join(" ")
    }

    pub fn decode(s: &str) -> Option<Self> {
        let v: Vec<u64> = s.split(' ').map(|t| u64::from_str_radix(t, 16).ok()).collect::<Option<_>>()?;
        let [client, hide, at, parent, table, name, up, flags, own, all] = v[..] else { return None };
        let bit = |b: u64| u32::try_from(b).ok().filter(|b| b.is_power_of_two());
        let (own, all) = (bit(own)?, bit(all)?);
        let pointers = [name, up].iter().all(|a| a.is_multiple_of(8) && (8..0x180).contains(a));
        let ok = pointers
            && flags.is_multiple_of(4)
            && (8..0x180).contains(&flags)
            && own != all
            && hide <= 1
            && [at, parent].iter().all(|a| a.is_multiple_of(8) && *a >= 0x10000);
        let client = u32::try_from(client).ok()?;
        ok.then_some(Self { client, hide: hide == 1, at, parent, table, name, up, flags, own, all })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hin_und_zurueck_und_pruefung() {
        let o = Order {
            client: 7,
            hide: true,
            at: 0x20000,
            parent: 0x30000,
            table: 0x7000,
            name: 0x30,
            up: 0xA0,
            flags: 0x28,
            own: 4,
            all: 16,
        };
        assert_eq!(Order::decode(&o.encode()), Some(o));
        for bad in [
            Order { own: 3, ..o },
            Order { all: 4, ..o },
            Order { at: 3, ..o },
            Order { flags: 0x200, ..o },
            Order { name: 0x31, ..o },
        ] {
            assert_eq!(Order::decode(&bad.encode()), None);
        }
        assert_eq!(Order::decode("1 2"), None);
        assert_eq!(Order::decode(""), None);
    }
}
