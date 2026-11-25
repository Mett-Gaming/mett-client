//! Auftrag, eine von wenigen benannten Zahlen-Einstellungen des laufenden Spiels umzustellen.

/// Name und erlaubte Werte. Nur diese.
pub const ALLOWED: [(&str, f32, f32); 1] = [(GAMMA, 0.0, 15.0)];
pub const GAMMA: &str = "gfx_gamma";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Order {
    /// Endet dieser Prozess, stellt das Plugin alles zurück.
    pub client: u32,
    /// Platz in `ALLOWED`.
    pub which: usize,
    /// `None` stellt zurück.
    pub target: Option<f32>,
    pub table: u64,
    pub name: u64,
    pub value: u64,
    pub at: u64,
    pub info: u64,
}

impl Order {
    pub fn encode(&self) -> String {
        let t = self.target.map_or("-".into(), |t| t.to_string());
        let v = [self.client as u64, self.which as u64, self.table, self.name, self.value, self.at, self.info];
        format!("{t} {}", v.map(|v| format!("{v:x}")).join(" "))
    }

    pub fn decode(s: &str) -> Option<Self> {
        let (target, rest) = s.split_once(' ')?;
        let v: Vec<u64> = rest.split(' ').map(|t| u64::from_str_radix(t, 16).ok()).collect::<Option<_>>()?;
        let [client, which, table, name, value, at, info] = v[..] else { return None };
        let which = usize::try_from(which).ok()?;
        let (_, low, high) = *ALLOWED.get(which)?;
        let target = match target {
            "-" => None,
            t => Some(t.parse::<f32>().ok().filter(|t| (low..=high).contains(t))?),
        };
        // Vor dem Wert liegen die zwei Grenzen.
        let ok = name.is_multiple_of(8)
            && name < 0x800
            && value.is_multiple_of(4)
            && (0x18..0x30).contains(&value)
            && at.is_multiple_of(8);
        ok.then_some(Self { client: u32::try_from(client).ok()?, which, target, table, name, value, at, info })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hin_und_zurueck_und_nur_erlaubtes() {
        let o =
            Order { client: 7, which: 0, target: Some(10.0), table: 1, name: 0x188, value: 0x18, at: 0x20000, info: 9 };
        assert_eq!(Order::decode(&o.encode()), Some(o));
        let off = Order { target: None, ..o };
        assert_eq!(Order::decode(&off.encode()), Some(off));
        for bad in [
            Order { target: Some(16.0), ..o },
            Order { target: Some(f32::NAN), ..o },
            Order { which: 1, ..o },
            Order { value: 0x10, ..o },
            Order { at: 3, ..o },
        ] {
            assert_eq!(Order::decode(&bad.encode()), None, "{bad:?}");
        }
        assert_eq!(Order::decode(""), None);
    }
}
