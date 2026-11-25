//! Java-Aussehen: stellt zwei benannte Einstellungen des laufenden Spiels um und wieder zurück. Vor jedem Schreiben
//! werden Tabelle, Beschreibung und Name geprüft. Nur im Hauptthread benutzen.
use crate::panel::{usable, word};
use mett_plugin::style::{JAVA, Order};

pub struct Style {
    pub order: Order,
    /// Wert des Nutzers und unserer, solange unserer gilt.
    held: [Option<(f32, f32)>; 2],
}

impl Style {
    pub fn new(order: Order) -> Self {
        Self { order, held: [None; 2] }
    }

    /// Ist das noch genau diese Einstellung?
    fn is(&self, k: usize) -> bool {
        let (o, (at, info)) = (&self.order, self.order.options[k]);
        let Some(text) = info.checked_add(o.name) else { return false };
        if !usable(at, 0x40) || !usable(text, 32) {
            return false;
        }
        unsafe {
            let (start, len) = (word(text), word(text + 16));
            let name = JAVA[k].0.as_bytes();
            word(at) == o.table
                && word(at + 8) == info
                && len == name.len() as u64
                && usable(start & !7, len + 8)
                && std::slice::from_raw_parts(start as *const u8, name.len()) == name
        }
    }

    /// Einstellen oder zurückstellen. `false`: passt nicht mehr, nie wieder anfassen.
    pub fn step(&mut self) -> bool {
        if !(self.is(0) && self.is(1)) {
            return false;
        }
        for (k, (_, ours)) in JAVA.into_iter().enumerate() {
            let at = (self.order.options[k].0 + self.order.value) as *mut f32;
            let now = unsafe { at.read() };
            if !now.is_finite() {
                return false;
            }
            match (self.order.on, self.held[k]) {
                (true, Some((_, set))) if now == set => {}
                // Neu, oder der Nutzer hat selbst umgestellt: Das ist sein Wert.
                (true, _) => {
                    self.held[k] = Some((now, ours));
                    unsafe { at.write(ours) };
                }
                (false, Some((user, set))) => {
                    if now == set {
                        unsafe { at.write(user) };
                    }
                    self.held[k] = None;
                }
                (false, None) => {}
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Memory::*;

    const TABLE: u64 = 0x7FF6_0000_2000;

    #[test]
    fn stellt_um_und_wieder_zurueck() {
        unsafe {
            let mem = VirtualAlloc(None, 0x10000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) as u64;
            let set = |o: u64, v: u64| ((mem + o) as *mut u64).write_volatile(v);
            let put = |o: u64, v: f32| ((mem + o) as *mut f32).write_volatile(v);
            let get = || [0x118u64, 0x218].map(|o| ((mem + o) as *const f32).read_volatile());
            for (k, at, info, text) in [(0usize, 0x100u64, 0x1000u64, 0x4000u64), (1, 0x200, 0x2000, 0x4100)] {
                set(at, TABLE);
                set(at + 8, mem + info);
                let name = JAVA[k].0;
                std::ptr::copy_nonoverlapping(name.as_ptr(), (mem + text) as *mut u8, name.len());
                set(info + 0x188, mem + text);
                set(info + 0x198, name.len() as u64);
            }
            put(0x118, 1.0);
            put(0x218, 0.7);
            let options = [(mem + 0x100, mem + 0x1000), (mem + 0x200, mem + 0x2000)];
            let order = Order { client: 1, on: true, table: TABLE, name: 0x188, value: 0x18, options };
            let mut s = Style::new(order);
            assert!(s.step() && s.step());
            assert_eq!(get(), [-1.001, 0.5]);
            // Der Nutzer stellt selbst etwas um: Das wird am Ende wiederhergestellt.
            put(0x218, 0.3);
            assert!(s.step());
            assert_eq!(get(), [-1.001, 0.5]);
            s.order.on = false;
            assert!(s.step());
            assert_eq!(get(), [1.0, 0.3]);
            assert!(s.step());
            assert_eq!(get(), [1.0, 0.3]);
            // Andere Tabelle, Beschreibung oder Name: nichts anfassen.
            for (o, v) in [(0x100u64, 5u64), (0x208, mem + 0x1000), (0x1198, 3), (0x4100, 0x4141_4141)] {
                let keep = word(mem + o);
                set(o, v);
                assert!(!Style::new(order).step(), "{o:x}");
                assert_eq!(get(), [1.0, 0.3]);
                set(o, keep);
            }
        }
    }
}
