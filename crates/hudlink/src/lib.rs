//! Verbindung zwischen Client und HUD-Plugin über gemeinsamen Speicher: Zeichenbefehle hin, Eingaben zurück.
//! Nichts davon landet auf der Platte. Meldet sich der Client nicht mehr, gibt das Plugin alles frei.
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering::SeqCst};
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::*;
use windows::core::{Error, HSTRING, Result};

const MAGIC: u32 = 0x4D48_5544;
const API: u32 = 9;
/// So lange darf der Client schweigen.
pub const TIMEOUT_MS: u64 = 2000;
const EVENTS: usize = 1024;
const DATA: usize = 1 << 20;
/// Gleichzeitig mögliche Symboltafeln und ihre Größe (512 × 512).
const SHEETS: usize = 32;
const SHEET: usize = 1 << 20;

/// Wünsche des Clients.
pub mod control {
    /// Chat-Tasten gehören uns.
    pub const CHAT: u32 = 1;
    /// Die Befehlstaste auch.
    pub const COMMANDS: u32 = 2;
    /// Menü oder Editor offen.
    pub const MENU: u32 = 4;
    /// Zeigerbild im Menü: 0 Pfeil, 1 waagrecht, 2 senkrecht, 3 schräg wie \\, 4 schräg wie /.
    pub const SHAPE_SHIFT: u32 = 4;
    pub const SHAPE_MASK: u32 = 7;
    /// Die Tab-Taste gehört beim Spielen uns.
    pub const TAB: u32 = 8;
    /// Zoom: Taste, Mausrad, Stufen-Tasten und Zweit-Zoom gehören beim Spielen uns.
    pub const ZOOM: u32 = 128;
    pub const ZOOM_WHEEL: u32 = 256;
    pub const ZOOM_KEYS: u32 = 512;
    pub const ZOOM_SECOND: u32 = 1024;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Chat geöffnet, mit Schrägstrich bei der Befehlstaste.
    Open {
        command: bool,
    },
    Char(char),
    Key {
        vk: u8,
        shift: bool,
        ctrl: bool,
    },
    Wheel {
        notches: i8,
        shift: bool,
    },
    Move {
        x: i16,
        y: i16,
    },
    /// 0 links, 1 rechts.
    Button {
        which: u8,
        down: bool,
    },
    /// Tab-Taste beim Spielen gedrückt oder losgelassen.
    Tab {
        down: bool,
    },
    /// Das Plugin hat von selbst geschlossen (z. B. Anzeige ausgefallen).
    Closed,
}

impl Event {
    fn pack(self) -> u64 {
        let (kind, v): (u64, u64) = match self {
            Event::Open { command } => (1, command as u64),
            Event::Char(c) => (2, c as u64),
            Event::Key { vk, shift, ctrl } => (3, vk as u64 | (shift as u64) << 8 | (ctrl as u64) << 9),
            Event::Wheel { notches, shift } => (4, notches as u8 as u64 | (shift as u64) << 8),
            Event::Move { x, y } => (5, x as u16 as u64 | (y as u16 as u64) << 16),
            Event::Button { which, down } => (6, which as u64 | (down as u64) << 8),
            Event::Closed => (7, 0),
            Event::Tab { down } => (8, down as u64),
        };
        kind << 56 | v
    }

    fn unpack(v: u64) -> Option<Self> {
        let bit = |n: u32| v >> n & 1 != 0;
        Some(match v >> 56 {
            1 => Event::Open { command: bit(0) },
            2 => Event::Char(char::from_u32(v as u32)?),
            3 => Event::Key { vk: v as u8, shift: bit(8), ctrl: bit(9) },
            4 => Event::Wheel { notches: v as u8 as i8, shift: bit(8) },
            5 => Event::Move { x: v as u16 as i16, y: (v >> 16) as u16 as i16 },
            6 => Event::Button { which: v as u8, down: bit(8) },
            7 => Event::Closed,
            8 => Event::Tab { down: bit(0) },
            _ => return None,
        })
    }
}

