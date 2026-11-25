//! Ersatz-Spiel für Tests: eigenes D3D12-Fenster der Klasse „Bedrock“, zeigt blaue Bilder, bis es beendet wird.
//! Zählt Tasten und rohe Maus, die bei ihm ankommen, und endet nach der angegebenen Zeit von selbst.
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, w};

static KEYS: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_KEYDOWN {
        KEYS.fetch_add(1, SeqCst);
    }
    unsafe { DefWindowProcW(h, m, w, l) }
}

fn main() -> windows::core::Result<()> {
    let seconds = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(20);
    unsafe {
        let class = WNDCLASSW { lpfnWndProc: Some(proc), lpszClassName: w!("Bedrock"), ..Default::default() };
        RegisterClassW(&class);
        let ex = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
        let h = CreateWindowExW(ex, w!("Bedrock"), w!("Ersatz"), WS_POPUP, 0, 0, 400, 300, None, None, None, None)?;
        let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        let mut device: Option<ID3D12Device> = None;
        D3D12CreateDevice(None, D3D_FEATURE_LEVEL_11_0, &mut device)?;
        let device = device.ok_or(E_FAIL)?;
        let queue: ID3D12CommandQueue = device.CreateCommandQueue(&Default::default())?;
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: 400,
            Height: 300,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 3,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            ..Default::default()
        };
        let swap: IDXGISwapChain3 = factory.CreateSwapChainForHwnd(&queue, h, &desc, None, None)?.cast()?;
        let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
        let event = CreateEventW(None, false, false, None)?;
        println!("bereit {}", h.0 as isize);
        let start = std::time::Instant::now();
        let mut n = 0u64;
        while start.elapsed().as_secs() < seconds {
            let mut m = MSG::default();
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
            }
            swap.Present(1, DXGI_PRESENT(0)).ok()?;
            n += 1;
            queue.Signal(&fence, n)?;
            fence.SetEventOnCompletion(n, event)?;
            WaitForSingleObject(event, 1000);
        }
        println!("Ende: {n} Bilder, {} Tasten", KEYS.load(SeqCst));
    }
    Ok(())
}
