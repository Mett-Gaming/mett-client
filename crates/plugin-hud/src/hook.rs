//! Hängt sich in die Bildausgabe. Die Stellen kommen aus einer eigenen Probe-Swapchain und damit von Windows selbst,
//! nicht aus Minecraft: keine Spieladressen, kein veränderter Spielcode.
use std::mem::{offset_of, size_of};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::SeqCst};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, HRESULT, IUnknown, Interface, Result, w};

type Present = unsafe extern "system" fn(*mut std::ffi::c_void, u32, u32) -> HRESULT;
type Present1 = unsafe extern "system" fn(*mut std::ffi::c_void, u32, u32, *const DXGI_PRESENT_PARAMETERS) -> HRESULT;
type Resize = unsafe extern "system" fn(*mut std::ffi::c_void, u32, u32, u32, DXGI_FORMAT, u32) -> HRESULT;
type Resize1 = unsafe extern "system" fn(
    *mut std::ffi::c_void,
    u32,
    u32,
    u32,
    DXGI_FORMAT,
    u32,
    *const u32,
    *const *mut std::ffi::c_void,
) -> HRESULT;

/// Ereignisse an das Plugin.
pub trait Events: Sync {
    /// Kurz bevor das Bild gezeigt wird.
    fn frame(&self, swap: &IDXGISwapChain3);
    /// Die Bildpuffer müssen freigegeben werden.
    fn release(&self, swap: &IDXGISwapChain3);
}

static EVENTS: std::sync::OnceLock<&'static dyn Events> = std::sync::OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Aufrufe, die gerade in unserem Code sind.
static INSIDE: AtomicU32 = AtomicU32::new(0);

/// Platz in der Tabelle und ursprüngliche Funktion.
struct Slot {
    at: AtomicUsize,
    old: AtomicUsize,
}

static SLOTS: [Slot; 4] = [const { Slot { at: AtomicUsize::new(0), old: AtomicUsize::new(0) } }; 4];
const PRESENT: usize = 0;
const PRESENT1: usize = 1;
const RESIZE: usize = 2;
const RESIZE1: usize = 3;

struct Guard;

impl Guard {
    fn new() -> Self {
        INSIDE.fetch_add(1, SeqCst);
        Guard
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        INSIDE.fetch_sub(1, SeqCst);
    }
}

fn tell(this: *mut std::ffi::c_void, f: impl FnOnce(&dyn Events, &IDXGISwapChain3)) {
    if !ACTIVE.load(SeqCst) {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        if let Some(e) = EVENTS.get()
            && let Some(u) = IUnknown::from_raw_borrowed(&this)
            && let Ok(s) = u.cast::<IDXGISwapChain3>()
        {
            f(*e, &s);
        }
    }));
}

unsafe extern "system" fn present(this: *mut std::ffi::c_void, sync: u32, flags: u32) -> HRESULT {
    let _g = Guard::new();
    if flags & DXGI_PRESENT_TEST.0 == 0 {
        tell(this, |e, s| e.frame(s));
    }
    unsafe { std::mem::transmute::<usize, Present>(SLOTS[PRESENT].old.load(SeqCst))(this, sync, flags) }
}

unsafe extern "system" fn present1(
    this: *mut std::ffi::c_void,
    sync: u32,
    flags: u32,
    p: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    let _g = Guard::new();
    if flags & DXGI_PRESENT_TEST.0 == 0 {
        tell(this, |e, s| e.frame(s));
    }
    unsafe { std::mem::transmute::<usize, Present1>(SLOTS[PRESENT1].old.load(SeqCst))(this, sync, flags, p) }
}

unsafe extern "system" fn resize(
    this: *mut std::ffi::c_void,
    n: u32,
    w: u32,
    h: u32,
    f: DXGI_FORMAT,
    flags: u32,
) -> HRESULT {
    let _g = Guard::new();
    tell(this, |e, s| e.release(s));
    unsafe { std::mem::transmute::<usize, Resize>(SLOTS[RESIZE].old.load(SeqCst))(this, n, w, h, f, flags) }
}

