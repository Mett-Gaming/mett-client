//! Bildschirm und Eingabe von Mett im Spiel: zeichnet, was der Client schickt, und meldet ihm Eingaben. Was gezeigt
//! wird und was Eingaben bedeuten, entscheidet der Client.
mod hook;
mod input;
mod mouse;
mod zoom;

use hook::{Events, Probe};
use hud::symbols::{Sheet, Symbols};
use hud::{Draw, wire};
use hudlink::Server;
use mett_plugin::{API, Host, Info};
use render::native::Native;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::sync::{Mutex, RwLock};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Dxgi::IDXGISwapChain3;
use windows::Win32::System::LibraryLoader::*;
use windows::core::PCWSTR;

/// Zum Auslesen von außen: gezeichnete Bilder, Fehler, letzter Fehlerwert, Bildformat.
#[unsafe(no_mangle)]
pub static METT_HUD: [AtomicU32; 6] = [const { AtomicU32::new(0) }; 6];
static STATE: Mutex<Option<State>> = Mutex::new(None);
static HUD: Hud = Hud;
static LINK: RwLock<Option<Server>> = RwLock::new(None);

/// Verbindung zum Client, falls sie besteht.
pub(crate) fn link<R>(f: impl FnOnce(&Server) -> R) -> Option<R> {
    LINK.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(f)
}

struct State {
    probe: Probe,
    native: Option<Native>,
    /// Nach wiederholten Fehlern wird nicht weiter versucht.
    failures: u32,
    scene: Vec<Draw>,
    /// Symboltafeln nach Platz.
    sheets: BTreeMap<usize, Sheet>,
    sheets_new: bool,
}

// Die Grafikobjekte werden nur unter der Sperre benutzt.
unsafe impl Send for State {}

struct Hud;

impl Events for Hud {
    fn frame(&self, swap: &IDXGISwapChain3) {
        // Nie warten: lieber ein Bild ohne HUD als ein Ruckler.
        let Ok(mut s) = STATE.try_lock() else { return };
        let Some(s) = &mut *s else { return };
        if s.failures >= 5 || !hook::is_game(swap) {
            return;
        }
        let start = std::time::Instant::now();
        let drawn = draw(s, swap);
        // Zeit je Bild in Nanosekunden, geglättet und längste.
        let t = start.elapsed().as_nanos().min(u32::MAX as u128 / 2) as u32;
        let mean = METT_HUD[4].load(SeqCst);
        METT_HUD[4].store(mean - mean / 64 + t / 64, SeqCst);
        METT_HUD[5].fetch_max(t, SeqCst);
        if let Err(e) = drawn {
            s.native = None;
            s.failures += 1;
            // Ohne Anzeige darf niemand ohne Eingabe dastehen.
            input::close();
            METT_HUD[1].fetch_add(1, SeqCst);
            METT_HUD[2].store(e.code().0 as u32, SeqCst);
        }
    }

    fn release(&self, swap: &IDXGISwapChain3) {
        let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = s.as_mut().and_then(|s| s.native.as_mut())
            && n.is_for(swap)
        {
            n.release();
        }
    }
}