#[repr(C)]
struct Block {
    magic: AtomicU32,
    api: AtomicU32,
    /// Breite und Höhe des Bilds, je 16 Bit. 0: noch kein Bild.
    screen: AtomicU32,
    /// Gezählte Bilder und ihre mittlere Dauer in Mikrosekunden.
    frames: AtomicU32,
    failures: AtomicU32,
    control: AtomicU32,
    heartbeat: AtomicU64,
    keys: [AtomicU32; 8],
    /// Zoom, näher, weiter, Zweit-Zoom.
    zoom: [AtomicU32; 4],
    written: AtomicU32,
    read: AtomicU32,
    events: [AtomicU64; EVENTS],
    /// Ungerade, solange geschrieben wird.
    sequence: AtomicU32,
    length: AtomicU32,
    data: [AtomicU8; DATA],
    sheets: [SheetSlot; SHEETS],
}

#[repr(C)]
struct SheetSlot {
    /// Ungerade, solange geschrieben wird.
    sequence: AtomicU32,
    /// Kennung und Kante; Kante 0 heißt leer.
    id: AtomicU32,
    side: AtomicU32,
    data: [AtomicU8; SHEET],
}

struct Map {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    wake: HANDLE,
}

// Zugriff nur über atomare Felder.
unsafe impl Send for Map {}
unsafe impl Sync for Map {}

impl Map {
    fn names(pid: u32) -> (HSTRING, HSTRING) {
        (HSTRING::from(format!("MettHud.{pid}")), HSTRING::from(format!("MettHud.{pid}.Ereignis")))
    }

    fn create(pid: u32) -> Result<Self> {
        let (map, event) = Self::names(pid);
        unsafe {
            let size = size_of::<Block>() as u32;
            let handle = CreateFileMappingW(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, size, &map)?;
            // Einen fremden oder alten Bereich gleichen Namens nicht benutzen.
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return Err(ERROR_ALREADY_EXISTS.into());
            }
            Self::view(handle, CreateEventW(None, false, false, &event))
        }
    }

    fn open(pid: u32) -> Result<Self> {
        let (map, event) = Self::names(pid);
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, &map)?;
            Self::view(handle, OpenEventW(SYNCHRONIZATION_ACCESS_RIGHTS(0x0010_0002), false, &event))
        }
    }

    unsafe fn view(handle: HANDLE, wake: Result<HANDLE>) -> Result<Self> {
        unsafe {
            let wake = wake.inspect_err(|_| {
                let _ = CloseHandle(handle);
            })?;
            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size_of::<Block>());
            if view.Value.is_null() {
                let e = Error::from_thread();
                let _ = CloseHandle(handle);
                let _ = CloseHandle(wake);
                return Err(e);
            }
            Ok(Self { handle, view, wake })
        }
    }

    fn block(&self) -> &Block {
        unsafe { &*self.view.Value.cast::<Block>() }
    }
}

impl Drop for Map {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
            let _ = CloseHandle(self.wake);
        }
    }
}

fn now() -> u64 {
    unsafe { GetTickCount64() }
}

/// Seite im Spiel.
pub struct Server {
    map: Map,
    seen: AtomicU32,
    sheets: [AtomicU32; SHEETS],
}

/// Geänderte Symboltafel: Platz, Kennung, Kante und Bildpunkte; leer heißt weg.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetData {
    pub slot: usize,
    pub id: u8,
    pub side: u32,
    pub pixels: Vec<u8>,
}

impl Server {
    pub fn create() -> Result<Self> {
        let map = Map::create(std::process::id())?;
        let b = map.block();
        b.api.store(API, SeqCst);
        b.magic.store(MAGIC, SeqCst);
        Ok(Self { map, seen: AtomicU32::new(0), sheets: std::array::from_fn(|_| AtomicU32::new(0)) })
    }

