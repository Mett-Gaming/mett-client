//! Rechtzeitig Bescheid bekommen: An den Bausteinen der Nachrichtenliste ruft das Spiel beim Anordnen leere Rückrufe auf.
//! Ihre Objekte bekommen eine Kopie der Funktionstabelle, in der diese leeren Funktionen auf uns zeigen. Ersetzt wird
//! nur, was nachweislich nichts tut; der Code des Spiels bleibt unberührt. Nur im Hauptthread benutzen.
use crate::panel::{usable, word};
use std::panic::catch_unwind;
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use windows::Win32::System::Memory::*;

const SLOTS: usize = 16;
/// Kopierte Einträge; was hinter den eigenen Funktionen liegt, wird nie darüber aufgerufen.
const COPIED: u64 = 32;
const MOST: usize = 8;

/// Objekte, die gerade unsere Tabelle benutzen. Nur für sie gilt ein Rückruf.
static AT: [AtomicU64; MOST] = [const { AtomicU64::new(0) }; MOST];

struct Part {
    at: u64,
    original: u64,
    ours: u64,
    /// Wo und was der Verweis auf das Element ist.
    owner: (u64, u64),
}

/// Hält die Tabellen einer Liste.
pub struct Follow {
    pub list: u64,
    parts: Vec<Part>,
}

/// Was die Funktion tut, wenn es sicher nichts ist.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Empty {
    /// `ret`
    Nothing,
    /// `xor al, al; ret`
    No,
}

fn protection(at: u64, size: u64) -> Option<PAGE_PROTECTION_FLAGS> {
    let mut m = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(at as *const _), &mut m, size_of::<MEMORY_BASIC_INFORMATION>()) };
    let end = m.BaseAddress as u64 + m.RegionSize as u64;
    (n != 0 && m.State == MEM_COMMIT && at >= 0x10000 && at.checked_add(size)? <= end).then_some(m.Protect)
}

fn empty(f: u64) -> Option<Empty> {
    let code = [PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY];
    if !protection(f, 16).is_some_and(|p| code.contains(&p)) {
        return None;
    }
    match unsafe { *(f as *const [u8; 3]) } {
        [0xC3, ..] | [0xC2, 0, 0] => Some(Empty::Nothing),
        [0x32 | 0x30, 0xC0, 0xC3] => Some(Empty::No),
        _ => None,
    }
}

fn called(this: u64) {
    if AT.iter().any(|a| a.load(SeqCst) == this) {
        let _ = catch_unwind(crate::height_changed);
    }
}

unsafe extern "system" fn nothing(this: u64) {
    called(this);
}

unsafe extern "system" fn no(this: u64) -> u64 {
    called(this);
    0
}

/// Kopie der Tabelle samt dem Wort davor. Bleibt bis zum Prozessende bestehen.
fn copy(original: u64) -> Option<u64> {
    let read = [PAGE_READONLY, PAGE_READWRITE, PAGE_EXECUTE_READ, PAGE_WRITECOPY];
    if !protection(original - 8, (COPIED + 1) * 8).is_some_and(|p| read.contains(&p)) {
        return None;
    }
    let mut v: Vec<u64> = (0..=COPIED).map(|i| unsafe { word(original - 8 + i * 8) }).collect();
    let mut replaced = false;
    // Der Aufräumer (0) bleibt immer der des Spiels.
    for f in &mut v[2..=SLOTS] {
        match empty(*f) {
            Some(Empty::Nothing) => *f = nothing as *const () as u64,
            Some(Empty::No) => *f = no as *const () as u64,
            None => continue,
        }
        replaced = true;
    }
    replaced.then(|| v.leak().as_ptr() as u64 + 8)
}

/// Wo in einem Element die Liste seiner Bausteine steht, und wo ein Baustein sein Element nennt.
#[derive(Clone, Copy)]
pub struct Where {
    pub parts: u64,
    pub owner: u64,
    pub delta: u64,
}

impl Follow {
    /// `image`: Anfang und Größe des Programms; nur Tabellen daraus gelten.
    pub fn attach(list: u64, w: Where, image: (u64, u64)) -> Option<Self> {
        let (begin, end) = unsafe { (word(list + w.parts), word(list + w.parts + 8)) };
        let size = end.checked_sub(begin).filter(|s| *s > 0 && *s <= MOST as u64 * 8 && s % 8 == 0)?;
        if !usable(begin, size) {
            return None;
        }
        let all: Vec<u64> = (0..size / 8).map(|i| unsafe { word(begin + i * 8) }).collect();
        let fits = |c: &u64| {
            let program = image.0..image.0 + image.1;
            usable(*c, 0x20) && unsafe { program.contains(&word(*c)) && word(*c + w.owner) == list + w.delta }
        };
        if !all.iter().all(fits) || AT.iter().any(|a| a.load(SeqCst) != 0) {
            return None;
        }
        let mut parts = vec![];
        for at in all {
            let original = unsafe { word(at) };
            let Some(ours) = copy_once(original) else { continue };
            AT[parts.len()].store(at, SeqCst);
            unsafe { (at as *mut u64).write_volatile(ours) };
            parts.push(Part { at, original, ours, owner: (at + w.owner, list + w.delta) });
        }
        Some(Self { list, parts })
    }
}

