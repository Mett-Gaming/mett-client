//! Einziger Ort mit Zugriff auf fremden Prozessspeicher. Liest nur.
use windows::Win32::Foundation::{CloseHandle, ERROR_PARTIAL_COPY, HANDLE};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;
use windows::core::Result;

pub mod attributes;
pub mod chat;
pub mod commands;
pub mod model;
pub mod options;
pub mod players;
pub mod server;
pub mod sheets;
pub mod uisizes;
pub mod uitree;

/// Alle Prozesse mit diesem Dateinamen.
pub fn find_processes(exe: &str) -> Result<Vec<u32>> {
    let mut found = vec![];
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            if String::from_utf16_lossy(&e.szExeFile[..len]).eq_ignore_ascii_case(exe) {
                found.push(e.th32ProcessID);
            }
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    Ok(found)
}

/// Adressbereich des Programms.
pub fn image_range(pid: u32) -> Result<std::ops::Range<u64>> {
    let m = first_module(pid)?;
    Ok(m.modBaseAddr as u64..m.modBaseAddr as u64 + m.modBaseSize as u64)
}

fn first_module(pid: u32) -> Result<MODULEENTRY32W> {
    let mut m = MODULEENTRY32W { dwSize: size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid)?;
        // Das erste Modul ist das Programm selbst.
        let first = Module32FirstW(snap, &mut m);
        let _ = CloseHandle(snap);
        first?;
    }
    Ok(m)
}

/// Abbild des Programms, wie es im Speicher liegt, mit Anfangsadresse.
pub fn image(pid: u32) -> Result<(u64, Vec<u8>)> {
    let m = first_module(pid)?;
    let reader = Reader::open(pid)?;
    let (base, mut bytes) = (m.modBaseAddr as usize, vec![0u8; m.modBaseSize as usize]);
    // Einzelne Seiten dürfen fehlen.
    for (i, page) in bytes.chunks_mut(4096).enumerate() {
        let _ = reader.read(base + i * 4096, page);
    }
    Ok((base as u64, bytes))
}

pub struct Reader(HANDLE);

impl Reader {
    pub fn open(pid: u32) -> Result<Self> {
        unsafe { OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map(Self) }
    }

    /// Zusätzlich mit Auskunft über Speicherbereiche, weiterhin nur lesend.
    pub fn open_for_scan(pid: u32) -> Result<Self> {
        unsafe { OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, false, pid).map(Self) }
    }

    /// Private, beschreibbare Bereiche (dort liegen Spieldaten), höchstens 4 MB je Stück.
    pub fn writable_regions(&self) -> Vec<(usize, usize)> {
        let mut found = vec![];
        let mut at = 0usize;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let size = size_of::<MEMORY_BASIC_INFORMATION>();
        while unsafe { VirtualQueryEx(self.0, Some(at as *const _), &mut info, size) } == size {
            let start = info.BaseAddress as usize;
            if info.State == MEM_COMMIT && info.Type == MEM_PRIVATE && info.Protect == PAGE_READWRITE {
                let step = 4 << 20;
                found.extend((0..info.RegionSize).step_by(step).map(|o| (start + o, step.min(info.RegionSize - o))));
            }
            match start.checked_add(info.RegionSize) {
                Some(next) if next > at => at = next,
                _ => break,
            }
        }
        found
    }

    pub fn read(&self, address: usize, buf: &mut [u8]) -> Result<()> {
        let mut n = 0;
        unsafe {
            ReadProcessMemory(self.0, address as *const _, buf.as_mut_ptr().cast(), buf.len(), Some(&mut n))?;
        }
        if n == buf.len() { Ok(()) } else { Err(ERROR_PARTIAL_COPY.into()) }
    }
}

// Nur lesende Aufrufe auf einem Prozess-Handle; Windows erlaubt sie aus mehreren Threads.
unsafe impl Send for Reader {}
unsafe impl Sync for Reader {}

impl Reader {
    /// Geht alle beschreibbaren Bereiche mit mehreren Threads durch. `f` bekommt Adresse und Inhalt.
    pub fn scan<T: Send>(&self, f: &(dyn Fn(usize, &[u8]) -> Vec<T> + Sync)) -> Vec<T> {
        // Dem Spiel bleiben genug Kerne.
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        let threads = (cores / 4).clamp(1, 4);
        let regions = self.writable_regions();
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|t| {
            let work = || {
                // Das Spiel geht vor.
                unsafe {
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
                }
                let (mut out, mut buf) = (vec![], vec![]);
                while let Some((at, size)) = regions.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                    buf.resize(*size, 0);
                    if self.read(*at, &mut buf).is_ok() {
                        out.extend(f(*at, &buf));
                    }
                }
                out
            };
            let handles: Vec<_> = (0..threads).map(|_| t.spawn(work)).collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        })
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liest_bekannten_wert() {
        let value = std::hint::black_box(0x4d45_5454_u32);
        let mut b = [0; 4];
        Reader::open(std::process::id()).unwrap().read(&raw const value as usize, &mut b).unwrap();
        assert_eq!(u32::from_ne_bytes(b), value);
    }

    #[test]
    fn findet_eigenen_speicher() {
        let v = std::hint::black_box(vec![7u8; 4096]);
        let r = Reader::open_for_scan(std::process::id()).unwrap();
        let at = v.as_ptr() as usize;
        assert!(r.writable_regions().iter().any(|(s, n)| (*s..s + n).contains(&at)));
    }

    #[test]
    fn suche_in_mehreren_threads_findet_alles_einmal() {
        // Eigener Bereich, den sonst niemand verändert.
        let page = unsafe { VirtualAlloc(None, 8 << 20, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
        let mark = 0x4D45_5454_5343_414Eu64 ^ std::process::id() as u64;
        let spots = [0usize, 4 << 20, (8 << 20) - 8].map(|o| page as usize + o);
        spots.iter().for_each(|s| unsafe { (*s as *mut u64).write_volatile(mark) });
        let r = Reader::open_for_scan(std::process::id()).unwrap();
        let hits = r.scan(&|at, b| {
            let words = b.as_chunks::<8>().0.iter().enumerate();
            words.filter(|(_, w)| u64::from_le_bytes(**w) == mark).map(|(i, _)| at + i * 8).collect()
        });
        assert!(spots.iter().all(|s| hits.iter().filter(|h| *h == s).count() == 1));
    }

    #[test]
    fn ungueltige_adresse_ist_fehler() {
        assert!(Reader::open(std::process::id()).unwrap().read(8, &mut [0; 4]).is_err());
    }
}