    /// Hat sich der Client gerade erst gemeldet?
    pub fn alive(&self) -> bool {
        let last = self.map.block().heartbeat.load(SeqCst);
        last != 0 && now().saturating_sub(last) < TIMEOUT_MS
    }

    /// Wünsche des Clients; ohne Client keine.
    pub fn control(&self) -> u32 {
        if self.alive() { self.map.block().control.load(SeqCst) } else { 0 }
    }

    /// Chat-Tasten und Befehlstasten.
    pub fn is_key(&self, vk: u8, command: bool) -> bool {
        let keys = &self.map.block().keys;
        let range = if command { 4..8 } else { 0..4 };
        vk != 0 && keys[range].iter().any(|k| k.load(SeqCst) == vk as u32)
    }

    /// Zoom, näher, weiter, Zweit-Zoom.
    pub fn zoom_keys(&self) -> [u8; 4] {
        std::array::from_fn(|i| self.map.block().zoom[i].load(SeqCst) as u8)
    }

    pub fn push(&self, e: Event) {
        let b = self.map.block();
        let w = b.written.load(SeqCst);
        // Liest niemand, läuft nichts über.
        if w.wrapping_sub(b.read.load(SeqCst)) as usize >= EVENTS {
            return;
        }
        b.events[w as usize % EVENTS].store(e.pack(), SeqCst);
        b.written.store(w.wrapping_add(1), SeqCst);
        unsafe {
            let _ = SetEvent(self.map.wake);
        }
    }

    pub fn report(&self, screen: (u32, u32), frames: u32, failures: u32) {
        let b = self.map.block();
        b.screen.store(screen.0.min(0xFFFF) << 16 | screen.1.min(0xFFFF), SeqCst);
        b.frames.store(frames, SeqCst);
        b.failures.store(failures, SeqCst);
    }

    /// Neue Zeichenbefehle seit dem letzten Mal. `None`: unverändert oder gerade im Umbau.
    pub fn scene(&self) -> Option<Vec<u8>> {
        let b = self.map.block();
        let s = b.sequence.load(SeqCst);
        if s & 1 != 0 || s == self.seen.load(SeqCst) {
            return None;
        }
        let n = (b.length.load(SeqCst) as usize).min(DATA);
        let mut out = vec![0u8; n];
        unsafe { std::ptr::copy_nonoverlapping(b.data.as_ptr().cast::<u8>(), out.as_mut_ptr(), n) };
        std::sync::atomic::fence(SeqCst);
        if b.sequence.load(SeqCst) != s {
            return None;
        }
        self.seen.store(s, SeqCst);
        Some(out)
    }
}

impl Server {
    /// Symboltafeln, die sich seit dem letzten Mal geändert haben.
    pub fn sheets(&self) -> Vec<SheetData> {
        let mut out = vec![];
        for (slot, (s, seen)) in self.map.block().sheets.iter().zip(&self.sheets).enumerate() {
            let q = s.sequence.load(SeqCst);
            if q & 1 != 0 || q == seen.load(SeqCst) {
                continue;
            }
            let (id, side) = (s.id.load(SeqCst) as u8, s.side.load(SeqCst));
            let n = (side as usize).saturating_mul(side as usize).saturating_mul(4);
            if n > SHEET {
                continue;
            }
            let mut pixels = vec![0u8; n];
            unsafe { std::ptr::copy_nonoverlapping(s.data.as_ptr().cast::<u8>(), pixels.as_mut_ptr(), n) };
            std::sync::atomic::fence(SeqCst);
            if s.sequence.load(SeqCst) == q {
                seen.store(q, SeqCst);
                out.push(SheetData { slot, id, side, pixels });
            }
        }
        out
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.map.block().magic.store(0, SeqCst);
        unsafe {
            let _ = SetEvent(self.map.wake);
        }
    }
}

/// Seite im Client.
pub struct Client {
    map: Map,
}

