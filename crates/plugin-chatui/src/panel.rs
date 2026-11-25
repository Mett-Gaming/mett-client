//! Das Element selbst: vor jedem Schreiben Klasse, Namen und Eltern prüfen. Nur im Hauptthread benutzen.
use crate::follow::{Follow, Where};
use crate::width::Width;
use mett_plugin::panel::{BACKGROUND, LIST, NAME, Order, PARENT, ROW, TEXT, Wish};
use std::collections::HashMap;
use windows::Win32::System::Memory::*;

/// So viel eines Elements muss mindestens lesbar sein.
const END: u64 = 0x190;
/// Weit außerhalb des Bildschirms.
const AWAY: f32 = -5000.0;
const MOST: usize = 4096;
/// Java: 4 nach rechts; senkrecht sitzen die Buchstaben so schon mittig in der niedrigeren Zeile.
const JAVA_TEXT: Place = (4.0, -INSET);
/// Um so viel rückt der Hintergrund einer Zeile oben und unten ein. Das Spiel rundet ihn auf ganze Pixel nach außen; ohne
/// den Abstand überlappen sich zwei Zeilen um einen Pixel und es entsteht eine Trennlinie.
const INSET: f32 = 0.05;

type Place = (f32, f32);

pub struct Target {
    pub order: Order,
    /// Lage des Spiels, bevor wir etwas geändert haben.
    original: Option<Place>,
    written: Option<Place>,
    /// Linker Rand der Zeilen, wie das Spiel ihn setzt.
    left: f32,
    /// Höchste gesehene Deckkraft je Zeile.
    full: HashMap<u64, f32>,
    /// Lage des Zeilentexts, wie das Spiel sie setzt.
    text: Option<Place>,
    follow: Option<Follow>,
    /// Anzeige und Liste.
    widths: Vec<(u64, Width)>,
    /// Höhe beim letzten Schreiben.
    height: f32,
}

/// Beschreibbar und lang genug?
pub fn usable(at: u64, size: u64) -> bool {
    let mut m = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(at as *const _), &mut m, size_of::<MEMORY_BASIC_INFORMATION>()) };
    let end = m.BaseAddress as u64 + m.RegionSize as u64;
    let open = n != 0 && m.State == MEM_COMMIT && m.Protect == PAGE_READWRITE;
    open && at >= 0x10000 && at.is_multiple_of(8) && at.checked_add(size).is_some_and(|e| e <= end)
}

pub unsafe fn word(at: u64) -> u64 {
    unsafe { *(at as *const u64) }
}

fn pair(at: u64) -> Place {
    unsafe { (*(at as *const f32), *((at + 4) as *const f32)) }
}

fn sane(p: Place) -> bool {
    [p.0, p.1].iter().all(|n| n.is_finite() && n.abs() < 20_000.0)
}

impl Target {
    pub fn new(order: Order) -> Self {
        Self {
            order,
            original: None,
            written: None,
            left: 0.0,
            full: HashMap::new(),
            text: None,
            follow: None,
            widths: vec![],
            height: -1.0,
        }
    }

    fn is(&self, at: u64, name: &str) -> bool {
        let o = &self.order.offsets;
        if !usable(at, END) || unsafe { word(at) } != self.order.table {
            return false;
        }
        let mut want = [0u8; 32];
        want[..name.len()].copy_from_slice(name.as_bytes());
        want[16..24].copy_from_slice(&name.len().to_le_bytes());
        want[24..].copy_from_slice(&15usize.to_le_bytes());
        unsafe { *((at + o.name) as *const [u8; 32]) == want }
    }

    fn same(&self) -> bool {
        let (o, at) = (&self.order.offsets, self.order.at);
        self.is(at, NAME) && unsafe { word(at + o.parent) } == self.order.parent && self.is(self.order.parent, PARENT)
    }

    /// Bildschirmlage der Eltern: Summe nach oben.
    fn parents(&self) -> Option<Place> {
        let o = &self.order.offsets;
        let mut sum = (0.0, 0.0);
        let mut at = self.order.parent;
        for _ in 0..40 {
            if !usable(at, END) || unsafe { word(at) } != self.order.table {
                return Some(sum).filter(|s| sane(*s));
            }
            let p = pair(at + o.place);
            sum = (sum.0 + p.0, sum.1 + p.1);
            at = unsafe { word(at + o.parent) };
        }
        None
    }

