//! Breite eines Elements: setzt den gespeicherten Wert, nach dem sich neue Zeilen richten, und stellt den alten zurück.
//! Es sind nur Zahlen in den Daten dieses einen Elements. Nur im Hauptthread benutzen.
use crate::panel::{usable, word};
use mett_plugin::panel::Offsets;

pub struct Width {
    control: u64,
    /// Baustein und sein Maß.
    part: u64,
    var: u64,
    o: Offsets,
    original: f32,
    ours: Option<f32>,
}

unsafe fn float(at: u64) -> f32 {
    unsafe { *(at as *const f32) }
}

impl Width {
    pub fn attach(control: u64, o: &Offsets) -> Option<Self> {
        if o.layout == 0 || o.parts == 0 {
            return None;
        }
        let (begin, end) = unsafe { (word(control + o.parts), word(control + o.parts + 8)) };
        let size = end.checked_sub(begin).filter(|s| *s > 0 && *s <= 64 && s % 8 == 0)?;
        if !usable(begin, size) {
            return None;
        }
        let fits =
            |c: &u64| usable(*c, 0xB0) && unsafe { word(*c) == o.layout && word(*c + o.owner) == control + o.delta };
        let all: Vec<u64> = (0..size / 8).map(|i| unsafe { word(begin + i * 8) }).filter(fits).collect();
        let [part] = all[..] else { return None };
        let var = unsafe { word(part + o.width) };
        let new = Self { control, part, var, o: *o, original: 0.0, ours: None };
        (new.alive() && new.agrees()).then_some(new)
    }

    /// Gehören Baustein und Maß noch zu diesem Element?
    pub fn alive(&self) -> bool {
        let (o, mine) = (&self.o, self.control + self.o.delta);
        usable(self.part, 0xB0)
            && usable(self.var, 0x50)
            && unsafe {
                word(self.part) == o.layout
                    && word(self.part + o.owner) == mine
                    && word(self.part + o.width) == self.var
                    && word(self.var) == mine
            }
    }

    /// Maß und Element nennen dieselbe Breite.
    fn agrees(&self) -> bool {
        let (a, b) = unsafe { (float(self.var + self.o.value), float(self.control + self.o.size)) };
        a.is_finite() && a > 0.0 && (a - b).abs() < 0.1
    }

    fn write(&self, width: f32) {
        unsafe {
            ((self.var + self.o.value) as *mut f32).write(width);
            ((self.control + self.o.size) as *mut f32).write(width);
        }
    }

    pub fn set(&mut self, width: f32) {
        let now = unsafe { float(self.var + self.o.value) };
        // Vom Spiel neu gerechnet: Das ist die neue Ausgangslage.
        if Some(now) != self.ours {
            if !self.agrees() {
                return;
            }
            self.original = now;
        }
        if now != width {
            self.write(width);
        }
        self.ours = Some(width);
    }
}

impl Drop for Width {
    /// Nur zurück, wo noch unsere Zahl steht.
    fn drop(&mut self) {
        if usable(self.control, 0x180) && self.alive() && self.ours == Some(unsafe { float(self.var + self.o.value) }) {
            self.write(self.original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Memory::*;

    const TABLE: u64 = 0x7FF6_0000_1000;

    #[test]
    fn setzt_die_breite_und_stellt_zurueck() {
        unsafe {
            let mem = VirtualAlloc(None, 0x10000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) as u64;
            let set = |o: u64, v: u64| ((mem + o) as *mut u64).write_volatile(v);
            let put = |o: u64, v: f32| ((mem + o) as *mut f32).write_volatile(v);
            let get = || (((mem + 0x58) as *const f32).read_volatile(), ((mem + 0x4020) as *const f32).read_volatile());
            let control = mem;
            put(0x58, 341.6);
            set(0xC8, mem + 0x1000);
            set(0xD0, mem + 0x1010);
            // Zwei Bausteine, nur der zweite hält die Maße.
            set(0x1000, mem + 0x2000);
            set(0x1008, mem + 0x3000);
            set(0x2000, 0x1234);
            set(0x2008, control + 0x10);
            set(0x3000, TABLE);
            set(0x3008, control + 0x10);
            set(0x3030, mem + 0x4000);
            set(0x4000, control + 0x10);
            put(0x4020, 341.6);
            let o = Offsets {
                size: 0x58,
                parts: 0xC8,
                owner: 8,
                delta: 0x10,
                layout: TABLE,
                width: 0x30,
                value: 0x20,
                ..Default::default()
            };
            for bad in [
                Offsets { layout: 0, ..o },
                Offsets { layout: TABLE + 8, ..o },
                Offsets { delta: 0x18, ..o },
                Offsets { width: 0x38, ..o },
                Offsets { value: 0x24, ..o },
            ] {
                assert!(Width::attach(control, &bad).is_none());
            }
            let mut w = Width::attach(control, &o).unwrap();
            for v in [500.0, 250.5] {
                w.set(v);
                assert_eq!(get(), (v, v));
            }
            drop(w);
            assert_eq!(get(), (341.6, 341.6));
            // Das Spiel rechnet neu (anderes Fenster): neue Ausgangslage, unsere Breite gilt weiter.
            let mut w = Width::attach(control, &o).unwrap();
            w.set(500.0);
            put(0x58, 400.0);
            put(0x4020, 400.0);
            w.set(500.0);
            assert_eq!(get(), (500.0, 500.0));
            drop(w);
            assert_eq!(get(), (400.0, 400.0));
            // Maß gehört zu etwas anderem: nichts zurückschreiben.
            let mut w = Width::attach(control, &o).unwrap();
            w.set(500.0);
            set(0x4000, 0x99);
            assert!(!w.alive());
            drop(w);
            assert_eq!(get(), (500.0, 500.0));
        }
    }
}
