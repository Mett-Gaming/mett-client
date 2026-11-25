//! Loader nach dem Vorbild von SKSE: bringt den Kern ins Spiel, der dann einzelne Plugins lädt.
//! Üblicher Weg (`LoadLibrary`), keine Tarnung. Wird der Zugriff verweigert, ist das ein Fehler und kein Anlass für Umwege.
use mett_plugin::Status;
use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::Win32::Foundation::*;
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Diagnostics::ToolHelp::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;
use windows::core::{Error, HSTRING, PCSTR, Result, s, w};

const TIMEOUT_MS: u32 = 5000;

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open(pid: u32) -> Result<Handle> {
    let access = PROCESS_CREATE_THREAD
        | PROCESS_QUERY_INFORMATION
        | PROCESS_SYNCHRONIZE
        | PROCESS_VM_OPERATION
        | PROCESS_VM_READ
        | PROCESS_VM_WRITE;
    unsafe { OpenProcess(access, false, pid).map(Handle) }
}

/// Adresse des Moduls im Zielprozess, falls geladen.
pub fn find(pid: u32, dll: &Path) -> Result<Option<usize>> {
    let name = dll.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    unsafe {
        let snap = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid)?);
        let mut m = MODULEENTRY32W { dwSize: size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
        let mut ok = Module32FirstW(snap.0, &mut m).is_ok();
        while ok {
            let len = m.szModule.iter().position(|c| *c == 0).unwrap_or(m.szModule.len());
            if String::from_utf16_lossy(&m.szModule[..len]).to_lowercase() == name {
                return Ok(Some(m.modBaseAddr as usize));
            }
            ok = Module32NextW(snap.0, &mut m).is_ok();
        }
    }
    Ok(None)
}

pub fn load(pid: u32, dll: &Path) -> Result<()> {
    // Erst prüfen, dann schreiben: nie doppelt laden.
    if find(pid, dll)?.is_some() {
        return Ok(());
    }
    let full = std::path::absolute(dll).map_err(|_| windows::core::Error::from(ERROR_FILE_NOT_FOUND))?;
    if !full.is_file() {
        return Err(ERROR_FILE_NOT_FOUND.into());
    }
    call_with_text(&open(pid)?, kernel(s!("LoadLibraryW"))?, full.as_os_str())?;
    if find(pid, dll)?.is_some() { Ok(()) } else { Err(ERROR_MOD_NOT_FOUND.into()) }
}

pub fn unload(pid: u32, dll: &Path) -> Result<()> {
    let Some(base) = find(pid, dll)? else { return Ok(()) };
    call(&open(pid)?, kernel(s!("FreeLibrary"))?, base as *mut c_void)?;
    if find(pid, dll)?.is_none() { Ok(()) } else { Err(ERROR_BUSY.into()) }
}

/// Lädt den Kern und startet die genannten Plugins. Liefert die Anzahl der geladenen.
pub fn start(pid: u32, core: &Path, plugins: &[&str]) -> Result<u32> {
    load(pid, core)?;
    let f = remote_export(pid, core, s!("mett_core_start"))?;
    match call_with_text(&open(pid)?, f, plugins.join(";").as_ref())? {
        u32::MAX => Err(ERROR_FUNCTION_FAILED.into()),
        n => Ok(n),
    }
}

/// Warum ein Ladeversuch scheitert: sieht der Prozess die Datei, und welchen Fehler meldet Windows dabei?
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnosis {
    /// Dateieigenschaften aus Sicht des Prozesses, `None` wenn er die Datei nicht sieht.
    pub attributes: Option<u32>,
    pub loaded: bool,
    /// Windows-Fehlerwerte des Lade-Threads in ihrer Reihenfolge. Spätere können schon vom Thread-Ende stammen.
    pub errors: Vec<u32>,
}