    fn write(&self, at: u64, p: Place) {
        unsafe {
            let to = (at + self.order.offsets.place) as *mut f32;
            to.write(p.0);
            to.add(1).write(p.1);
        }
        self.mark(at);
    }

    /// Geprüfte Kinder in ihrer Reihenfolge.
    fn children(&self, at: u64) -> Vec<u64> {
        let o = &self.order.offsets;
        let (begin, end) = unsafe { (word(at + o.children), word(at + o.children + 8)) };
        let Some(size) = end.checked_sub(begin).filter(|s| *s > 0 && *s <= 0x8000 && s % 16 == 0) else {
            return vec![];
        };
        if !usable(begin, size) {
            return vec![];
        }
        let ours = |c: &u64| usable(*c, END) && unsafe { word(*c) == self.order.table && word(*c + o.parent) == at };
        (0..size / 16).map(|k| unsafe { word(begin + k * 16 + o.half) }).filter(ours).collect()
    }

    /// Text einer Zeile.
    fn text_of(&self, row: u64) -> Option<u64> {
        let child = |at: u64, name: &str| self.children(at).into_iter().find(|c| self.is(*c, name));
        child(child(row, BACKGROUND)?, TEXT)
    }

    /// Hintergrund einer Zeile ohne Trennlinien oder zurück.
    fn tighten(&mut self, row: u64, java: bool) {
        let o = self.order.offsets;
        let Some(back) = self.children(row).into_iter().find(|c| self.is(*c, BACKGROUND)) else { return };
        let (place, size, full) = (pair(back + o.place), pair(back + o.size), pair(row + o.size).1);
        if !sane(place) || !sane(size) || !(1.0..5000.0).contains(&full) {
            return;
        }
        let ours = (size.1 - (full - 2.0 * INSET)).abs() < 0.001 && place.1 == INSET;
        // Nur was genau so liegt, wie das Spiel es setzt: oben in der Zeile und so hoch wie sie.
        let theirs = (size.1 - full).abs() < 0.001 && place.1 == 0.0;
        let set = |height: f32, y: f32| {
            unsafe { ((back + o.size + 4) as *mut f32).write(height) };
            self.write(back, (place.0, y));
        };
        match java {
            true if theirs => set(full - 2.0 * INSET, INSET),
            // Das Spiel hat nur eines von beiden neu gesetzt.
            true if !ours && (place.1 == INSET || place.1 == 0.0) => set(full - 2.0 * INSET, INSET),
            false if ours => set(full, 0.0),
            _ => {}
        }
    }

    /// Text wie in Java eingerückt oder zurück.
    fn indent(&mut self, row: u64, java: bool) {
        let Some(text) = self.text_of(row) else { return };
        let now = pair(text + self.order.offsets.place);
        if !sane(now) {
            return;
        }
        if now != JAVA_TEXT {
            self.text = Some(now);
        }
        let to = if java { JAVA_TEXT } else { self.text.unwrap_or(now) };
        // Ohne bekannte alte Lage bleibt alles, wie es ist.
        if now != to && self.text.is_some() {
            self.write(text, to);
        }
    }

    /// Deckkraft des Texts einer Zeile.
    fn opacity(&self, row: u64) -> Option<f32> {
        let text = self.text_of(row)?;
        let alpha = self.order.offsets.alpha;
        let v = (alpha != 0).then(|| unsafe { *((text + alpha) as *const f32) })?;
        (0.0..=1.0).contains(&v).then_some(v)
    }

    /// Hat das Ausblenden begonnen?
    fn fading(&mut self, row: u64) -> bool {
        let Some(now) = self.opacity(row) else { return false };
        let full = self.full.entry(row).or_insert(now);
        *full = full.max(now);
        now < *full * 0.995
    }

    fn list(&self) -> Option<u64> {
        let lists: Vec<u64> = self.children(self.order.at).into_iter().filter(|c| self.is(*c, LIST)).collect();
        let [list] = lists[..] else { return None };
        Some(list)
    }

    /// Rückrufe an der Liste einrichten oder zurücknehmen.
    fn watch(&mut self, image: (u64, u64)) {
        let o = &self.order.offsets;
        let list = self.list().filter(|_| o.parts != 0 && matches!(self.order.wish, Wish::Place { .. }));
        if self.follow.as_ref().map(|f| f.list) == list {
            return;
        }
        self.follow = None;
        let place = Where { parts: o.parts, owner: o.owner, delta: o.delta };
        self.follow = list.and_then(|l| Follow::attach(l, place, image));
    }