fn draw(s: &mut State, swap: &IDXGISwapChain3) -> windows::core::Result<()> {
    if s.native.as_ref().is_some_and(|n| !n.is_for(swap)) {
        s.native = None;
    }
    if s.native.is_none() {
        METT_HUD[3].store(unsafe { swap.GetDesc1()? }.Format.0 as u32, SeqCst);
        let queue = s.probe.queue_of(swap).ok_or(windows::Win32::Foundation::E_NOINTERFACE)?;
        let mut n = Native::new(swap, &queue)?;
        // Minecrafts Schrift aus der Installation; wird nicht mitgeliefert.
        if let Some(dir) = std::env::current_exe().ok().as_ref().and_then(|p| p.parent()) {
            n.painter.load_game_font(&dir.join("data").join("fonts").join("Mojangles.ttf"));
        }
        s.native = Some(n);
        s.sheets_new = true;
    }
    input::tick();
    match link(|l| (l.alive(), l.scene())) {
        Some((true, Some(bytes))) => s.scene = wire::decode(&bytes).unwrap_or_default(),
        Some((true, None)) => {}
        // Ohne Client bleibt nichts stehen.
        _ => s.scene.clear(),
    }
    let alive = link(|l| l.alive()).unwrap_or(false);
    for d in link(|l| l.sheets()).unwrap_or_default() {
        s.sheets_new = true;
        match Sheet::new(d.id, d.side, d.pixels) {
            Some(sheet) => s.sheets.insert(d.slot, sheet),
            None => s.sheets.remove(&d.slot),
        };
    }
    if !alive && !s.sheets.is_empty() {
        s.sheets.clear();
        s.sheets_new = true;
    }
    let Some(n) = &mut s.native else { return Ok(()) };
    if std::mem::take(&mut s.sheets_new) {
        n.painter.set_symbols(Symbols(s.sheets.values().map(|s| (s.id, s.clone())).collect()));
    }
    let size = n.size()?;
    if !s.scene.is_empty() && !zoom::hides() {
        n.draw(&s.scene)?;
    }
    let frames = METT_HUD[0].fetch_add(1, SeqCst) + 1;
    link(|l| l.report(size, frames, METT_HUD[1].load(SeqCst)));
    s.failures = 0;
    Ok(())
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
    std::panic::catch_unwind(|| {
        let Ok(probe) = Probe::new() else { return 1 };
        let Ok(server) = Server::create() else { return 4 };
        let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        *LINK.write().unwrap_or_else(|e| e.into_inner()) = Some(server);
        if probe.install(&HUD).is_err() {
            LINK.write().unwrap_or_else(|e| e.into_inner()).take();
            return 2;
        }
        // Ohne Fenster bleibt die Anzeige, nur die Eingabe fällt aus.
        input::install();
        // Ohne GameInput bleibt nur die Kamera beweglich.
        mouse::install();
        *s =
            Some(State { probe, native: None, failures: 0, scene: vec![], sheets: BTreeMap::new(), sheets_new: false });
        0
    })
    .unwrap_or(3)
}