/// Genau ein Ladeversuch; ein geladenes Modul wird gleich wieder entfernt.
pub fn diagnose(pid: u32, dll: &Path) -> Result<Diagnosis> {
    let process = open(pid)?;
    let path = dll.as_os_str();
    let seen = call_with_text(&process, kernel(s!("GetFileAttributesW"))?, path)?;
    let text: Vec<u16> = path.encode_wide().chain([0]).collect();
    let bytes = text.len() * 2;
    let mut errors = vec![];
    unsafe {
        let at = VirtualAllocEx(process.0, None, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if at.is_null() {
            return Err(Error::from_thread());
        }
        WriteProcessMemory(process.0, at, text.as_ptr().cast(), bytes, None)?;
        let f =
            std::mem::transmute::<usize, unsafe extern "system" fn(*mut c_void) -> u32>(kernel(s!("LoadLibraryW"))?);
        let start = CREATE_SUSPENDED.0;
        let thread = Handle(CreateRemoteThread(process.0, None, 0, Some(f), Some(at), start, None)?);
        let slot = teb(&thread)? + 0x68;
        ResumeThread(thread.0);
        // Der Fehlerwert steht nur, solange der Thread lebt.
        let begin = std::time::Instant::now();
        while WaitForSingleObject(thread.0, 0) == WAIT_TIMEOUT && begin.elapsed().as_millis() < TIMEOUT_MS as u128 {
            let mut v = 0u32;
            let to = (&raw mut v).cast();
            if ReadProcessMemory(process.0, slot as *const c_void, to, 4, None).is_ok()
                && v != 0
                && errors.last() != Some(&v)
            {
                errors.push(v);
            }
        }
        if WaitForSingleObject(thread.0, 0) == WAIT_OBJECT_0 {
            let _ = VirtualFreeEx(process.0, at, 0, MEM_RELEASE);
        }
    }
    let loaded = find(pid, dll)?.is_some();
    unload(pid, dll)?;
    Ok(Diagnosis { attributes: (seen != u32::MAX).then_some(seen), loaded, errors })
}

/// Lage des Thread-Datenblocks, in dem Windows den letzten Fehler ablegt.
fn teb(thread: &Handle) -> Result<usize> {
    #[repr(C)]
    #[derive(Default)]
    struct Basic {
        exit: usize,
        teb: usize,
        rest: [usize; 4],
    }
    type Query = unsafe extern "system" fn(HANDLE, u32, *mut Basic, u32, *mut u32) -> i32;
    unsafe {
        let nt = GetModuleHandleW(w!("ntdll.dll"))?;
        let f = GetProcAddress(nt, s!("NtQueryInformationThread")).ok_or_else(Error::from_thread)?;
        let query = std::mem::transmute::<unsafe extern "system" fn() -> isize, Query>(f);
        let mut b = Basic::default();
        if query(thread.0, 0, &mut b, size_of::<Basic>() as u32, std::ptr::null_mut()) < 0 {
            return Err(ERROR_FUNCTION_FAILED.into());
        }
        Ok(b.teb)
    }
}

/// Nimmt alle Plugins zurück und entfernt den Kern.
pub fn stop(pid: u32, core: &Path) -> Result<()> {
    if find(pid, core)?.is_none() {
        return Ok(());
    }
    let f = remote_export(pid, core, s!("mett_core_stop"))?;
    // Scheitert das Zurücknehmen, bleibt der Kern lieber geladen, als Code unter laufenden Plugins wegzuziehen.
    if call(&open(pid)?, f, std::ptr::null_mut())? != 0 {
        return Err(ERROR_FUNCTION_FAILED.into());
    }
    unload(pid, core)
}

/// Stand jedes Plugins: Name und Zustand.
pub fn status(pid: u32) -> Result<Vec<(String, u32)>> {
    unsafe {
        let name = HSTRING::from(mett_plugin::map_name(pid));
        let map = Handle(OpenFileMappingW(FILE_MAP_READ.0, false, &name)?);
        let view = MapViewOfFile(map.0, FILE_MAP_READ, 0, 0, size_of::<Status>());
        if view.Value.is_null() {
            return Err(Error::from_thread());
        }
        let s = view.Value.cast::<Status>().read();
        let _ = UnmapViewOfFile(view);
        if s.magic != mett_plugin::MAGIC || s.api != mett_plugin::API {
            return Err(ERROR_INVALID_DATA.into());
        }
        let items = s.items.iter().take((s.count as usize).min(mett_plugin::MAX_PLUGINS));
        let text = |n: &[u8]| String::from_utf8_lossy(n.split(|b| *b == 0).next().unwrap_or_default()).into_owned();
        Ok(items.map(|i| (text(&i.name), i.state)).collect())
    }
}

/// kernel32 liegt in allen Prozessen an derselben Adresse.
fn kernel(name: PCSTR) -> Result<usize> {
    unsafe {
        let k = GetModuleHandleW(w!("kernel32.dll"))?;
        GetProcAddress(k, name).map(|f| f as usize).ok_or_else(Error::from_thread)
    }
}

/// Adresse einer Funktion des Kerns im Zielprozess: gleicher Abstand zum Modulanfang wie bei uns.
fn remote_export(pid: u32, dll: &Path, name: PCSTR) -> Result<usize> {
    let base = find(pid, dll)?.ok_or(Error::from(ERROR_MOD_NOT_FOUND))?;
    unsafe {
        let here = LoadLibraryExW(&HSTRING::from(dll), None, DONT_RESOLVE_DLL_REFERENCES)?;
        let at = GetProcAddress(here, name).map(|f| f as usize - here.0 as usize).ok_or_else(Error::from_thread);
        let _ = FreeLibrary(here);
        Ok(base + at?)
    }
}

/// Liest Daten, die ein geladenes Modul unter einem Namen veröffentlicht.
pub fn read_export(pid: u32, dll: &Path, name: PCSTR, out: &mut [u8]) -> Result<()> {
    let at = remote_export(pid, dll, name)?;
    unsafe { ReadProcessMemory(open(pid)?.0, at as *const c_void, out.as_mut_ptr().cast(), out.len(), None) }
}

/// Ruft eine veröffentlichte Funktion eines eigenen Plugins mit einem Text auf.
pub fn call_export_text(pid: u32, dll: &Path, name: PCSTR, text: &str) -> Result<u32> {
    call_with_text(&open(pid)?, remote_export(pid, dll, name)?, text.as_ref())
}

/// Ruft eine veröffentlichte Funktion eines eigenen Plugins auf (nur Entwicklung).
pub fn call_export(pid: u32, dll: &Path, name: PCSTR) -> Result<u32> {
    call(&open(pid)?, remote_export(pid, dll, name)?, std::ptr::null_mut())
}

/// Schreibt in eine veröffentlichte Variable eines eigenen Plugins (nur Entwicklung).
pub fn write_export(pid: u32, dll: &Path, name: PCSTR, data: &[u8]) -> Result<()> {
    let at = remote_export(pid, dll, name)?;
    unsafe { WriteProcessMemory(open(pid)?.0, at as *const c_void, data.as_ptr().cast(), data.len(), None) }
}

/// Aufruf mit einem Text, der dafür kurz im Zielprozess liegt.
fn call_with_text(process: &Handle, f: usize, text: &OsStr) -> Result<u32> {
    let text: Vec<u16> = text.encode_wide().chain([0]).collect();
    let bytes = text.len() * 2;
    unsafe {
        let at = VirtualAllocEx(process.0, None, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if at.is_null() {
            return Err(Error::from_thread());
        }
        let done =
            WriteProcessMemory(process.0, at, text.as_ptr().cast(), bytes, None).and_then(|_| call(process, f, at));
        // Läuft der Aufruf noch, braucht er den Speicher weiter; dann lieber stehen lassen.
        if !matches!(&done, Err(e) if e.code() == ERROR_TIMEOUT.to_hresult()) {
            let _ = VirtualFreeEx(process.0, at, 0, MEM_RELEASE);
        }
        done
    }
}

fn call(process: &Handle, f: usize, argument: *mut c_void) -> Result<u32> {
    unsafe {
        let f = std::mem::transmute::<usize, unsafe extern "system" fn(*mut c_void) -> u32>(f);
        let thread = Handle(CreateRemoteThread(process.0, None, 0, Some(f), Some(argument), 0, None)?);
        if WaitForSingleObject(thread.0, TIMEOUT_MS) != WAIT_OBJECT_0 {
            return Err(Error::from(ERROR_TIMEOUT.to_hresult()));
        }
        let mut code = 0;
        GetExitCodeThread(thread.0, &mut code)?;
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

    fn target() -> Child {
        let c = Command::new("ping.exe").args(["-n", "30", "127.0.0.1"]).stdout(Stdio::null()).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        c
    }

    #[test]
    fn laden_und_entladen_laesst_den_prozess_leben() {
        // Eine Windows-DLL, die ping.exe selbst nicht benutzt.
        let dll = Path::new(r"C:\Windows\System32\avifil32.dll");
        let mut c = target();
        let pid = c.id();
        let before = find(pid, dll).unwrap();
        let loaded = load(pid, dll).and_then(|_| load(pid, dll));
        let there = find(pid, dll).unwrap();
        let gone = unload(pid, dll);
        let alive = c.try_wait().unwrap().is_none();
        c.kill().unwrap();
        c.wait().unwrap();
        assert!(before.is_none() && there.is_some() && alive, "{loaded:?}");
        assert!(loaded.is_ok() && gone.is_ok(), "{loaded:?} {gone:?}");
    }

    #[test]
    fn fehlende_datei_wird_gar_nicht_erst_geladen() {
        let mut c = target();
        let r = load(c.id(), Path::new(r"C:\gibt-es-nicht\mett.dll"));
        let alive = c.try_wait().unwrap().is_none();
        c.kill().unwrap();
        c.wait().unwrap();
        assert!(r.is_err() && alive);
    }
}
