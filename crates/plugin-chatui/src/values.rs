//! Benannte Zahlen-Einstellungen umstellen und zurück. Minecraft begrenzt den Wert mit den Grenzen der Einstellung selbst,
//! deshalb werden sie mit erweitert. Vor jedem Schreiben alles prüfen. Nur im Hauptthread benutzen.
use crate::panel::{usable, word};
use mett_plugin::values::{ALLOWED, Order};

pub struct Value {
    pub order: Order,
    /// Wert des Nutzers und unserer.
    held: Option<(f32, f32)>,
    /// Grenzen des Spiels, solange unsere gelten.
    limits: Option<(f32, f32)>,
}

impl Value {
    pub fn new(order: Order) -> Self {
        Self { order, held: None, limits: None }
    }

    /// Ist das noch genau diese Einstellung?
    fn same(&self) -> bool {
        let o = &self.order;
        let Some(text) = o.info.checked_add(o.name) else { return false };
        if !usable(o.at, 0x40) || !usable(text, 32) {
            return false;
        }
        unsafe {
            let (start, len) = (word(text), word(text + 16));
            let name = ALLOWED[o.which].0.as_bytes();
            // Kurze Namen stehen im Text selbst.
            let start = if word(text + 24) < 16 { text } else { start };
            word(o.at) == o.table
                && word(o.at + 8) == o.info
                && len == name.len() as u64
                && usable(start & !7, len + 8)
                && std::slice::from_raw_parts(start as *const u8, name.len()) == name
        }
    }

    /// Einstellen oder zurückstellen. `false`: passt nicht mehr, nie wieder anfassen.
    pub fn step(&mut self) -> bool {
        if !self.same() {
            return false;
        }
        let at = self.order.at + self.order.value;
        let (low, high, field) = ((at - 8) as *mut f32, (at - 4) as *mut f32, at as *mut f32);
        let (min, max, now) = unsafe { (low.read(), high.read(), field.read()) };
        if ![min, max, now].iter().all(|v| v.is_finite()) || min >= max {
            return false;
        }
        match (self.order.target, self.held) {
            (Some(to), held) if held.is_none_or(|(_, set)| set != to || now != set) => {
                // Neu, oder der Nutzer hat selbst umgestellt: Das ist sein Wert.
                let user = held.filter(|(_, set)| *set == now).map_or(now, |h| h.0);
                let (min, max) = *self.limits.get_or_insert((min, max));
                unsafe {
                    low.write(min.min(to));
                    high.write(max.max(to));
                    field.write(to);
                }
                self.held = Some((user, to));
            }
            (Some(_), _) => {}
            (None, held) => {
                unsafe {
                    // Erst der Wert, dann die Grenzen.
                    if let Some((user, _)) = held.filter(|(_, set)| *set == now) {
                        field.write(user);
                    }
                    if let Some((min, max)) = self.limits.take() {
                        low.write(min);
                        high.write(max);
                    }
                }
                self.held = None;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Memory::*;

    const TABLE: u64 = 0x7000;

    fn setup(name: &str) -> (u64, Order) {
        unsafe {
            let m = VirtualAlloc(None, 0x2000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) as u64;
            let put = |at: u64, v: u64| (at as *mut u64).write(v);
            let info = m + 0x400;
            put(m, TABLE);
            put(m + 8, info);
            for (o, v) in [(0x10, 0.0f32), (0x14, 1.0), (0x18, 0.7)] {
                ((m + o) as *mut f32).write(v);
            }
            std::ptr::copy_nonoverlapping(name.as_ptr(), (info + 0x88) as *mut u8, name.len());
            put(info + 0x88 + 16, name.len() as u64);
            put(info + 0x88 + 24, 15);
            let order =
                Order { client: 1, which: 0, target: Some(10.0), table: TABLE, name: 0x88, value: 0x18, at: m, info };
            (m, order)
        }
    }

    fn values(m: u64) -> [f32; 3] {
        [0x10, 0x14, 0x18].map(|o| unsafe { ((m + o) as *const f32).read() })
    }

    #[test]
    fn stellt_um_und_restlos_zurueck() {
        let (m, order) = setup("gfx_gamma");
        let mut v = Value::new(order);
        assert!(v.step() && values(m) == [0.0, 10.0, 10.0]);
        // Anderer Wunsch: Der Wert des Nutzers bleibt gemerkt.
        v.order.target = Some(5.0);
        assert!(v.step() && values(m)[2] == 5.0);
        // Der Nutzer stellt selbst um.
        unsafe { ((m + 0x18) as *mut f32).write(0.3) };
        assert!(v.step() && values(m)[2] == 5.0);
        v.order.target = None;
        assert!(v.step() && values(m) == [0.0, 1.0, 0.3]);
        assert!(v.step() && values(m) == [0.0, 1.0, 0.3]);
    }

    #[test]
    fn fremde_einstellungen_bleiben_unberuehrt() {
        let (m, order) = setup("gfx_gammb");
        assert!(!Value::new(order).step() && values(m) == [0.0, 1.0, 0.7]);
        let (m, order) = setup("gfx_gamma");
        assert!(!Value::new(Order { table: 1, ..order }).step());
        assert!(!Value::new(Order { info: m + 0x600, ..order }).step());
        assert_eq!(values(m), [0.0, 1.0, 0.7]);
    }
}