    /// Aus einem Rückruf, mitten im Anordnen: nur nachrücken, wenn sich die Höhe geändert hat.
    pub fn height_changed(&mut self) {
        if self.same() && pair(self.order.at + self.order.offsets.size).1 != self.height {
            self.place();
        }
    }

    /// Breite der Anzeige und ihrer Liste setzen oder die des Spiels zurückgeben. Neue Zeilen richten sich danach.
    fn size(&mut self) {
        let width = match self.order.wish {
            Wish::Place { size, .. } if size.0 > 0.0 => size.0,
            _ => return self.widths.clear(),
        };
        let wanted: Vec<u64> = [Some(self.order.at), self.list()].into_iter().flatten().collect();
        self.widths.retain(|(at, w)| wanted.contains(at) && w.alive());
        for at in wanted {
            if self.widths.iter().all(|w| w.0 != at)
                && let Some(w) = Width::attach(at, &self.order.offsets)
            {
                self.widths.push((at, w));
            }
        }
        // Beide oder keine, sonst passen Anzeige und Zeilen nicht zusammen.
        if self.widths.len() == 2 {
            self.widths.iter_mut().for_each(|(_, w)| w.set(width));
        }
    }

    /// Nur die neuesten Nachrichten zeigen, ältere aus dem Bild rücken. `sudden`: auch die, die gerade verblassen.
    fn limit(&mut self, rows: usize, sudden: bool, high: f32, java: bool) {
        let o = self.order.offsets;
        let Some(list) = self.list() else { return };
        let all: Vec<u64> = self.children(list).into_iter().filter(|c| self.is(*c, ROW)).collect();
        let old = all.len().saturating_sub(rows);
        let end = all.last().map_or(0.0, |r| pair(r + o.place).1 + pair(r + o.size).1);
        // Alle Zeilen stehen am selben linken Rand.
        self.left = all.iter().map(|r| pair(r + o.place).0).find(|x| *x != AWAY).unwrap_or(self.left);
        self.full.retain(|r, _| all.contains(r));
        let count = all.len();
        for (i, row) in all.into_iter().enumerate() {
            self.tighten(row, java);
            self.indent(row, java);
            let now = pair(row + o.place);
            // Die neueste bleibt immer, auch wenn sie allein zu hoch ist.
            let outside = high > 0.0 && i + 1 < count && end - now.1 > high + 0.5;
            let away = self.fading(row) && sudden || i < old || outside;
            let x = if away { AWAY } else { self.left };
            let moved = (now.0 == AWAY) != away;
            if moved && sane(now) && sane((x, now.1)) {
                self.write(row, (x, now.1));
            }
        }
    }

    /// Lässt das Spiel die Bildschirmlage des Elements und aller seiner Kinder neu ausrechnen.
    fn mark(&self, at: u64) {
        let o = &self.order.offsets;
        let mut todo = vec![at];
        let mut left = MOST;
        while let Some(e) = todo.pop() {
            unsafe {
                let flags = (e + o.flags) as *mut u32;
                flags.write(flags.read() | o.bit as u32);
            }
            for child in self.children(e) {
                if left > 0 {
                    left -= 1;
                    todo.push(child);
                }
            }
        }
    }

    /// Ein Schritt. `false`: passt nicht mehr, nie wieder anfassen.
    pub fn step(&mut self, image: (u64, u64)) -> bool {
        if !self.same() || !self.place() {
            return false;
        }
        match self.order.wish {
            Wish::Place { rows, sudden, size, java, .. } => self.limit(rows as usize, sudden, size.1, java),
            Wish::Release | Wish::Hide => self.limit(usize::MAX, false, 0.0, false),
        }
        self.size();
        self.watch(image);
        true
    }