/// Je Tabelle nur eine Kopie, auch über mehrere Welten.
fn copy_once(original: u64) -> Option<u64> {
    static DONE: std::sync::Mutex<Vec<(u64, u64)>> = std::sync::Mutex::new(vec![]);
    let mut done = DONE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, ours)) = done.iter().find(|d| d.0 == original) {
        return Some(*ours);
    }
    let ours = copy(original)?;
    done.push((original, ours));
    Some(ours)
}

impl Drop for Follow {
    /// Nur zurück, wo noch unsere Tabelle am selben Element steht. Andere Objekte gibt es nicht mehr.
    fn drop(&mut self) {
        for p in &self.parts {
            unsafe {
                if usable(p.at, 0x20) && word(p.at) == p.ours && word(p.owner.0) == p.owner.1 {
                    (p.at as *mut u64).write_volatile(p.original);
                }
            }
        }
        AT.iter().for_each(|a| a.store(0, SeqCst));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn real(this: u64) -> u64 {
        this + 1
    }

    #[test]
    fn ersetzt_nur_leere_und_stellt_zurueck() {
        unsafe {
            let code = VirtualAlloc(None, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) as *mut u8;
            let put = |o: usize, b: &[u8]| std::ptr::copy_nonoverlapping(b.as_ptr(), code.add(o), b.len());
            put(0, &[0xC3]);
            put(16, &[0xC2, 0, 0]);
            put(32, &[0x32, 0xC0, 0xC3]);
            put(48, &[0x48, 0x8B, 0xC1, 0xC3]);
            put(64, &[0xC2, 8, 0]);
            let f = |o: usize| code as u64 + o as u64;
            use Empty::*;
            let found = [0, 16, 32, 48, 64].map(|o| empty(f(o)));
            assert_eq!(found, [Some(Nothing), Some(Nothing), Some(No), None, None]);
            assert_eq!(empty(0x10), None);

            let mem = VirtualAlloc(None, 0x10000, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) as u64;
            let set = |o: u64, v: u64| ((mem + o) as *mut u64).write_volatile(v);
            let real = real as *const () as u64;
            (0..48u64).for_each(|i| set(0x1000 + i * 8, real));
            let table = mem + 0x1008;
            // Aufräumer (leer), leer, nein.
            set(0x1008, f(0));
            set(0x1010, f(0));
            set(0x1018, f(32));
            let list = mem + 0x2000;
            set(0x2000 + 0xC8, mem + 0x3000);
            set(0x2000 + 0xD0, mem + 0x3010);
            for (i, part) in [0x4000u64, 0x4100].into_iter().enumerate() {
                set(0x3000 + i as u64 * 8, mem + part);
                set(part, table);
                set(part + 8, list + 0x10);
            }
            let w = Where { parts: 0xC8, owner: 8, delta: 0x10 };
            let image = (mem, 0x10000);
            // Fremder Baustein oder Tabelle außerhalb des Programms: nichts anfassen.
            assert!(Follow::attach(list, Where { delta: 0x18, ..w }, image).is_none());
            assert!(Follow::attach(list, w, (mem, 0x100)).is_none());
            assert_eq!(word(mem + 0x4000), table);

            let follow = Follow::attach(list, w, image).unwrap();
            let new = word(mem + 0x4000);
            assert!(new != table && word(mem + 0x4100) == new);
            // Immer nur eine Liste.
            assert!(Follow::attach(list, w, image).is_none());
            assert_eq!((word(new), word(new + 24), word(new - 8)), (f(0), real, real));
            type F = unsafe extern "system" fn(u64) -> u64;
            let call = |slot: u64| std::mem::transmute::<u64, F>(word(new + slot * 8))(mem + 0x4000);
            call(1);
            assert_eq!((call(2), call(3)), (0, mem + 0x4001));
            // Ein Baustein gehört inzwischen zu etwas anderem: Finger weg.
            set(0x4108, 0x1234);
            drop(follow);
            assert_eq!((word(mem + 0x4000), word(mem + 0x4100)), (table, new));
            // Danach wirkt ein verspäteter Aufruf nicht mehr, und es geht von vorn.
            call(1);
            set(0x4100, table);
            set(0x4108, list + 0x10);
            drop(Follow::attach(list, w, image).unwrap());
            assert_eq!((word(mem + 0x4000), word(mem + 0x4100)), (table, table));
        }
    }
}
