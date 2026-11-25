//! Nur Entwicklung, eigener Prozess: Wie viele Bytes schreibt GameInput als Mauswert, und wo ändert sich etwas?
use windows::Win32::System::LibraryLoader::*;
use windows::core::{GUID, HRESULT, s, w};

type Obj = *mut *const usize;

fn main() {
    unsafe {
        let dll = LoadLibraryW(w!("GameInputRedist.dll")).expect("GameInputRedist");
        let f = GetProcAddress(dll, s!("GameInputInitialize")).expect("Export");
        type Init = unsafe extern "system" fn(*const GUID, *mut Obj) -> HRESULT;
        let init = std::mem::transmute::<unsafe extern "system" fn() -> isize, Init>(f);
        let iid = GUID::from_u128(0xbbaa66d2_837a_40f7_a303_917d500955f4);
        let mut input: Obj = std::ptr::null_mut();
        println!("Init {:?}", init(&iid, &mut input));
        type Current = unsafe extern "system" fn(Obj, u32, *mut std::ffi::c_void, *mut Obj) -> HRESULT;
        let current = std::mem::transmute::<usize, Current>(*(*input).add(4));
        let seconds = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(2u32);
        let mut last = [0i64; 16];
        for i in 0..seconds * 20 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let mut r: Obj = std::ptr::null_mut();
            let hr = current(input, 0x20, std::ptr::null_mut(), &mut r);
            if hr.is_err() || r.is_null() {
                if i == 0 {
                    println!("Kein Messwert: {hr:?}");
                }
                continue;
            }
            type State = unsafe extern "system" fn(Obj, *mut u8) -> bool;
            let get = std::mem::transmute::<usize, State>(*(*r).add(14));
            let (mut a, mut b) = ([0x11u8; 128], [0xEEu8; 128]);
            let ok = get(r, a.as_mut_ptr());
            get(r, b.as_mut_ptr());
            let written = (0..128).filter(|k| a[*k] == b[*k]).count();
            let words: [i64; 16] = std::array::from_fn(|k| i64::from_le_bytes(a[k * 8..k * 8 + 8].try_into().unwrap()));
            if words != last {
                println!("ok {ok}, {written} Bytes geschrieben: {:?}", &words[..written / 8]);
                last = words;
            }
            type Release = unsafe extern "system" fn(Obj) -> u32;
            std::mem::transmute::<usize, Release>(*(*r).add(2))(r);
        }
    }
}
