//! Minecraft Bedrock finden, starten und sein Ende erkennen. Hängt nur an Programm- und Paketname.
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
use windows::core::{PCWSTR, PWSTR, Result, w};

pub const EXE: &str = "Minecraft.Windows.exe";
const FAMILY: PCWSTR = w!("Microsoft.MinecraftUWP_8wekyb3d8bbwe");
const AUMID: PCWSTR = w!("Microsoft.MinecraftUWP_8wekyb3d8bbwe!Game");
const START_TIMEOUT: Duration = Duration::from_secs(120);

pub fn is_installed() -> bool {
    let (mut count, mut len) = (0, 0);
    unsafe {
        let r = GetPackagesByPackageFamily(FAMILY, &mut count, None, &mut len, None);
        r == ERROR_INSUFFICIENT_BUFFER && count > 0
    }
}

/// Startet über die App-Kennung. Minecraft wird so nicht unser Kindprozess.
pub fn launch() -> Result<()> {
    unsafe {
        let m: IApplicationActivationManager =
            CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)?;
        m.ActivateApplication(AUMID, PCWSTR::null(), AO_NONE).map(|_| ())
    }
}

#[derive(Debug, PartialEq)]
pub enum Status {
    Waiting,
    Running,
    Exited,
    TimedOut,
}

pub struct Watcher {
    exe: String,
    since: Instant,
    last_scan: Option<Instant>,
    process: Option<HANDLE>,
    pid: Option<u32>,
    limited: bool,
}

impl Watcher {
    /// `limited`: nach einer Weile aufgeben, wenn das Spiel nicht auftaucht.
    pub fn new(exe: String, limited: bool) -> Self {
        Self { exe, since: Instant::now(), last_scan: None, process: None, pid: None, limited }
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn poll(&mut self, now: Instant) -> Status {
        if let Some(h) = self.process {
            let done = unsafe { WaitForSingleObject(h, 0) } == WAIT_OBJECT_0;
            return if done { Status::Exited } else { Status::Running };
        }
        if self.last_scan.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(1)) {
            self.last_scan = Some(now);
            for pid in gamemem::find_processes(&self.exe).unwrap_or_default() {
                if let Ok(h) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
                    self.process = Some(h);
                    self.pid = Some(pid);
                    return Status::Running;
                }
            }
        }
        if self.limited && now.duration_since(self.since) > START_TIMEOUT {
            return Status::TimedOut;
        }
        Status::Waiting
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(h) = self.process {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

fn window_path(hwnd: HWND) -> Option<PathBuf> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(p);
        r.ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// Gehört das Fenster zu einem Programm mit diesem Dateinamen?
pub fn window_is(hwnd: HWND, exe: &str) -> bool {
    window_path(hwnd).and_then(|p| p.file_name().map(|n| n.eq_ignore_ascii_case(exe))).unwrap_or(false)
}

/// Ordner des Programms, dem das Fenster gehört.
pub fn directory(hwnd: HWND) -> Option<PathBuf> {
    window_path(hwnd)?.parent().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erkennt_start_und_ende() {
        let mut child = std::process::Command::new("ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut w = Watcher::new("PING.EXE".into(), true);
        let found = w.poll(Instant::now());
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(found, Status::Running);
        assert_eq!(w.poll(Instant::now()), Status::Exited);
    }

    #[test]
    fn zeitlimit_nur_wenn_gewuenscht() {
        let later = Instant::now() + START_TIMEOUT + Duration::from_secs(1);
        assert_eq!(Watcher::new("gibt-es-nicht.exe".into(), true).poll(later), Status::TimedOut);
        assert_eq!(Watcher::new("gibt-es-nicht.exe".into(), false).poll(later), Status::Waiting);
    }
}