impl Client {
    pub fn open(pid: u32) -> Result<Self> {
        let map = Map::open(pid)?;
        let b = map.block();
        if b.magic.load(SeqCst) != MAGIC || b.api.load(SeqCst) != API {
            return Err(ERROR_INVALID_DATA.into());
        }
        // Altes aus einer früheren Verbindung verwerfen.
        b.read.store(b.written.load(SeqCst), SeqCst);
        Ok(Self { map })
    }

    /// Läuft das Plugin noch?
    pub fn connected(&self) -> bool {
        self.map.block().magic.load(SeqCst) == MAGIC
    }

    pub fn beat(&self, control: u32) {
        let b = self.map.block();
        b.control.store(control, SeqCst);
        b.heartbeat.store(now().max(1), SeqCst);
    }

    pub fn set_zoom_keys(&self, keys: [u8; 4]) {
        self.map.block().zoom.iter().zip(keys).for_each(|(k, v)| k.store(v as u32, SeqCst));
    }

    pub fn set_keys(&self, chat: &[u16], command: &[u16]) {
        let keys = &self.map.block().keys;
        for (i, k) in keys.iter().enumerate() {
            let list = if i < 4 { chat } else { command };
            k.store(list.get(i % 4).copied().unwrap_or(0) as u32, SeqCst);
        }
    }

    /// Bildgröße des Spiels, sobald das erste Bild gezeichnet ist.
    pub fn screen(&self) -> Option<(u32, u32)> {
        let s = self.map.block().screen.load(SeqCst);
        (s >> 16 != 0 && s & 0xFFFF != 0).then_some((s >> 16, s & 0xFFFF))
    }

    /// (Bilder, Fehler)
    pub fn counts(&self) -> (u32, u32) {
        (self.map.block().frames.load(SeqCst), self.map.block().failures.load(SeqCst))
    }

    /// `false`: zu groß, nichts übertragen.
    pub fn send(&self, scene: &[u8]) -> bool {
        if scene.len() > DATA {
            return false;
        }
        let b = self.map.block();
        // Nur der Client schreibt.
        let s = b.sequence.load(SeqCst) | 1;
        b.sequence.store(s, SeqCst);
        std::sync::atomic::fence(SeqCst);
        unsafe { std::ptr::copy_nonoverlapping(scene.as_ptr(), b.data.as_ptr().cast_mut().cast::<u8>(), scene.len()) };
        b.length.store(scene.len() as u32, SeqCst);
        b.sequence.store(s.wrapping_add(1), SeqCst);
        true
    }