#[allow(clippy::too_many_arguments)] // Form von IDXGISwapChain3::ResizeBuffers1
unsafe extern "system" fn resize1(
    this: *mut std::ffi::c_void,
    n: u32,
    w: u32,
    h: u32,
    f: DXGI_FORMAT,
    flags: u32,
    masks: *const u32,
    queues: *const *mut std::ffi::c_void,
) -> HRESULT {
    let _g = Guard::new();
    tell(this, |e, s| e.release(s));
    let old = SLOTS[RESIZE1].old.load(SeqCst);
    unsafe { std::mem::transmute::<usize, Resize1>(old)(this, n, w, h, f, flags, masks, queues) }
}

/// Was die Probe über die Swapchain von Windows verrät.
pub struct Probe {
    vtable: usize,
    /// Tabelle der Warteschlange, um sie beim Spiel wiederzuerkennen.
    queue_vtable: usize,
    /// Stellen in der Swapchain, an denen ihre Warteschlange steht.
    queue_offsets: Vec<usize>,
}

/// Lesbare Bytes ab hier, höchstens `max`.
fn readable(at: usize, max: usize) -> usize {
    let mut m = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe { VirtualQuery(Some(at as *const _), &mut m, size_of::<MEMORY_BASIC_INFORMATION>()) };
    let ok = [PAGE_READWRITE, PAGE_READONLY, PAGE_EXECUTE_READ, PAGE_WRITECOPY].contains(&m.Protect);
    if n == 0 || m.State != MEM_COMMIT || !ok {
        return 0;
    }
    (m.BaseAddress as usize + m.RegionSize - at).min(max)
}

impl Probe {
    pub fn new() -> Result<Self> {
        unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(h, m, w, l) }
        }
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None)?.into();
            let name = w!("MettProbe");
            let class =
                WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: name, ..Default::default() };
            RegisterClassW(&class);
            let none = Default::default();
            let window = CreateWindowExW(none, name, w!(""), WS_POPUP, 0, 0, 8, 8, None, None, Some(instance), None)?;
            let r = Self::with(window);
            let _ = DestroyWindow(window);
            let _ = UnregisterClassW(name, Some(instance));
            r
        }
    }

    fn with(window: HWND) -> Result<Self> {
        unsafe {
            let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let mut device: Option<ID3D12Device> = None;
            D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device)?;
            let device = device.ok_or(E_FAIL)?;
            let queue: ID3D12CommandQueue = device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC::default())?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: 8,
                Height: 8,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                ..Default::default()
            };
            let swap: IDXGISwapChain3 = factory.CreateSwapChainForHwnd(&queue, window, &desc, None, None)?.cast()?;
            let (object, wanted) = (swap.as_raw() as usize, queue.as_raw() as usize);
            let len = readable(object, 0x1000);
            let words = (0..len / 8).map(|i| i * 8);
            let offsets: Vec<usize> = words.filter(|o| *((object + o) as *const usize) == wanted).collect();
            if offsets.is_empty() {
                return Err(E_NOTIMPL.into());
            }
            Ok(Self {
                vtable: *(object as *const usize),
                queue_vtable: *(wanted as *const usize),
                queue_offsets: offsets,
            })
        }
    }

    /// Warteschlange einer anderen Swapchain, mehrfach geprüft. `None` heißt: nicht zeichnen.
    pub fn queue_of(&self, swap: &IDXGISwapChain3) -> Option<ID3D12CommandQueue> {
        unsafe {
            let object = swap.as_raw() as usize;
            if *(object as *const usize) != self.vtable {
                return None;
            }
            let device: ID3D12Device = swap.GetDevice().ok()?;
            let device: IUnknown = device.cast().ok()?;
            self.queue_offsets.iter().find_map(|o| {
                if readable(object + o, 8) < 8 {
                    return None;
                }
                let raw = *((object + o) as *const usize);
                if !raw.is_multiple_of(8) || readable(raw, 8) < 8 || *(raw as *const usize) != self.queue_vtable {
                    return None;
                }
                let raw = raw as *mut std::ffi::c_void;
                let q = ID3D12CommandQueue::from_raw_borrowed(&raw)?;
                let mut owner: Option<ID3D12Device> = None;
                q.GetDevice(&mut owner).ok()?;
                let same = owner?.cast::<IUnknown>().ok()? == device;
                (same && q.GetDesc().Type == D3D12_COMMAND_LIST_TYPE_DIRECT).then(|| q.clone())
            })
        }
    }

    /// Einhängen. Schlägt etwas fehl, wird alles zurückgenommen.
    pub fn install(&self, events: &'static dyn Events) -> Result<()> {
        let _ = EVENTS.set(events);
        type V = IDXGISwapChain3_Vtbl;
        const SC1: usize = offset_of!(V, base__) + offset_of!(IDXGISwapChain2_Vtbl, base__);
        const SC0: usize = SC1 + offset_of!(IDXGISwapChain1_Vtbl, base__);
        let table = [
            (SC0 + offset_of!(IDXGISwapChain_Vtbl, Present), present as *const () as usize),
            (SC1 + offset_of!(IDXGISwapChain1_Vtbl, Present1), present1 as *const () as usize),
            (SC0 + offset_of!(IDXGISwapChain_Vtbl, ResizeBuffers), resize as *const () as usize),
            (offset_of!(V, ResizeBuffers1), resize1 as *const () as usize),
        ];
        for (slot, (offset, new)) in SLOTS.iter().zip(table) {
            let at = self.vtable + offset;
            // Erst merken, dann tauschen: Der Hook darf nie ohne Original laufen.
            slot.old.store(unsafe { *(at as *const usize) }, SeqCst);
            slot.at.store(at, SeqCst);
            if let Err(e) = write(at, new) {
                uninstall();
                return Err(e);
            }
        }
        ACTIVE.store(true, SeqCst);
        Ok(())
    }
}

