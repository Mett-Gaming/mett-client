//! Zeichnet in die Bildpuffer einer fremden D3D12-Swapchain, unmittelbar bevor das Bild gezeigt wird.
use crate::{Painter, target_properties};
use hud::Draw;
use windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_IGNORE;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Direct3D11on12::*;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::{IUnknown, Interface, Result};

struct Frame {
    wrapped: ID3D11Resource,
    bitmap: ID2D1Bitmap1,
}

pub struct Native {
    pub painter: Painter,
    swap: IDXGISwapChain3,
    on12: ID3D11On12Device,
    ctx11: ID3D11DeviceContext,
    frames: Vec<Frame>,
    size: (u32, u32),
}

impl Native {
    /// `queue` muss die Warteschlange sein, mit der die Swapchain erzeugt wurde.
    pub fn new(swap: &IDXGISwapChain3, queue: &ID3D12CommandQueue) -> Result<Self> {
        unsafe {
            let device: ID3D12Device = swap.GetDevice()?;
            let (mut d11, mut ctx11) = (None, None);
            let queues = [Some(queue.cast::<IUnknown>()?)];
            let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT.0;
            D3D11On12CreateDevice(&device, flags, None, Some(&queues), 0, Some(&mut d11), Some(&mut ctx11), None)?;
            let d11 = d11.ok_or(windows::Win32::Foundation::E_FAIL)?;
            let dxgi: IDXGIDevice = d11.cast()?;
            let d2d: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let ctx = d2d.CreateDevice(&dxgi)?.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let mut n = Self {
                painter: Painter::new(ctx)?,
                swap: swap.clone(),
                on12: d11.cast()?,
                ctx11: ctx11.ok_or(windows::Win32::Foundation::E_FAIL)?,
                frames: vec![],
                size: (0, 0),
            };
            n.wrap()?;
            Ok(n)
        }
    }

    fn wrap(&mut self) -> Result<()> {
        unsafe {
            let desc = self.swap.GetDesc1()?;
            self.size = (desc.Width, desc.Height);
            for i in 0..desc.BufferCount {
                let buffer: ID3D12Resource = self.swap.GetBuffer(i)?;
                let flags = D3D11_RESOURCE_FLAGS { BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32, ..Default::default() };
                let mut wrapped: Option<ID3D11Resource> = None;
                let present = D3D12_RESOURCE_STATE_PRESENT;
                self.on12.CreateWrappedResource(&buffer, &flags, present, present, &mut wrapped)?;
                let wrapped = wrapped.ok_or(windows::Win32::Foundation::E_FAIL)?;
                let surface: IDXGISurface = wrapped.cast()?;
                let props = target_properties(desc.Format, D2D1_ALPHA_MODE_IGNORE);
                let bitmap = self.painter.ctx.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
                self.frames.push(Frame { wrapped, bitmap });
            }
        }
        Ok(())
    }

    /// Gehört dieses Objekt zu der Swapchain?
    pub fn is_for(&self, swap: &IDXGISwapChain3) -> bool {
        self.swap == *swap
    }

    /// Aktuelle Bildgröße, auch direkt nach einer Änderung.
    pub fn size(&mut self) -> Result<(u32, u32)> {
        if self.frames.is_empty() {
            self.wrap()?;
        }
        Ok(self.size)
    }

    /// Zeichnet über das fertige Bild.
    pub fn draw(&mut self, items: &[Draw]) -> Result<()> {
        unsafe {
            if self.frames.is_empty() {
                self.wrap()?;
            }
            let i = self.swap.GetCurrentBackBufferIndex() as usize;
            let Some(f) = self.frames.get(i) else { return Ok(()) };
            let resource = [Some(f.wrapped.clone())];
            self.on12.AcquireWrappedResources(&resource);
            self.painter.ctx.SetTarget(&f.bitmap);
            self.painter.ctx.BeginDraw();
            let drawn = self.painter.items(items);
            let ended = self.painter.ctx.EndDraw(None, None);
            self.painter.ctx.SetTarget(None);
            // Immer zurückgeben, auch nach einem Fehler.
            self.on12.ReleaseWrappedResources(&resource);
            self.ctx11.Flush();
            drawn.and(ended)
        }
    }

    /// Gibt die Bildpuffer frei, damit das Spiel ihre Größe ändern kann. Beim nächsten Zeichnen neu geholt.
    pub fn release(&mut self) {
        unsafe {
            self.painter.ctx.SetTarget(None);
            self.frames.clear();
            self.ctx11.ClearState();
            self.ctx11.Flush();
        }
    }
}