    /// Symboltafel auf einen Platz legen; Kante 0 leert ihn. `false`: passt nicht.
    pub fn send_sheet(&self, slot: usize, id: u8, side: u32, pixels: &[u8]) -> bool {
        let Some(s) = self.map.block().sheets.get(slot) else { return false };
        if pixels.len() > SHEET || pixels.len() != side as usize * side as usize * 4 {
            return false;
        }
        let q = s.sequence.load(SeqCst) | 1;
        s.sequence.store(q, SeqCst);
        std::sync::atomic::fence(SeqCst);
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), s.data.as_ptr().cast_mut().cast::<u8>(), pixels.len())
        };
        s.id.store(id as u32, SeqCst);
        s.side.store(side, SeqCst);
        s.sequence.store(q.wrapping_add(1), SeqCst);
        true
    }

    /// So viele Tafeln passen gleichzeitig.
    pub const SHEETS: usize = SHEETS;

    pub fn events(&self) -> Vec<Event> {
        let b = self.map.block();
        let mut out = vec![];
        let end = b.written.load(SeqCst);
        let mut at = b.read.load(SeqCst);
        if end.wrapping_sub(at) as usize > EVENTS {
            at = end;
        }
        while at != end {
            out.extend(Event::unpack(b.events[at as usize % EVENTS].load(SeqCst)));
            at = at.wrapping_add(1);
        }
        b.read.store(end, SeqCst);
        out
    }

    /// Wartet auf neue Eingaben, höchstens so lange.
    pub fn wait(&self, ms: u32) -> bool {
        unsafe { WaitForSingleObject(self.map.wake, ms) == WAIT_OBJECT_0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ereignisse_hin_und_zurueck() {
        let all = [
            Event::Open { command: true },
            Event::Open { command: false },
            Event::Char('😀'),
            Event::Key { vk: 0xFF, shift: true, ctrl: false },
            Event::Key { vk: 1, shift: false, ctrl: true },
            Event::Wheel { notches: -1, shift: true },
            Event::Move { x: -5, y: 2000 },
            Event::Button { which: 1, down: true },
            Event::Closed,
        ];
        for e in all {
            assert_eq!(Event::unpack(e.pack()), Some(e));
        }
        assert_eq!(Event::unpack(0), None);
        assert_eq!(Event::unpack(2 << 56 | 0xD800), None);
    }

    /// Alles in einem Test: Der Name hängt am Prozess.
    #[test]
    fn verbindung() {
        assert!(Client::open(std::process::id()).is_err());
        let s = Server::create().unwrap();
        assert!(Server::create().is_err());
        let c = Client::open(std::process::id()).unwrap();
        assert!(!s.alive() && s.control() == 0 && s.scene().is_none() && c.screen().is_none());
        c.beat(control::CHAT | control::MENU);
        c.set_keys(&[84, 13], &[191]);
        assert_eq!(s.control(), 5);
        assert!(s.is_key(84, false) && s.is_key(13, false) && s.is_key(191, true));
        assert!(!s.is_key(191, false) && !s.is_key(84, true) && !s.is_key(0, false));
        assert!(c.send(b"abc") && !c.send(&vec![0; DATA + 1]));
        assert_eq!(s.scene().as_deref(), Some(&b"abc"[..]));
        assert_eq!(s.scene(), None);
        c.send(b"");
        assert_eq!(s.scene().as_deref(), Some(&b""[..]));
        s.push(Event::Char('ä'));
        assert!(c.wait(100));
        assert_eq!(c.events(), [Event::Char('ä')]);
        assert!(c.events().is_empty());
        // Läuft nie über und verliert nur das Neueste.
        (0..2000).for_each(|_| s.push(Event::Closed));
        assert_eq!(c.events().len(), EVENTS);
        s.report((2560, 1440), 7, 1);
        assert_eq!((c.screen(), c.counts()), (Some((2560, 1440)), (7, 1)));
        // Gleichzeitig schreiben und lesen: nie ein halber Stand.
        std::thread::scope(|t| {
            t.spawn(|| (0..3000u32).for_each(|i| assert!(c.send(&vec![i as u8; 5000]))));
            for _ in 0..3000 {
                if let Some(v) = s.scene() {
                    assert!(v.is_empty() || (v.len() == 5000 && v.iter().all(|x| *x == v[0])));
                }
            }
        });
        // Symboltafeln: nur Geändertes, falsche Maße nie.
        assert!(s.sheets().is_empty());
        assert!(c.send_sheet(2, 0xE4, 2, &[5; 16]));
        assert!(!c.send_sheet(2, 0xE4, 2, &[5; 15]) && !c.send_sheet(SHEETS, 0xE4, 2, &[5; 16]));
        assert!(!c.send_sheet(0, 0xE4, 1024, &vec![0; 4 << 20]));
        assert_eq!(s.sheets(), [SheetData { slot: 2, id: 0xE4, side: 2, pixels: vec![5; 16] }]);
        assert!(s.sheets().is_empty());
        assert!(c.send_sheet(2, 0, 0, &[]));
        assert_eq!(s.sheets(), [SheetData { slot: 2, id: 0, side: 0, pixels: vec![] }]);
        // Schweigt der Client, gilt nichts mehr.
        c.map.block().heartbeat.store(now().saturating_sub(TIMEOUT_MS + 1).max(1), SeqCst);
        assert!(!s.alive() && s.control() == 0);
        drop(s);
        assert!(!c.connected());
    }
}
