//! Lädt die eingeschalteten Plugins. Erst alle fragen, dann die passenden laden; eines kann das andere nicht verhindern.
use mett_plugin::{API, Host, Info, Item, Load, MAGIC, MAX_PLUGINS, NAME_LEN, Query, Status, Unload, state};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::{HSTRING, PCWSTR, Result, s};

struct Plugin {
    name: String,
    module: usize,
    unload: Unload,
}

struct Running {
    plugins: Vec<Plugin>,
    status: Status,
    map: usize,
    view: usize,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// `enabled`: Namen, durch `;` getrennt. Schon laufende bleiben, fehlende kommen dazu. Liefert die Anzahl geladener.
pub fn start(enabled: &str) -> Result<u32> {
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    let dir = own_dir()?.join("plugins");
    let host = host()?;
    let r = running.get_or_insert_with(|| {
        let status =
            Status { magic: MAGIC, api: API, count: 0, items: [Item { name: [0; NAME_LEN], state: 0 }; MAX_PLUGINS] };
        let (map, view) = publish(&status).unwrap_or((0, 0));
        Running { plugins: vec![], status, map, view }
    });
    let new: Vec<&str> =
        enabled.split(';').filter(|n| !n.is_empty() && r.plugins.iter().all(|p| p.name != *n)).collect();
    let asked: Vec<_> = new.into_iter().map(|n| (n, ask(&dir, n, &host))).collect();
    for (name, answer) in asked {
        let state = match answer {
            Err(s) => s,
            Ok((module, load, unload)) => {
                if unsafe { load(&host) } == 0 {
                    r.plugins.push(Plugin { name: name.into(), module: module.0 as usize, unload });
                    state::LOADED
                } else {
                    unsafe {
                        let _ = FreeLibrary(module);
                    }
                    state::LOAD_FAILED
                }
            }
        };
        // Ein neuer Versuch ersetzt den alten Stand.
        let text = |i: &Item| i.name.iter().take_while(|b| **b != 0).copied().eq(name.bytes().take(NAME_LEN - 1));
        let count = r.status.count as usize;
        let at = r.status.items[..count].iter().position(text).unwrap_or(count);
        if at < MAX_PLUGINS {
            let item = &mut r.status.items[at];
            item.state = state;
            item.name.iter_mut().zip(name.bytes().take(NAME_LEN - 1)).for_each(|(a, b)| *a = b);
            r.status.count = r.status.count.max(at as u32 + 1);
        }
    }
    if r.view != 0 {
        unsafe { (r.view as *mut Status).write(r.status) };
    }
    Ok(r.plugins.len() as u32)
}

/// In umgekehrter Reihenfolge zurücknehmen.
pub fn stop() {
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(r) = running.take() else { return };
    unsafe {
        for p in r.plugins.into_iter().rev() {
            (p.unload)();
            let _ = FreeLibrary(HMODULE(p.module as *mut _));
        }
        if r.view != 0 {
            let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: r.view as *mut _ });
            let _ = CloseHandle(HANDLE(r.map as *mut _));
        }
    }
}

type Entry = (HMODULE, Load, Unload);

fn ask(dir: &Path, name: &str, host: &Host) -> std::result::Result<Entry, u32> {
    if !mett_plugin::valid_name(name) {
        return Err(state::BAD_NAME);
    }
    let path = HSTRING::from(dir.join(format!("{name}.dll")).as_path());
    unsafe {
        // Abhängigkeiten nur aus dem Plugin-Ordner und Windows.
        let flags = LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32;
        let module = LoadLibraryExW(&path, None, flags).map_err(|_| state::BAD_DLL)?;
        let result = (|| {
            let get = |n| GetProcAddress(module, n).ok_or(state::NO_EXPORTS);
            let query: Query = std::mem::transmute(get(s!("mett_plugin_query"))?);
            let load: Load = std::mem::transmute(get(s!("mett_plugin_load"))?);
            let unload: Unload = std::mem::transmute(get(s!("mett_plugin_unload"))?);
            let mut info = Info::default();
            let answer = query(host, &mut info);
            mett_plugin::judge(answer, &info)?;
            Ok((module, load, unload))
        })();
        if result.is_err() {
            let _ = FreeLibrary(module);
        }
        result
    }
}

fn module_path(module: Option<HMODULE>) -> Result<PathBuf> {
    let mut b = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(module, &mut b) } as usize;
    if n == 0 || n >= b.len() {
        return Err(ERROR_FILE_NOT_FOUND.into());
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&b[..n])))
}

fn own_dir() -> Result<PathBuf> {
    let mut me = HMODULE::default();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    unsafe { GetModuleHandleExW(flags, PCWSTR(own_dir as *const () as *const u16), &mut me)? };
    let path = module_path(Some(me))?;
    path.parent().map(Path::to_path_buf).ok_or_else(|| ERROR_PATH_NOT_FOUND.into())
}

fn host() -> Result<Host> {
    unsafe {
        let base = GetModuleHandleW(None)?.0 as usize;
        // Bildgröße aus dem PE-Kopf.
        let pe = base + (*((base + 0x3c) as *const u32)) as usize;
        let image_size = (*((pe + 0x50) as *const u32)) as usize;
        Ok(Host {
            size: size_of::<Host>() as u32,
            api: API,
            game: game_version().unwrap_or_default(),
            base,
            image_size,
        })
    }
}

fn game_version() -> Result<[u16; 4]> {
    let path = HSTRING::from(module_path(None)?.as_path());
    unsafe {
        let size = GetFileVersionInfoSizeW(&path, None);
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&path, None, size, data.as_mut_ptr().cast())?;
        let (mut p, mut n) = (std::ptr::null_mut(), 0);
        VerQueryValueW(data.as_ptr().cast(), windows::core::w!("\\"), &mut p, &mut n).ok()?;
        let v = &*p.cast::<VS_FIXEDFILEINFO>();
        let w = |x: u32| [(x >> 16) as u16, x as u16];
        let (a, b) = (w(v.dwFileVersionMS), w(v.dwFileVersionLS));
        Ok([a[0], a[1], b[0], b[1]])
    }
}

fn publish(status: &Status) -> Result<(usize, usize)> {
    unsafe {
        let name = HSTRING::from(mett_plugin::map_name(GetCurrentProcessId()));
        let size = size_of::<Status>();
        let map = CreateFileMappingW(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, size as u32, &name)?;
        let view = MapViewOfFile(map, FILE_MAP_WRITE, 0, 0, size);
        if view.Value.is_null() {
            let e = windows::core::Error::from_thread();
            let _ = CloseHandle(map);
            return Err(e);
        }
        view.Value.cast::<Status>().write(*status);
        Ok((map.0 as usize, view.Value as usize))
    }
}