    fn place(&mut self) -> bool {
        let o = &self.order.offsets;
        let (now, size) = (pair(self.order.at + o.place), pair(self.order.at + o.size));
        if !sane(now) || !sane(size) {
            return false;
        }
        // Hat das Spiel selbst neu gerechnet, ist das die neue Ausgangslage.
        if self.written != Some(now) {
            self.original = Some(now);
        }
        let Some(original) = self.original else { return false };
        let to = match self.order.wish {
            Wish::Release => original,
            Wish::Hide => (AWAY, original.1),
            Wish::Place { x, bottom, .. } => {
                let Some(p) = self.parents() else { return false };
                (x - p.0, bottom - size.1 - p.1)
            }
        };
        if !sane(to) {
            return false;
        }
        if to != now {
            self.write(self.order.at, to);
        }
        self.written = Some(to);
        self.height = size.1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mett_plugin::panel::Offsets;

    const IMAGE: (u64, u64) = (0, 0);
    const TABLE: u64 = 0x7FF6_1234_5678;
    const O: Offsets = Offsets {
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
        layout: 0,
        width: 0,
        value: 0,
    };

    /// Elemente in eigenem Speicher, je 0x200 Byte.
    struct Game(*mut u8);

    impl Game {
        fn new() -> Self {
            Game(unsafe { VirtualAlloc(None, 0x10000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) } as *mut u8)
        }

        fn at(&self, i: u64) -> u64 {
            self.0 as u64 + i * 0x200
        }

        fn put<T>(&self, at: u64, v: T) {
            unsafe { (at as *mut T).write_volatile(v) };
        }

        fn add(&self, i: u64, name: &str, parent: u64, place: Place, size: Place) -> u64 {
            let at = self.at(i);
            self.put(at, TABLE);
            let mut n = [0u8; 16];
            n[..name.len()].copy_from_slice(name.as_bytes());
            self.put(at + 0x30, n);
            self.put(at + 0x40, [name.len() as u64, 15]);
            self.put(at + 0x50, place);
            self.put(at + 0x58, size);
            self.put(at + 0xA0, parent);
            at
        }

        fn marked(&self, at: u64) -> bool {
            unsafe { ((at + 0x28) as *const u32).read_volatile() & 1 == 1 }
        }
    }

    fn scene() -> (Game, u64, u64, Order) {
        let g = Game::new();
        let root = g.add(0, "root_panel", 0, (10.0, 20.0), (854.0, 480.0));
        let stack = g.add(1, PARENT, root, (5.0, 0.0), (341.0, 480.0));
        let panel = g.add(2, NAME, stack, (0.0, 62.0), (341.0, 22.0));
        let order = Order {
            client: 1,
            at: panel,
            parent: stack,
            table: TABLE,
            offsets: O,
            wish: Wish::Place { x: 100.0, bottom: 400.0, rows: 10, sudden: false, size: (0.0, 0.0), java: false },
        };
        (g, stack, panel, order)
    }

    #[test]
    fn setzt_den_unteren_rand_und_stellt_zurueck() {
        let (g, _, panel, order) = scene();
        let mut t = Target::new(order);
        assert!(t.step(IMAGE));
        assert_eq!(pair(panel + 0x50), (85.0, 358.0));
        // Wächst der Chat, bleibt sein unterer Rand stehen.
        g.put(panel + 0x58, (341.0f32, 44.0f32));
        assert!(t.step(IMAGE));
        assert_eq!(pair(panel + 0x50), (85.0, 336.0));
        // Das Spiel rechnet neu: Das ist die neue Ausgangslage.
        g.put(panel + 0x50, (0.0f32, 50.0f32));
        assert!(t.step(IMAGE));
        assert_eq!(pair(panel + 0x50), (85.0, 336.0));
        t.order.wish = Wish::Hide;
        assert!(t.step(IMAGE));
        assert_eq!(pair(panel + 0x50), (AWAY, 50.0));
        t.order.wish = Wish::Release;
        assert!(t.step(IMAGE));
        assert_eq!(pair(panel + 0x50), (0.0, 50.0));
    }

    #[test]
    fn markiert_genau_die_eigenen_kinder() {
        let (g, stack, panel, order) = scene();
        let mine = g.add(3, "text", panel, (0.0, 0.0), (1.0, 1.0));
        let deeper = g.add(4, "text", mine, (0.0, 0.0), (1.0, 1.0));
        let other = g.add(5, "text", stack, (0.0, 0.0), (1.0, 1.0));
        g.put(g.at(8), [0, mine, 0, other, 0, 0x18]);
        g.put(panel + 0xA8, [g.at(8), g.at(8) + 48]);
        g.put(g.at(9), [0, deeper]);
        g.put(mine + 0xA8, [g.at(9), g.at(9) + 16]);
        let mut t = Target::new(order);
        assert!(t.step(IMAGE));
        assert!(g.marked(panel) && g.marked(mine) && g.marked(deeper));
        assert!(!g.marked(other) && !g.marked(stack));
        // Ohne Änderung wird nichts markiert.
        g.put(panel + 0x28, 0x67cu32);
        assert!(t.step(IMAGE));
        assert!(!g.marked(panel));
    }

    #[test]
    fn zeigt_nur_die_neuesten() {
        let (g, _, panel, order) = scene();
        let list = g.add(3, LIST, panel, (0.0, -44.0), (341.0, 55.0));
        g.put(g.at(20), [0, list]);
        g.put(panel + 0xA8, [g.at(20), g.at(20) + 16]);
        let rows: Vec<u64> = (0..5).map(|i| g.add(4 + i, ROW, list, (1.5, 11.0 * i as f32), (339.0, 11.0))).collect();
        rows.iter().enumerate().for_each(|(i, r)| g.put(g.at(21) + i as u64 * 16 + 8, *r));
        g.put(list + 0xA8, [g.at(21), g.at(21) + 80]);
        let xs = || rows.iter().map(|r| pair(r + 0x50).0).collect::<Vec<_>>();
        let want = |rows| Wish::Place { x: 0.0, bottom: 400.0, rows, sudden: false, size: (0.0, 0.0), java: false };
        let mut t = Target::new(order);
        for (wish, shown) in [(want(2), 2), (want(4), 4), (want(1), 1), (want(9), 5), (want(1), 1), (Wish::Hide, 5)] {
            t.order.wish = wish;
            assert!(t.step(IMAGE));
            assert!(xs().iter().enumerate().all(|(i, x)| *x == if i < 5 - shown { AWAY } else { 1.5 }), "{:?}", xs());
        }
        // Höhen bleiben unberührt, und am Ende steht alles wieder da.
        t.order.wish = want(2);
        assert!(t.step(IMAGE));
        assert!(rows.iter().enumerate().all(|(i, r)| pair(r + 0x50).1 == 11.0 * i as f32));
        t.order.wish = Wish::Release;
        assert!(t.step(IMAGE));
        assert_eq!(xs(), [1.5; 5]);
        // Nur was in die Höhe passt; die neueste immer.
        for (high, shown) in [(33.0, 3), (34.0, 3), (43.0, 3), (44.0, 4), (1.0, 1)] {
            t.order.wish =
                Wish::Place { x: 0.0, bottom: 400.0, rows: 9, sudden: false, size: (0.0, high), java: false };
            assert!(t.step(IMAGE));
            assert_eq!(xs().iter().filter(|x| **x == 1.5).count(), shown, "{high}");
        }
        t.order.wish = Wish::Release;
        assert!(t.step(IMAGE));
        // Andere Einträge gehen uns nichts an.
        g.put(rows[0] + 0x30, *b"other_grid_item");
        g.put(rows[0] + 0x40, 15u64);
        t.order.wish = want(1);
        assert!(t.step(IMAGE));
        assert_eq!(xs(), [1.5, AWAY, AWAY, AWAY, 1.5]);
    }

    #[test]
    fn ohne_animation_verschwindet_sofort() {
        let (g, _, panel, order) = scene();
        let list = g.add(3, LIST, panel, (0.0, 0.0), (341.0, 22.0));
        let row = g.add(4, ROW, list, (1.5, 0.0), (339.0, 11.0));
        let back = g.add(5, BACKGROUND, row, (0.0, 0.0), (339.0, 11.0));
        let text = g.add(6, TEXT, back, (2.0, 0.0), (334.0, 11.0));
        for (i, (owner, child)) in [(panel, list), (list, row), (row, back), (back, text)].into_iter().enumerate() {
            let l = g.at(20 + i as u64);
            g.put(l, [0, child]);
            g.put(owner + 0xA8, [l, l + 16]);
        }
        let mut t = Target::new(order);
        let want = |sudden| Wish::Place { x: 0.0, bottom: 400.0, rows: 10, sudden, size: (0.0, 0.0), java: false };
        // (Schalter, Deckkraft, zu sehen). Eine von Anfang an schwächere Deckkraft ist kein Ausblenden.
        let steps =
            [(true, 0.8, true), (true, 0.79, false), (false, 0.79, true), (true, 0.3, false), (true, 0.8, true)];
        for (sudden, alpha, seen) in steps {
            g.put(text + 0x70, alpha as f32);
            t.order.wish = want(sudden);
            assert!(t.step(IMAGE));
            assert_eq!(pair(row + 0x50).0, if seen { 1.5 } else { AWAY }, "{sudden} {alpha}");
        }
        g.put(text + 0x70, 0.1f32);
        assert!(t.step(IMAGE));
        assert_eq!(pair(row + 0x50).0, AWAY);
        t.order.wish = Wish::Release;
        assert!(t.step(IMAGE));
        assert_eq!(pair(row + 0x50).0, 1.5);
        // Ohne bekannte Deckkraft passiert nichts.
        let mut t = Target::new(Order { offsets: Offsets { alpha: 0, ..O }, wish: want(true), ..order });
        g.put(text + 0x70, 1.0f32);
        assert!(t.step(IMAGE));
        g.put(text + 0x70, 0.2f32);
        assert!(t.step(IMAGE));
        assert_eq!(pair(row + 0x50).0, 1.5);
    }

    #[test]
    fn rueckt_aus_dem_rueckruf_nach() {
        let (g, _, panel, order) = scene();
        let mut t = Target::new(order);
        assert!(t.step(IMAGE));
        g.put(panel + 0x58, (341.0f32, 11.0f32));
        t.height_changed();
        assert_eq!(pair(panel + 0x50), (85.0, 369.0));
        // Gleiche Höhe: nichts schreiben. Anderes Element: erst recht nicht.
        g.put(panel + 0x50, (1.0f32, 2.0f32));
        t.height_changed();
        assert_eq!(pair(panel + 0x50), (1.0, 2.0));
        g.put(panel + 0x58, (341.0f32, 33.0f32));
        g.put(panel + 0x30, *b"other");
        t.height_changed();
        assert_eq!(pair(panel + 0x50), (1.0, 2.0));
    }

    #[test]
    fn rueckt_den_text_wie_in_java_ein() {
        let (g, _, panel, order) = scene();
        let list = g.add(3, LIST, panel, (0.0, 0.0), (341.0, 22.0));
        let row = g.add(4, ROW, list, (0.0, 0.0), (339.0, 11.0));
        let back = g.add(5, BACKGROUND, row, (0.0, 0.0), (339.0, 11.0));
        let text = g.add(6, TEXT, back, (2.0, 0.0), (334.0, 11.0));
        for (i, (owner, child)) in [(panel, list), (list, row), (row, back), (back, text)].into_iter().enumerate() {
            let l = g.at(20 + i as u64);
            g.put(l, [0, child]);
            g.put(owner + 0xA8, [l, l + 16]);
        }
        let mut t = Target::new(order);
        let want = |java| Wish::Place { x: 0.0, bottom: 400.0, rows: 10, sudden: false, size: (0.0, 0.0), java };
        for (java, place) in [(true, JAVA_TEXT), (false, (2.0, 0.0)), (true, JAVA_TEXT)] {
            t.order.wish = want(java);
            assert!(t.step(IMAGE) && t.step(IMAGE));
            assert_eq!(pair(text + 0x50), place);
            // Der Hintergrund lässt oben und unten gleich viel frei.
            let (at, size) = (pair(back + 0x50), pair(back + 0x58));
            let want = if java { (INSET, 11.0 - 2.0 * INSET) } else { (0.0, 11.0) };
            assert!(at.1 == want.0 && (size.1 - want.1).abs() < 0.0001 && size.0 == 339.0, "{at:?} {size:?}");
        }
        t.order.wish = Wish::Release;
        assert!(t.step(IMAGE));
        assert_eq!(pair(text + 0x50), (2.0, 0.0));
        assert_eq!((pair(back + 0x50).1, pair(back + 0x58).1), (0.0, 11.0));
        // Das Spiel ordnet neu an: Beides kommt wieder.
        t.order.wish = want(true);
        assert!(t.step(IMAGE));
        g.put(back + 0x58, (339.0f32, 11.0f32));
        assert!(t.step(IMAGE));
        assert!((pair(back + 0x58).1 - 10.9).abs() < 0.0001);
    }

    #[test]
    fn jede_abweichung_laesst_die_finger_davon() {
        for spoil in [0u64, 0x30, 0x40, 0xA0] {
            let (g, _, panel, order) = scene();
            g.put(panel + spoil, 0xFFFF_FFFFu32);
            assert!(!Target::new(order).step(IMAGE), "{spoil:x}");
            assert_eq!(pair(panel + 0x50), (0.0, 62.0));
        }
        let (g, stack, panel, order) = scene();
        g.put(stack + 0x30, *b"other");
        assert!(!Target::new(order).step(IMAGE));
        assert_eq!(pair(panel + 0x50), (0.0, 62.0));
        let (g, _, panel, order) = scene();
        g.put(panel + 0x50, (f32::NAN, 0.0f32));
        assert!(!Target::new(order).step(IMAGE));
        assert!(!Target::new(Order { at: 0x10, ..order }).step(IMAGE));
        assert!(!Target::new(Order { at: u64::MAX - 7, ..order }).step(IMAGE));
    }
}