#[unsafe(no_mangle)]
pub extern "system" fn mett_plugin_unload() {
    // Ab hier meldet sich niemand mehr; der Client merkt es daran.
    zoom::clear();
    let clean = input::uninstall() & mouse::uninstall() & hook::uninstall();
    if clean {
        if let Some(mut s) = STATE.lock().unwrap_or_else(|e| e.into_inner()).take()
            && let Some(n) = &mut s.native
        {
            n.release();
        }
        LINK.write().unwrap_or_else(|e| e.into_inner()).take();
        return;
    }
    LINK.write().unwrap_or_else(|e| e.into_inner()).take();
    // Noch im Aufrufweg: geladen lassen, damit nichts ins Leere springt.
    unsafe {
        let mut pinned = HMODULE::default();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
        let _ = GetModuleHandleExW(flags, PCWSTR(mett_plugin_unload as *const () as *const u16), &mut pinned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hud::{Rect, rgb, theme};
    use hudlink::{Client, Event, control};
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::Win32::Graphics::Dxgi::Common::*;
    use windows::Win32::Graphics::Dxgi::*;
    use windows::Win32::System::Threading::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{Interface, w};

    struct Game {
        device: ID3D12Device,
        queue: ID3D12CommandQueue,
        swap: IDXGISwapChain3,
        alloc: ID3D12CommandAllocator,
        list: ID3D12GraphicsCommandList,
        fence: ID3D12Fence,
        value: u64,
    }

    fn barrier(r: &ID3D12Resource, a: D3D12_RESOURCE_STATES, b: D3D12_RESOURCE_STATES) -> D3D12_RESOURCE_BARRIER {
        D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    // Leiht sich die Ressource nur; kein zusätzlicher Verweis.
                    pResource: unsafe { std::mem::transmute_copy(r) },
                    Subresource: 0,
                    StateBefore: a,
                    StateAfter: b,
                }),
            },
        }
    }

    impl Game {
        fn new(format: DXGI_FORMAT) -> Self {
            unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
                let watched = [WM_KEYDOWN, WM_KEYUP, WM_CHAR, WM_INPUT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEWHEEL];
                if watched.contains(&m) {
                    SEEN.lock().unwrap().push(m);
                    return LRESULT(0);
                }
                unsafe { DefWindowProcW(h, m, w, l) }
            }
            unsafe {
                let c = WNDCLASSW { lpfnWndProc: Some(proc), lpszClassName: w!("Bedrock"), ..Default::default() };
                RegisterClassW(&c);
                let ex = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
                let h = CreateWindowExW(ex, w!("Bedrock"), w!(""), WS_POPUP, 0, 0, 400, 300, None, None, None, None);
                let h = h.unwrap();
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                let mut device: Option<ID3D12Device> = None;
                D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device).unwrap();
                let device = device.unwrap();
                let queue: ID3D12CommandQueue = device.CreateCommandQueue(&Default::default()).unwrap();
                let f: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).unwrap();
                let desc = DXGI_SWAP_CHAIN_DESC1 {
                    Width: 400,
                    Height: 300,
                    Format: format,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: 3,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                    ..Default::default()
                };
                let swap = f.CreateSwapChainForHwnd(&queue, h, &desc, None, None).unwrap().cast().unwrap();
                let alloc: ID3D12CommandAllocator =
                    device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
                let list: ID3D12GraphicsCommandList =
                    device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None).unwrap();
                list.Close().unwrap();
                let fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
                Self { device, queue, swap, alloc, list, fence, value: 0 }
            }
        }

        fn wait(&mut self) {
            unsafe {
                self.value += 1;
                self.queue.Signal(&self.fence, self.value).unwrap();
                let e = CreateEventW(None, false, false, None).unwrap();
                self.fence.SetEventOnCompletion(self.value, e).unwrap();
                WaitForSingleObject(e, 5000);
                CloseHandle(e).unwrap();
            }
        }

        fn run(&mut self, f: impl FnOnce(&ID3D12GraphicsCommandList)) {
            unsafe {
                self.alloc.Reset().unwrap();
                self.list.Reset(&self.alloc, None).unwrap();
                f(&self.list);
                self.list.Close().unwrap();
                self.queue.ExecuteCommandLists(&[Some(self.list.cast().unwrap())]);
            }
            self.wait();
        }

        /// Zeigt ein Bild und liefert danach die Farbe an dieser Stelle.
        fn show(&mut self, x: u32, y: u32) -> [u8; 4] {
            unsafe {
                let i = self.swap.GetCurrentBackBufferIndex();
                let buffer: ID3D12Resource = self.swap.GetBuffer(i).unwrap();
                let heap: ID3D12DescriptorHeap = self
                    .device
                    .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                        Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                        NumDescriptors: 1,
                        ..Default::default()
                    })
                    .unwrap();
                let rtv = heap.GetCPUDescriptorHandleForHeapStart();
                self.device.CreateRenderTargetView(&buffer, None, rtv);
                let (p, rt) = (D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET);
                self.run(|l| {
                    l.ResourceBarrier(&[barrier(&buffer, p, rt)]);
                    l.ClearRenderTargetView(rtv, &[0.0, 0.0, 1.0, 1.0], None);
                    l.ResourceBarrier(&[barrier(&buffer, rt, p)]);
                });
                self.swap.Present(0, DXGI_PRESENT(0)).unwrap();
                self.wait();
                let desc = D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                    Width: 2048 * 600,
                    Height: 1,
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                    ..Default::default()
                };
                let props = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_READBACK, ..Default::default() };
                let mut back: Option<ID3D12Resource> = None;
                let dest = D3D12_RESOURCE_STATE_COPY_DEST;
                self.device
                    .CreateCommittedResource(&props, D3D12_HEAP_FLAG_NONE, &desc, dest, None, &mut back)
                    .unwrap();
                let back = back.unwrap();
                let d = buffer.GetDesc();
                let mut foot = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
                self.device.GetCopyableFootprints(&d, 0, 1, 0, Some(&mut foot), None, None, None);
                let src = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&buffer),
                    Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
                };
                let dst = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&back),
                    Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: foot },
                };
                let cs = D3D12_RESOURCE_STATE_COPY_SOURCE;
                self.run(|l| {
                    l.ResourceBarrier(&[barrier(&buffer, p, cs)]);
                    l.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
                    l.ResourceBarrier(&[barrier(&buffer, cs, p)]);
                });
                let mut data = std::ptr::null_mut();
                back.Map(0, None, Some(&mut data)).unwrap();
                let at = (y * foot.Footprint.RowPitch + x * 4) as usize;
                let c = *data.cast::<u8>().add(at).cast::<[u8; 4]>();
                back.Unmap(0, None);
                c
            }
        }
    }

    const BLUE: [u8; 4] = [0, 0, 255, 255];

    static SEEN: Mutex<Vec<u32>> = Mutex::new(vec![]);

    fn inputs(h: HWND, c: &Client) {
        let send = |m: u32, w: usize, l: isize| unsafe { SendMessageW(h, m, Some(WPARAM(w)), Some(LPARAM(l))) };
        let seen = || std::mem::take(&mut *SEEN.lock().unwrap());
        let (w, t) = (b'W' as usize, b'T' as usize);
        send(WM_KEYDOWN, w, 0);
        send(WM_INPUT, 0, 0);
        assert_eq!(seen(), [WM_KEYDOWN, WM_INPUT]);
        // Will der Client die Taste nicht, gehört sie dem Spiel.
        c.beat(0);
        send(WM_KEYDOWN, t, 0);
        send(WM_KEYUP, t, 0);
        assert_eq!(seen(), [WM_KEYDOWN, WM_KEYUP]);
        c.set_keys(&[84, 13], &[191]);
        c.beat(control::CHAT);
        c.events();
        // Öffnen: Maus, Tasten und Klicks erreichen das Spiel nicht mehr.
        send(WM_KEYDOWN, t, 0);
        send(WM_CHAR, b'x' as usize, 0);
        send(WM_KEYUP, t, 0);
        send(WM_INPUT, 0, 0);
        send(WM_LBUTTONDOWN, 1, 0);
        send(WM_LBUTTONUP, 0, 0);
        send(WM_KEYDOWN, b'A' as usize, 0);
        for c in "ä😀".encode_utf16() {
            send(WM_CHAR, c as usize, 0);
        }
        send(WM_CHAR, 8, 0);
        send(WM_KEYUP, b'A' as usize, 0);
        assert_eq!(seen(), []);
        // Vorher gedrückt: Loslassen muss ankommen, sonst läuft die Figur weiter.
        send(WM_KEYUP, w, 0);
        assert_eq!(seen(), [WM_KEYUP]);
        let got = c.events();
        assert_eq!(got.len(), 6, "{got:?}");
        // Ob gerade Umschalttasten gedrückt sind, hängt vom Rechner ab.
        assert_eq!(got[0], Event::Open { command: false });
        assert_eq!(got[1..3], [Event::Button { which: 0, down: true }, Event::Button { which: 0, down: false }]);
        assert!(matches!(got[3], Event::Key { vk: b'A', .. }));
        assert_eq!(got[4..], [Event::Char('ä'), Event::Char('😀')]);
        // Zeiger frei und danach wieder wie vorher.
        assert_eq!(send(WM_SETCURSOR, 0, 0).0, 1);
        // Schließen mit Esc: Wiederholung und Loslassen gehören noch uns, danach ist alles wieder frei.
        let esc = VK_ESCAPE.0 as usize;
        assert!(input::blocked());
        send(WM_KEYDOWN, esc, 0);
        assert!(!input::blocked());
        send(WM_KEYDOWN, esc, 1 << 30);
        send(WM_KEYUP, esc, 0);
        assert_eq!(seen(), []);
        send(WM_KEYDOWN, esc, 0);
        send(WM_INPUT, 0, 0);
        send(WM_LBUTTONDOWN, 1, 0);
        assert_eq!(seen(), [WM_KEYDOWN, WM_INPUT, WM_LBUTTONDOWN]);
        // Tab: nur auf Wunsch des Clients unsere, mit Wiederholung und Loslassen.
        c.events();
        seen();
        let tab = VK_TAB.0 as usize;
        send(WM_KEYDOWN, tab, 0);
        send(WM_KEYUP, tab, 0);
        assert_eq!(seen(), [WM_KEYDOWN, WM_KEYUP]);
        c.beat(control::CHAT | control::TAB);
        send(WM_KEYDOWN, tab, 0);
        send(WM_KEYDOWN, tab, 1 << 30);
        assert!(!input::blocked());
        send(WM_KEYUP, tab, 0);
        assert_eq!(seen(), []);
        assert_eq!(c.events(), [Event::Tab { down: true }, Event::Tab { down: false }]);
        // Zoom: ohne Zoom-Plugin bleiben Taste und Mausrad beim Spiel.
        let (zoom, wheel) = (b'C' as usize, 120 << 16);
        c.set_zoom_keys([b'C', 0, 0, 0]);
        c.beat(control::CHAT | control::ZOOM | control::ZOOM_WHEEL);
        send(WM_KEYDOWN, zoom, 0);
        send(WM_KEYUP, zoom, 0);
        assert_eq!(seen(), [WM_KEYDOWN, WM_KEYUP]);
        zoom::pretend(-4, 2, 0.5, true);
        let state = |i: usize| zoom::METT_ZOOM[i].load(SeqCst);
        send(WM_KEYDOWN, zoom, 0);
        assert_eq!((state(0), state(1)), (1, 1));
        (0..5).for_each(|_| {
            send(WM_MOUSEWHEEL, wheel, 0);
        });
        assert_eq!(state(2), 2);
        assert_eq!(zoom::mouse(control::ZOOM | control::ZOOM_WHEEL), (0.5, 0.0, true));
        send(WM_KEYUP, zoom, 0);
        assert_eq!((state(0), seen()), (0, vec![]));
        // Unbelegt heißt keine Taste, auch nicht die Nummer 0.
        assert_eq!(zoom::key(0, u32::MAX, [0; 4]), None);
        zoom::clear();
        send(WM_MOUSEWHEEL, wheel, 0);
        assert_eq!(seen(), [WM_MOUSEWHEEL]);
        c.beat(control::CHAT | control::TAB);
        // Alt+Tab: Das Fenster verliert den Fokus, das Loslassen kommt nie.
        send(WM_KEYDOWN, tab, 0);
        send(WM_KILLFOCUS, 0, 0);
        assert_eq!(c.events(), [Event::Tab { down: true }, Event::Tab { down: false }]);
        send(WM_KEYUP, tab, 0);
        seen();
        // Menü: der Client sperrt und gibt frei; Esc entscheidet er.
        c.events();
        c.beat(control::MENU);
        send(WM_KEYDOWN, esc, 0);
        send(WM_MOUSEMOVE, 0, 7 << 16 | 5);
        assert!(input::blocked() && seen().is_empty());
        let got = c.events();
        assert!(matches!(got[0], Event::Key { vk: 0x1B, .. }) && got[1] == Event::Move { x: 5, y: 7 });
        send(WM_KEYUP, esc, 0);
        // Offener Chat und Client weg: alles wird frei.
        c.beat(control::CHAT);
        send(WM_KEYDOWN, t, 0);
        assert!(input::blocked());
        std::thread::sleep(std::time::Duration::from_millis(hudlink::TIMEOUT_MS + 50));
        assert!(!input::blocked());
        seen();
        send(WM_INPUT, 0, 0);
        assert_eq!(seen(), [WM_INPUT]);
    }

    /// Alles in einem Test, weil der Hook nur einmal im Prozess lebt.
    #[test]
    fn zeichnet_ins_bild_und_verschwindet_wieder() {
        let host = Host { size: size_of::<Host>() as u32, api: API, game: [0; 4], base: 0, image_size: 0 };
        let mut g = Game::new(DXGI_FORMAT_R8G8B8A8_UNORM);
        assert_eq!(g.show(90, 4), BLUE);
        for round in 0..2 {
            assert_eq!(unsafe { mett_plugin_load(&host) }, 0);
            let c = Client::open(std::process::id()).unwrap();
            // Ohne Befehle bleibt das Bild unberührt.
            assert_eq!(g.show(90, 4), BLUE);
            let rect = Rect { x: 80.0, y: 0.0, w: 240.0, h: 40.0 };
            let color = rgb(theme::ACCENT, 1.0);
            assert!(c.send(&wire::encode(&[Draw::Box { rect, radius: 0.0, color }])));
            c.beat(0);
            let accent = [(theme::ACCENT >> 16) as u8, (theme::ACCENT >> 8) as u8, theme::ACCENT as u8];
            let state = || METT_HUD.each_ref().map(|a| a.load(SeqCst));
            for _ in 0..7 {
                c.beat(0);
                assert_eq!(g.show(90, 4)[..3], accent, "Runde {round}, {:x?}", state());
                assert_eq!(g.show(200, 200), BLUE);
            }
            // Größe ändern: Das Spiel bekommt seine Bildpuffer frei.
            let flags = DXGI_SWAP_CHAIN_FLAG(0);
            unsafe { g.swap.ResizeBuffers(0, 500 + round, 320, DXGI_FORMAT_UNKNOWN, flags) }.unwrap();
            c.beat(0);
            assert_eq!(g.show(140, 4)[..3], accent, "{:x?}", state());
            assert_eq!(c.screen(), Some((500 + round, 320)));
            assert_eq!(METT_HUD[1].load(SeqCst), 0);
            inputs(unsafe { g.swap.GetHwnd() }.unwrap(), &c);
            // Der Client schwieg: Es bleibt nichts stehen.
            assert_eq!(g.show(140, 4), BLUE);
            mett_plugin_unload();
            assert!(!c.connected());
            drop(c);
            assert_eq!(g.show(140, 4), BLUE);
            // Nach dem Entladen hält nichts mehr die Bildpuffer fest.
            unsafe { g.swap.ResizeBuffers(0, 400, 300, DXGI_FORMAT_UNKNOWN, flags) }.unwrap();
        }
        // 10-Bit-Bild (HDR-fähig): zeichnen oder sauber aufgeben, nie das Spiel stören.
        unsafe { DestroyWindow(g.swap.GetHwnd().unwrap()) }.unwrap();
        let mut ten = Game::new(DXGI_FORMAT_R10G10B10A2_UNORM);
        assert_eq!(unsafe { mett_plugin_load(&host) }, 0);
        for _ in 0..8 {
            ten.show(90, 4);
        }
        println!("10 Bit: {:x?}", METT_HUD.each_ref().map(|a| a.load(SeqCst)));
        mett_plugin_unload();
    }
}
