//! Minecrafts Fadenkreuz aus- und einblenden. Vor jedem Schreiben Klasse und Namen prüfen. Nur im Hauptthread benutzen.
use crate::panel::{usable, word};
use mett_plugin::cross::{NAME, Order, SCREEN};

const END: u64 = 0x190;

pub struct Cursor {
    pub order: Order,
    /// Unsere Änderung gilt gerade.
    hidden: bool,
}

impl Cursor {
    pub fn new(order: Order) -> Self {
        Self { order, hidden: false }
    }

    /// Kurze Namen stehen im Element selbst, lange daneben.
    fn is(&self, at: u64, name: &str) -> bool {
        if !usable(at, END) || unsafe { word(at) } != self.order.table {
            return false;
        }
        let text = at + self.order.name;
        let (len, room) = unsafe { (word(text + 16), word(text + 24)) };
        if len != name.len() as u64 || room < len {
            return false;
        }
        let start = if room < 16 { text } else { unsafe { word(text) } };
        usable(start & !7, len + 8)
            && unsafe { std::slice::from_raw_parts(start as *const u8, name.len()) == name.as_bytes() }
    }

    fn same(&self) -> bool {
        let o = &self.order;
        self.is(o.at, NAME) && unsafe { word(o.at + o.up) } == o.parent && self.is(o.parent, SCREEN)
    }

    /// Ein Schritt. `false`: passt nicht mehr, nie wieder anfassen.
    pub fn step(&mut self) -> bool {
        if !self.same() {
            return false;
        }
        let o = &self.order;
        let both = o.own | o.all;
        let flags = (o.at + o.flags) as *mut u32;
        let now = unsafe { flags.read() };
        if o.hide {
            // Blendet Minecraft es wieder ein, gleich noch einmal. Was es selbst ausblendet, bleibt seine Sache.
            if now & o.own != 0 {
                unsafe { flags.write(now & !both) };
                self.hidden = true;
            }
        } else if std::mem::take(&mut self.hidden) && now & both == 0 {
            // Nur einblenden, was wir ausgeblendet haben, und nur so weit wie der Bildschirm selbst.
            let up = unsafe { ((o.parent + o.flags) as *const u32).read() };
            unsafe { flags.write(now | o.own | (up & o.all)) };
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            // Hinter dem Namen stehen Reste.
            std::ptr::copy_nonoverlapping(b"cursor\0eld".as_ptr(), (at + 0x30) as *mut u8, 10);
            put(at + 0x40, NAME.len() as u64);
            put(at + 0x48, 15);
            put(at + 0xA0, parent);
            std::ptr::copy_nonoverlapping(screen.as_ptr(), text as *mut u8, screen.len());
            put(parent + 0x30, text);
            put(parent + 0x40, screen.len() as u64);
            put(parent + 0x48, 31);
            put(at + 0x28, 0x7C);
            put(parent + 0x28, 0x7E);
            let order = Order {
                client: 1,
                hide: true,
                at,
                parent,
                table: TABLE,
                name: 0x30,
                up: 0xA0,
                flags: 0x28,
                own: 4,
                all: 16,
            };
            (m, order)
        }
    }

    fn flags(m: u64) -> u32 {
        unsafe { ((m + 0x28) as *const u32).read() }
    }

    fn set(m: u64, v: u32) {
        unsafe { ((m + 0x28) as *mut u32).write(v) }
    }

    #[test]
    fn blendet_aus_und_wieder_ein() {
        let (m, order) = setup(SCREEN);
        let mut c = Cursor::new(order);
        assert!(c.step() && flags(m) == 0x68);
        // Minecraft blendet es neu ein.
        set(m, 0x7C);
        assert!(c.step() && flags(m) == 0x68);
        c.order.hide = false;
        assert!(c.step() && flags(m) == 0x7C);
        // Ein zweites Mal passiert nichts mehr.
        set(m, 0x68);
        assert!(c.step() && flags(m) == 0x68);
    }

    #[test]
    fn was_minecraft_selbst_ausblendet_bleibt_aus() {
        let (m, order) = setup(SCREEN);
        set(m, 0x68);
        let mut c = Cursor::new(order);
        assert!(c.step());
        c.order.hide = false;
        assert!(c.step() && flags(m) == 0x68);
        // Ist der ganze Bildschirm ausgeblendet, bleibt es das auch.
        set(m, 0x7C);
        let mut c = Cursor::new(order);
        assert!(c.step());
        set(m + 0x400, 0x6E);
        c.order.hide = false;
        assert!(c.step() && flags(m) == 0x6C);
    }

    #[test]
    fn fremde_elemente_bleiben_unberuehrt() {
        let (m, order) = setup("hud_crosshair_other!");
        assert!(!Cursor::new(order).step() && flags(m) == 0x7C);
        let (m, order) = setup(SCREEN);
        assert!(!Cursor::new(Order { table: 1, ..order }).step() && flags(m) == 0x7C);
        assert!(!Cursor::new(Order { parent: m + 0x800, ..order }).step() && flags(m) == 0x7C);
        unsafe { (m as *mut u64).write(0) };
        assert!(!Cursor::new(order).step() && flags(m) == 0x7C);
    }
}
