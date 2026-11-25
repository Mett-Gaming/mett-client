//! FPS aus Windows' Ereignisprotokoll (ETW), wie bei PresentMon. Kein Eingriff ins Spiel.
//! Braucht die Windows-Gruppe „Leistungsprotokollbenutzer“.
use crate::frames::{Fps, Frames};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::System::Diagnostics::Etw::*;
use windows::core::{GUID, PCWSTR, PWSTR, w};

const NAME: PCWSTR = w!("MettClientFPS");
const DXGI: GUID = GUID::from_u128(0xca11c036_0102_4a2d_a6ad_f03cfed5d3c9);
/// Beginn von Present und PresentMultiplaneOverlay.
const PRESENT: [u16; 2] = [42, 55];

struct Context {
    pid: u32,
    state: Mutex<(Frames, Option<Instant>)>,
}

pub struct FpsSession {
    control: CONTROLTRACE_HANDLE,
    thread: Option<std::thread::JoinHandle<()>>,
    context: Arc<Context>,
}

/// Kopf plus Platz für den Sitzungsnamen, wie ETW es verlangt.
#[repr(C)]
struct Properties {
    head: EVENT_TRACE_PROPERTIES,
    name: [u16; 64],
}

impl Properties {
    fn new() -> Self {
        let mut head = EVENT_TRACE_PROPERTIES::default();
        head.Wnode.BufferSize = size_of::<Properties>() as u32;
        head.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        head.Wnode.ClientContext = 1; // Leistungszähler
        head.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        head.FlushTimer = 1;
        head.LoggerNameOffset = std::mem::offset_of!(Properties, name) as u32;
        Self { head, name: [0; 64] }
    }
}

fn stop_by_name() {
    let mut p = Properties::new();
    unsafe {
        let _ = ControlTraceW(CONTROLTRACE_HANDLE::default(), NAME, &mut p.head, EVENT_TRACE_CONTROL_STOP);
    }
}

impl FpsSession {
    pub fn start(pid: u32) -> Result<Self, &'static str> {
        let mut control = CONTROLTRACE_HANDLE::default();
        let mut r = unsafe { StartTraceW(&mut control, NAME, &mut Properties::new().head) };
        if r == ERROR_ALREADY_EXISTS {
            // Rest eines abgebrochenen Laufs.
            stop_by_name();
            r = unsafe { StartTraceW(&mut control, NAME, &mut Properties::new().head) };
        }
        if r == ERROR_ACCESS_DENIED {
            return Err("FPS: Recht fehlt");
        }
        if r != ERROR_SUCCESS {
            return Err("FPS nicht messbar");
        }
        let context = Arc::new(Context { pid, state: Mutex::default() });
        let mut session = Self { control, thread: None, context };
        unsafe {
            let on = EVENT_CONTROL_CODE_ENABLE_PROVIDER.0;
            let level = TRACE_LEVEL_INFORMATION as u8;
            // Ab hier räumt Drop die Sitzung auf.
            if EnableTraceEx2(session.control, &DXGI, on, level, 0, 0, 0, None) != ERROR_SUCCESS {
                return Err("FPS nicht messbar");
            }
        }
        let context = session.context.clone();
        session.thread = std::thread::Builder::new().spawn(move || consume(context)).ok();
        if session.thread.is_some() { Ok(session) } else { Err("FPS nicht messbar") }
    }

    pub fn pid(&self) -> u32 {
        self.context.pid
    }

    /// `None`, wenn länger kein Bild kam (Ladebildschirm, minimiert).
    pub fn fps(&self) -> Option<Fps> {
        let s = self.context.state.lock().ok()?;
        (s.1?.elapsed() < Duration::from_secs(3)).then(|| s.0.fps()).flatten()
    }
}

impl Drop for FpsSession {
    fn drop(&mut self) {
        stop_by_name();
        // Mit der Sitzung endet auch das Lesen.
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn consume(context: Arc<Context>) {
    unsafe {
        let mut log = EVENT_TRACE_LOGFILEW { LoggerName: PWSTR(NAME.0.cast_mut()), ..Default::default() };
        log.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        log.Anonymous2.EventRecordCallback = Some(on_event);
        log.Context = Arc::as_ptr(&context).cast_mut().cast();
        let h = OpenTraceW(&mut log);
        if h.Value == u64::MAX {
            return;
        }
        let _ = ProcessTrace(&[h], None, None);
        let _ = CloseTrace(h);
    }
    // `context` lebt bis hierher, der Rückruf kann also nicht ins Leere greifen.
}

unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
    unsafe {
        let r = &*record;
        let c = &*(r.UserContext as *const Context);
        let h = &r.EventHeader;
        if h.ProcessId == c.pid
            && h.ProviderId == DXGI
            && PRESENT.contains(&h.EventDescriptor.Id)
            && let Ok(mut s) = c.state.lock()
        {
            s.0.push(h.TimeStamp);
            s.1 = Some(Instant::now());
        }
    }
}
