//! Minecrafts Fadenkreuz aus dem Bild rücken und zurückstellen. Vor jedem Schreiben Klasse und Namen prüfen. Nur im
//! Hauptthread benutzen.
use crate::panel::{usable, word};
use mett_plugin::panel::{CROSSHAIR, CROSSHAIR_SCREEN, Order, Wish};

/// Weit außerhalb des Bildschirms.
const AWAY: f32 = -5000.0;
const END: u64 = 0x190;

pub struct Cursor {
    pub order: Order,
    /// Lage des Spiels.
    original: Option<f32>,
}

impl Cursor {
    pub fn new(order: Order) -> Self {
        Self { order, original: None }
    }

    /// Kurze Namen stehen im Element selbst, lange daneben.
    fn is(&self, at: u64, name: &str) -> bool {
        if !usable(at, END) || unsafe { word(at) } != self.order.table {
            return false;
        }
        let text = at + self.order.offsets.name;
        let (len, room) = unsafe { (word(text + 16), word(text + 24)) };
        if len != name.len() as u64 || room < len {
            return false;
        }
        let start = if room < 16 { text } else { unsafe { word(text) } };
        // Auch ein ungerader Anfang wäre lesbar; der Speicher davor wird mitgeprüft.
        usable(start & !7, len + 8)
            && unsafe { std::slice::from_raw_parts(start as *const u8, name.len()) == name.as_bytes() }
    }

    fn same(&self) -> bool {
        let (o, at) = (&self.order.offsets, self.order.at);
        self.is(at, CROSSHAIR)
            && unsafe { word(at + o.parent) } == self.order.parent
            && self.is(self.order.parent, CROSSHAIR_SCREEN)
    }

    /// Ein Schritt. `false`: passt nicht mehr, nie wieder anfassen.
    pub fn step(&mut self) -> bool {
        if !self.same() {
            return false;
        }
        let o = &self.order.offsets;
        let place = (self.order.at + o.place) as *mut f32;
        let now = unsafe { place.read() };
        if !now.is_finite() || now.abs() > 20_000.0 {
            return false;
        }
        if now != AWAY {
            // Hat das Spiel neu gerechnet, ist das die neue Ausgangslage.
            self.original = Some(now);
        }
        let Some(original) = self.original else {
            // Schon weggerückt, aber die alte Lage ist unbekannt: Das Spiel soll selbst rechnen.
            return false;
        };
        let to = if self.order.wish == Wish::Hide { AWAY } else { original };
        if to != now {
            unsafe {
                place.write(to);
                let flags = (self.order.at + o.flags) as *mut u32;
                flags.write(flags.read() | o.bit as u32);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mett_plugin::panel::Offsets;
    use windows::Win32::System::Memory::*;

    const TABLE: u64 = 0x7000;

    /// Element und Bildschirm im eigenen Arbeitsspeicher.
    fn setup(screen: &str) -> (u64, Order) {
        unsafe {
            let m = VirtualAlloc(None, 0x2000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) as u64;
            let put = |at: u64, v: u64| (at as *mut u64).write(v);
            let (at, parent, text) = (m, m + 0x400, m + 0x1000);
            put(at, TABLE);
            put(parent, TABLE);
            std::ptr::copy_nonoverlapping(CROSSHAIR.as_ptr(), (at + 0x30) as *mut u8, CROSSHAIR.len());
            put(at + 0x40, CROSSHAIR.len() as u64);
            put(at + 0x48, 15);
            put(at + 0xA0, parent);
            std::ptr::copy_nonoverlapping(screen.as_ptr(), text as *mut u8, screen.len());
            put(parent + 0x30, text);
            put(parent + 0x40, screen.len() as u64);
            put(parent + 0x48, 31);
            ((at + 0x60) as *mut f32).write(419.0);
            let offsets = Offsets {
                name: 0x30,
                place: 0x60,
                size: 0x70,
                flags: 0x28,
                bit: 1,
                parent: 0xA0,
                children: 0xA8,
                half: 0,
                ..Default::default()
            };
            (m, Order { client: 1, at, parent, table: TABLE, offsets, wish: Wish::Hide })
        }
    }

    fn x(m: u64) -> f32 {
        unsafe { ((m + 0x60) as *const f32).read() }
    }

    #[test]
    fn rueckt_weg_und_kommt_zurueck() {
        let (m, order) = setup(CROSSHAIR_SCREEN);
        let mut c = Cursor::new(order);
        assert!(c.step() && x(m) == AWAY);
        assert_eq!(unsafe { ((m + 0x28) as *const u32).read() }, 1);
        // Das Spiel rechnet neu.
        unsafe { ((m + 0x60) as *mut f32).write(300.0) };
        assert!(c.step() && x(m) == AWAY);
        c.order.wish = Wish::Release;
        assert!(c.step() && x(m) == 300.0);
    }

    #[test]
    fn fremde_elemente_bleiben_unberuehrt() {
        let (m, order) = setup("hud_crosshair_other!");
        assert!(!Cursor::new(order).step() && x(m) == 419.0);
        let (m, order) = setup(CROSSHAIR_SCREEN);
        assert!(!Cursor::new(Order { table: 1, ..order }).step() && x(m) == 419.0);
        assert!(!Cursor::new(Order { parent: m + 0x800, ..order }).step() && x(m) == 419.0);
        unsafe { (m as *mut u64).write(0) };
        assert!(!Cursor::new(order).step() && x(m) == 419.0);
    }
}
