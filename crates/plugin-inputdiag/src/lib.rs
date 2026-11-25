//! Nur für die Entwicklung, wird nie ausgeliefert. Zählt, welche Funktionen von GameInput das Spiel aufruft.
//! Reicht jeden Aufruf unverändert weiter; Inhalte werden nicht gelesen.
use mett_plugin::{API, Host, Info};
use std::arch::naked_asm;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering::SeqCst};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::core::{GUID, HRESULT, PCWSTR, s, w};

const N: usize = 20;
/// Aufrufe je Platz: erst IGameInput (0..20), dann IGameInputReading (20..40). 40: Stand, 41: Fehlerwert.
#[unsafe(no_mangle)]
pub static METT_COUNT: [AtomicU32; 42] = [const { AtomicU32::new(0) }; 42];
static OLD: [AtomicUsize; 40] = [const { AtomicUsize::new(0) }; 40];
static AT: [AtomicUsize; 40] = [const { AtomicUsize::new(0) }; 40];

/// Zählt und springt weiter, ohne Register oder Stapel anzufassen.
#[unsafe(naked)]
unsafe extern "system" fn thunk<const I: usize>() {
    naked_asm!(
        "lock inc dword ptr [rip + {count} + {i} * 4]",
        "jmp qword ptr [rip + {old} + {i} * 8]",
        count = sym METT_COUNT,
        old = sym OLD,
        i = const I,
    )
}

macro_rules! thunks {
    ($($i:literal)*) => { [$(thunk::<$i> as *const () as usize),*] };
}

fn all() -> [usize; 40] {
    thunks!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39)
}

type Obj = *mut *const usize;

/// Adresse liegt in ausführbarem Code.
fn is_code(p: usize) -> bool {
    let mut m = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(p as *const _), &mut m, size_of::<MEMORY_BASIC_INFORMATION>()) };
    n != 0 && m.State == MEM_COMMIT && (m.Protect == PAGE_EXECUTE_READ || m.Protect == PAGE_EXECUTE_READWRITE)
}

unsafe fn write(at: usize, v: usize) -> bool {
    unsafe {
        let mut old = PAGE_PROTECTION_FLAGS::default();
        if VirtualProtect(at as *const _, 8, PAGE_READWRITE, &mut old).is_err() {
            return false;
        }
        (*(at as *const AtomicUsize)).store(v, SeqCst);
        let _ = VirtualProtect(at as *const _, 8, old, &mut old);
        true
    }
}

/// Plätze ab 3; die ersten drei gehören IUnknown und bleiben unberührt.
unsafe fn hook(table: *const usize, first: usize, slots: usize) {
    let thunks = all();
    for i in 3..slots {
        unsafe {
            let f = *table.add(i);
            if !is_code(f) {
                return;
            }
            let k = first + i;
            OLD[k].store(f, SeqCst);
            AT[k].store(table.add(i) as usize, SeqCst);
            if !write(table.add(i) as usize, thunks[k]) {
                AT[k].store(0, SeqCst);
                return;
            }
        }
    }
}

unsafe fn install() -> Result<(), i32> {
    unsafe {
        let dll = GetModuleHandleW(w!("GameInputRedist.dll")).map_err(|e| e.code().0)?;
        let f = GetProcAddress(dll, s!("GameInputInitialize")).ok_or(1)?;
        type Init = unsafe extern "system" fn(*const GUID, *mut Obj) -> HRESULT;
        let init = std::mem::transmute::<unsafe extern "system" fn() -> isize, Init>(f);
        // Dieselbe Schnittstelle, die das Spiel anfordert.
        let iid = GUID::from_u128(0xbbaa66d2_837a_40f7_a303_917d500955f4);
        let mut input: Obj = std::ptr::null_mut();
        init(&iid, &mut input).ok().map_err(|e| e.code().0)?;
        if input.is_null() {
            return Err(2);
        }
        let table = *input;
        // GetCurrentReading(Art Maus, kein Gerät).
        type Current = unsafe extern "system" fn(Obj, u32, *mut std::ffi::c_void, *mut Obj) -> HRESULT;
        let current = std::mem::transmute::<usize, Current>(*table.add(4));
        let mut reading: Obj = std::ptr::null_mut();
        let r = current(input, 0x20, std::ptr::null_mut(), &mut reading);
        METT_COUNT[41].store(r.0 as u32, SeqCst);
        // Die letzten Plätze der Tabelle sind unbekannt; 17 gibt es sicher (SetFocusPolicy ist Platz 16).
        hook(table, 0, 17);
        if r.is_ok() && !reading.is_null() {
            let t = *reading;
            type Release = unsafe extern "system" fn(Obj) -> u32;
            std::mem::transmute::<usize, Release>(*t.add(2))(reading);
            hook(t, N, 18);
        }
        // Das eigene IGameInput bleibt bis zum Spielende bestehen; es hält die DLL fest.
        Ok(())
    }
}

/// # Safety
/// Wird nur vom Kern mit gültigen Zeigern aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_query(host: *const Host, info: *mut Info) -> i32 {
    unsafe {
        *info = Info { size: size_of::<Info>() as u32, api: API, version: 1 };
        if (*host).api == API { 0 } else { 1 }
    }
}

/// # Safety
/// Wird nur vom Kern mit gültigem Zeiger aufgerufen.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn mett_plugin_load(_: *const Host) -> i32 {
    match unsafe { install() } {
        Ok(()) => {
            METT_COUNT[40].store(1, SeqCst);
            0
        }
        Err(e) => {
            METT_COUNT[41].store(e as u32, SeqCst);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn mett_plugin_unload() {
    let thunks = all();
    unsafe {
        for (at, (old, mine)) in AT.iter().zip(OLD.iter().zip(thunks)) {
            let at = at.swap(0, SeqCst);
            if at != 0 && *(at as *const usize) == mine {
                write(at, old.load(SeqCst));
            }
        }
        // Ein Aufruf könnte gerade im Sprungstück sein: zur Sicherheit geladen lassen.
        let mut pinned = HMODULE::default();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
        let _ = GetModuleHandleExW(flags, PCWSTR(mett_plugin_unload as *const () as *const u16), &mut pinned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn add(a: u64, b: u64, c: u64, d: u64, e: u64, f: f64) -> u64 {
        a + b * 2 + c * 3 + d * 4 + e * 5 + f as u64
    }

    #[test]
    fn zaehlt_und_reicht_alles_weiter() {
        // Eigene Seite, damit das Umstellen der Rechte nichts anderes trifft.
        let table = unsafe { VirtualAlloc(None, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) } as *mut usize;
        let plain = add as *const () as usize;
        unsafe {
            (0..6).for_each(|i| table.add(i).write(plain));
            hook(table, 0, 6);
            let f = std::mem::transmute::<usize, unsafe extern "system" fn(u64, u64, u64, u64, u64, f64) -> u64>(
                std::ptr::read_volatile(table.add(4)),
            );
            for _ in 0..3 {
                assert_eq!(f(1, 2, 3, 4, 5, 6.0), 1 + 4 + 9 + 16 + 25 + 6);
            }
        }
        let n = |i: usize| METT_COUNT[i].load(SeqCst);
        assert_eq!((n(2), n(3), n(4), n(5)), (0, 0, 3, 0));
        let read = |i: usize| unsafe { std::ptr::read_volatile(table.add(i)) };
        assert_eq!(read(2), plain);
        assert_ne!(read(4), plain);
        mett_plugin_unload();
        assert!((0..6).all(|i| read(i) == plain));
    }
}