fn write(at: usize, value: usize) -> Result<()> {
    unsafe {
        let mut old = PAGE_PROTECTION_FLAGS::default();
        VirtualProtect(at as *const _, 8, PAGE_READWRITE, &mut old)?;
        (*(at as *const AtomicUsize)).store(value, SeqCst);
        VirtualProtect(at as *const _, 8, old, &mut old)
    }
}

/// `false`: Jemand hat sich nach uns eingehängt oder ein Aufruf läuft noch. Dann muss die DLL geladen bleiben.
pub fn uninstall() -> bool {
    ACTIVE.store(false, SeqCst);
    let ours = [
        present as *const () as usize,
        present1 as *const () as usize,
        resize as *const () as usize,
        resize1 as *const () as usize,
    ];
    let mut clean = true;
    for (slot, mine) in SLOTS.iter().zip(ours) {
        let at = slot.at.load(SeqCst);
        if at == 0 {
            continue;
        }
        // Nur zurücknehmen, was noch uns gehört.
        if unsafe { *(at as *const usize) } == mine {
            clean &= write(at, slot.old.load(SeqCst)).is_ok();
            slot.at.store(0, SeqCst);
        } else {
            clean = false;
        }
    }
    for _ in 0..100 {
        if INSIDE.load(SeqCst) == 0 {
            // Letzte Befehle hinter dem Zähler abwarten.
            std::thread::sleep(std::time::Duration::from_millis(50));
            return clean;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// Gehört die Swapchain zum sichtbaren Spielfenster dieses Prozesses?
pub fn is_game(swap: &IDXGISwapChain3) -> bool {
    unsafe {
        let Ok(h) = swap.GetHwnd() else { return false };
        let mut name = [0u16; 16];
        let n = GetClassNameW(h, &mut name) as usize;
        let visible: BOOL = IsWindowVisible(h);
        visible.as_bool() && String::from_utf16_lossy(&name[..n]) == "Bedrock"
    }
}
