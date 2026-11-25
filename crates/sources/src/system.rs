//! CPU, Arbeitsspeicher, Grafikkarte, Akku. Systemweit, ohne Zugriff aufs Spiel.
use std::ffi::c_void;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Power::*;
use windows::Win32::System::SystemInformation::*;
use windows::Win32::System::Threading::GetSystemTimes;
use windows::core::{s, w};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct System {
    pub cpu: u8,
    pub ram: u8,
    /// Nur mit NVIDIA-Treiber.
    pub gpu: Option<u8>,
}

pub struct Sampler {
    last: Option<(u64, u64)>,
    gpu: Option<Nvml>,
}

impl Sampler {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { last: None, gpu: Nvml::load() }
    }

    /// Der erste Aufruf liefert nichts, weil CPU-Last einen Vergleichswert braucht.
    pub fn sample(&mut self) -> Option<System> {
        let now = cpu_times()?;
        let before = self.last.replace(now)?;
        let cpu = cpu_load(before, now)?;
        let mut m = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
        Some(System { cpu, ram: m.dwMemoryLoad.min(100) as u8, gpu: self.gpu.as_ref().and_then(Nvml::load_percent) })
    }
}

/// (Leerlauf, Gesamt)
fn cpu_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    let n = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    // Die Kernelzeit enthält den Leerlauf schon.
    Some((n(idle), n(kernel) + n(user)))
}

fn cpu_load(a: (u64, u64), b: (u64, u64)) -> Option<u8> {
    let total = b.1.checked_sub(a.1).filter(|t| *t > 0)?;
    let idle = b.0.saturating_sub(a.0).min(total);
    Some(((total - idle) * 100 / total) as u8)
}

/// Ladung in Prozent, `None` ohne Akku.
pub fn battery() -> Option<u8> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s) }.ok()?;
    // 128: kein Akku, 255: unbekannt.
    (s.BatteryFlag & 128 == 0 && s.BatteryLifePercent <= 100).then_some(s.BatteryLifePercent)
}

type Init = unsafe extern "C" fn() -> i32;
type ByIndex = unsafe extern "C" fn(u32, *mut *mut c_void) -> i32;
type Utilization = unsafe extern "C" fn(*mut c_void, *mut [u32; 2]) -> i32;

/// NVIDIAs Messbibliothek, liegt beim Treiber.
struct Nvml {
    device: *mut c_void,
    utilization: Utilization,
}

impl Nvml {
    fn load() -> Option<Self> {
        unsafe {
            // Nur aus dem Windows-Ordner laden.
            let lib = LoadLibraryExW(w!("nvml.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
            let init: Init = std::mem::transmute(GetProcAddress(lib, s!("nvmlInit_v2"))?);
            let by_index: ByIndex = std::mem::transmute(GetProcAddress(lib, s!("nvmlDeviceGetHandleByIndex_v2"))?);
            let utilization: Utilization =
                std::mem::transmute(GetProcAddress(lib, s!("nvmlDeviceGetUtilizationRates"))?);
            let mut device = std::ptr::null_mut();
            (init() == 0 && by_index(0, &mut device) == 0).then_some(Self { device, utilization })
        }
    }

    fn load_percent(&self) -> Option<u8> {
        let mut v = [0; 2];
        (unsafe { (self.utilization)(self.device, &mut v) } == 0).then_some(v[0].min(100) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_last() {
        assert_eq!(cpu_load((100, 1000), (150, 1200)), Some(75));
        assert_eq!(cpu_load((100, 1000), (100, 1000)), None);
        assert_eq!(cpu_load((0, 0), (500, 100)), Some(0));
    }

    #[test]
    fn echte_messung_ist_plausibel() {
        let mut s = Sampler::new();
        assert_eq!(s.sample(), None);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let v = s.sample().unwrap();
        assert!(v.cpu <= 100 && v.ram > 0 && v.ram <= 100);
    }
}
