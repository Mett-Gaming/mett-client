//! Zeichnet die HUD-Befehle mit Direct2D.
pub mod native;

use hud::symbols::{HEIGHT, Symbols, private};
use hud::{Align, Color, Draw, Rect};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::{HSTRING, Interface, Result, w};

/// Malt auf das gerade gesetzte Ziel.
pub struct Painter {
    pub(crate) ctx: ID2D1DeviceContext,
    brush: ID2D1SolidColorBrush,
    dwrite: IDWriteFactory5,
    font: (HSTRING, Option<IDWriteFontCollection>),
    game_font: Option<GameFont>,
    symbols: Symbols,
    /// Erst beim Zeichnen angelegt.
    sheets: std::cell::RefCell<std::collections::BTreeMap<u8, ID2D1Bitmap1>>,
    icons: std::cell::OnceCell<Option<ID2D1Bitmap1>>,
    /// Schriftformate nach Größe und Ausrichtung; jedes Bild neu anlegen wäre teuer.
    formats: std::cell::RefCell<std::collections::HashMap<(u32, u8), IDWriteTextFormat>>,
}

/// Maße der Minecraft-Schrift, ohne etwas zu zeichnen.
pub struct Metrics(Option<GameFont>);

impl Metrics {
    pub fn load(path: &std::path::Path) -> Self {
        let dwrite = unsafe { DWriteCreateFactory::<IDWriteFactory5>(DWRITE_FACTORY_TYPE_SHARED) };
        Self(dwrite.and_then(|d| GameFont::load(&d, path)).ok())
    }

    /// Breite eines Zeichens in Schriftpixeln, mit den Tafeln des Servers.
    pub fn advance(&self, c: char, symbols: &Symbols) -> f32 {
        self.0.as_ref().map_or(6.0, |f| f.wide(c, symbols))
    }
}

pub(crate) fn target_properties(format: DXGI_FORMAT, alpha: D2D1_ALPHA_MODE) -> D2D1_BITMAP_PROPERTIES1 {
    D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT { format, alphaMode: alpha },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
        ..Default::default()
    }
}

impl Painter {
    pub fn new(ctx: ID2D1DeviceContext) -> Result<Self> {
        unsafe {
            ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = ctx.CreateSolidColorBrush(&color([1.0; 4]), None)?;
            let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            // Klappt die eigene Schrift nicht, reicht die Systemschrift.
            let font = match load_font(&dwrite) {
                Ok(c) => (HSTRING::from("Archivo"), Some(c)),
                Err(_) => (HSTRING::from("Segoe UI"), None),
            };
            Ok(Self {
                ctx,
                brush,
                dwrite,
                font,
                game_font: None,
                symbols: Symbols::default(),
                sheets: Default::default(),
                icons: Default::default(),
                formats: Default::default(),
            })
        }
    }

    pub(crate) fn items(&self, items: &[Draw]) -> Result<()> {
        unsafe {
            for item in items {
                match item {
                    Draw::Box { rect, radius, color: c } => {
                        self.brush.SetColor(&color(*c));
                        if *radius > 0.0 {
                            let rr = D2D1_ROUNDED_RECT { rect: rectf(rect), radiusX: *radius, radiusY: *radius };
                            self.ctx.FillRoundedRectangle(&rr, &self.brush);
                        } else {
                            self.ctx.FillRectangle(&rectf(rect), &self.brush);
                        }
                    }
                    Draw::Outline { rect, radius, width, color: c } => {
                        self.brush.SetColor(&color(*c));
                        let h = width / 2.0;
                        let r = Rect { x: rect.x + h, y: rect.y + h, w: rect.w - width, h: rect.h - width };
                        let rr = D2D1_ROUNDED_RECT { rect: rectf(&r), radiusX: *radius, radiusY: *radius };
                        self.ctx.DrawRoundedRectangle(&rr, &self.brush, *width, None);
                    }
                    Draw::Icon { rect, from, alpha } => {
                        let Some(b) = self.icons.get_or_init(|| self.icon_sheet()) else { continue };
                        let nearest = D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR;
                        self.ctx.DrawBitmap(b, Some(&rectf(rect)), *alpha, nearest, Some(&rectf(from)), None);
                    }
                    Draw::Game { x, y, unit, color: c, text } => {
                        let Some(f) = &self.game_font else { continue };
                        self.brush.SetColor(&color(*c));
                        self.game_text(f, (*x, *y), *unit, text);
                    }
                    Draw::Text { rect, size, color: c, align, text } => {
                        self.brush.SetColor(&color(*c));
                        let format = self.format(*size, *align)?;
                        let text: Vec<u16> = text.encode_utf16().collect();
                        self.ctx.DrawText(
                            &text,
                            &format,
                            &rectf(rect),
                            &self.brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// Verbindet nur andere Zeichen und braucht selbst keinen Platz.
fn joiner(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200D}' | '\u{FE00}'..='\u{FE0F}' | '\u{2060}')
}

/// Platz für Zeichen, die die Schrift nicht hat: Symbole als Lücke, andere aus der Ersatzschrift.
fn missing_advance(c: char) -> f32 {
    if joiner(c) {
        0.0
    } else if private(c) {
        5.0
    } else {
        9.0
    }
}

/// Minecrafts Pixelschrift. Ein großes A ist 5 Pixel breit plus 1 Abstand, daraus ergibt sich die Größe.
struct GameFont {
    face: IDWriteFontFace,
    /// Schriftgröße je Schriftpixel.
    em: f32,
    /// Breite eines Zeichens in Schriftpixeln je Entwurfseinheit.
    per_design: f32,
}

impl GameFont {
    fn load(dwrite: &IDWriteFactory5, path: &std::path::Path) -> Result<Self> {
        unsafe {
            let file = dwrite.CreateFontFileReference(&HSTRING::from(path), None)?;
            let (mut ok, mut kind, mut face_kind, mut count) = Default::default();
            file.Analyze(&mut ok, &mut kind, Some(&mut face_kind), &mut count)?;
            let face = dwrite.CreateFontFace(face_kind, &[Some(file)], 0, DWRITE_FONT_SIMULATIONS_NONE)?;
            let mut m = DWRITE_FONT_METRICS::default();
            face.GetMetrics(&mut m);
            let mut f = Self { face, em: 1.0, per_design: 1.0 };
            let a = f.design_advance('A').filter(|a| *a > 0.0).ok_or(windows::Win32::Foundation::E_FAIL)?;
            f.per_design = 6.0 / a;
            f.em = m.designUnitsPerEm as f32 * f.per_design;
            Ok(f)
        }
    }

    /// `None`, wenn die Schrift das Zeichen nicht hat.
    fn glyph(&self, c: char) -> Option<u16> {
        let (code, mut g) = (c as u32, 0u16);
        unsafe { self.face.GetGlyphIndices(&code, 1, &mut g) }.ok()?;
        (g != 0).then_some(g)
    }

    fn design_advance(&self, c: char) -> Option<f32> {
        let g = self.glyph(c)?;
        let mut m = DWRITE_GLYPH_METRICS::default();
        unsafe { self.face.GetDesignGlyphMetrics(&g, 1, &mut m, false) }.ok()?;
        Some(m.advanceWidth as f32)
    }

    /// Was die Schrift nicht hat, kommt von der Tafel des Servers, wenn sie dort belegt ist.
    fn on_sheet(&self, c: char, symbols: &Symbols) -> bool {
        (private(c) || self.glyph(c).is_none()) && symbols.advance(c).is_some()
    }

    fn wide(&self, c: char, symbols: &Symbols) -> f32 {
        if self.on_sheet(c, symbols) { symbols.advance(c).unwrap_or(0.0) } else { self.advance(c) }
    }

    fn advance(&self, c: char) -> f32 {
        self.design_advance(c).map_or_else(|| missing_advance(c), |a| a * self.per_design)
    }
}

impl Painter {
    /// Fehlt die Schrift, bleibt der Chat leer statt in falscher Schrift.
    pub fn load_game_font(&mut self, path: &std::path::Path) {
        self.game_font = GameFont::load(&self.dwrite, path).ok();
    }

    /// Breite eines Zeichens in Schriftpixeln.
    pub fn game_advance(&self, c: char) -> f32 {
        self.game_font.as_ref().map_or(6.0, |f| f.advance(c))
    }

    /// Symboltafeln des Servers; ersetzt die bisherigen.
    pub fn set_symbols(&mut self, symbols: Symbols) {
        self.sheets.borrow_mut().clear();
        self.symbols = symbols;
    }

    fn format(&self, size: f32, align: Align) -> Result<IDWriteTextFormat> {
        let mut formats = self.formats.borrow_mut();
        let key = (size.to_bits(), align as u8);
        if let Some(f) = formats.get(&key) {
            return Ok(f.clone());
        }
        // Beim Ziehen entstehen viele Größen.
        if formats.len() >= 64 {
            formats.clear();
        }
        unsafe {
            let f = self.dwrite.CreateTextFormat(
                &self.font.0,
                self.font.1.as_ref(),
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("de-de"),
            )?;
            f.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            })?;
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            formats.insert(key, f.clone());
            Ok(f)
        }
    }

    /// Die Symbole von AppleSkin; ohne sie wird nur das nicht gezeichnet.
    fn icon_sheet(&self) -> Option<ID2D1Bitmap1> {
        let a = hud::appleskin::atlas()?;
        let mut px = a.rgba.clone();
        for p in px.as_chunks_mut::<4>().0 {
            let k = p[3] as u32;
            let m = |c: u8| (c as u32 * k / 255) as u8;
            *p = [m(p[2]), m(p[1]), m(p[0]), p[3]];
        }
        let props = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        };
        let size = D2D_SIZE_U { width: a.width, height: a.height };
        unsafe { self.ctx.CreateBitmap(size, Some(px.as_ptr().cast()), a.width * 4, &props) }.ok()
    }

    /// Zeichnet ein Symbol, wenn seine Tafel da ist.
    unsafe fn symbol(&self, c: char, at: (f32, f32), unit: f32) {
        let Some((sheet, index)) = self.symbols.find(c) else { return };
        let Some((x, y, w, h)) = sheet.cell(index) else { return };
        unsafe {
            // Schatten gibt es nur für Schrift; er kommt als dunkle Farbe.
            let ink = self.brush.GetColor();
            if ink.r.max(ink.g).max(ink.b) <= 0.26 {
                return;
            }
            let mut sheets = self.sheets.borrow_mut();
            if let std::collections::btree_map::Entry::Vacant(e) = sheets.entry(sheet.id) {
                let props = D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    ..Default::default()
                };
                let size = D2D_SIZE_U { width: sheet.side, height: sheet.side };
                let data = sheet.pixels.as_ptr().cast();
                let Ok(b) = self.ctx.CreateBitmap(size, Some(data), sheet.side * 4, &props) else { return };
                e.insert(b);
            }
            let Some(bitmap) = sheets.get(&sheet.id) else { return };
            let scale = HEIGHT * unit / h;
            let to = D2D_RECT_F { left: at.0, top: at.1, right: at.0 + w * scale, bottom: at.1 + HEIGHT * unit };
            let from = D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h };
            let nearest = D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR;
            self.ctx.DrawBitmap(bitmap, Some(&to), ink.a, nearest, Some(&from), None);
        }
    }

    /// Einzelnes Zeichen aus Windows' Schriften, z. B. Emoji.
    unsafe fn substitute(&self, c: char, at: (f32, f32), unit: f32, width: f32) {
        unsafe {
            let format = self.dwrite.CreateTextFormat(
                w!("Segoe UI Emoji"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                7.0 * unit,
                w!("de-de"),
            );
            let Ok(format) = format else { return };
            let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let text: Vec<u16> = c.encode_utf16(&mut [0; 2]).to_vec();
            let rect = Rect { x: at.0, y: at.1 - unit, w: width, h: 9.0 * unit };
            let options = D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT;
            self.ctx.DrawText(&text, &format, &rectf(&rect), &self.brush, options, DWRITE_MEASURING_MODE_NATURAL);
        }
    }

    unsafe fn game_text(&self, f: &GameFont, at: (f32, f32), unit: f32, text: &str) {
        // Fehlende Zeichen bleiben im Lauf als leerer Platz und werden einzeln aus der Ersatzschrift gemalt.
        let blank = f.glyph(' ').unwrap_or(0);
        let glyphs: Vec<u16> = text.chars().map(|c| f.glyph(c).unwrap_or(blank)).collect();
        let advances: Vec<f32> = text.chars().map(|c| f.wide(c, &self.symbols) * unit).collect();
        if glyphs.is_empty() {
            return;
        }
        let mut x = at.0;
        for (c, a) in text.chars().zip(&advances) {
            if f.on_sheet(c, &self.symbols) {
                unsafe { self.symbol(c, (x, at.1), unit) };
            } else if f.glyph(c).is_none() && !private(c) && !joiner(c) {
                unsafe { self.substitute(c, (x, at.1), unit, *a) };
            }
            x += a;
        }
        let mut run = DWRITE_GLYPH_RUN {
            fontFace: std::mem::ManuallyDrop::new(Some(f.face.clone())),
            fontEmSize: f.em * unit,
            glyphCount: glyphs.len() as u32,
            glyphIndices: glyphs.as_ptr(),
            glyphAdvances: advances.as_ptr(),
            ..Default::default()
        };
        unsafe {
            // Ohne Glättung, damit die Pixel scharf bleiben. Die Grundlinie liegt 7 Pixel unter der Oberkante.
            self.ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_ALIASED);
            let base = windows_numerics::Vector2 { X: at.0, Y: at.1 + 7.0 * unit };
            self.ctx.DrawGlyphRun(base, &run, None, &self.brush, DWRITE_MEASURING_MODE_NATURAL);
            self.ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            std::mem::ManuallyDrop::drop(&mut run.fontFace);
        }
    }
}

fn load_font(dwrite: &IDWriteFactory5) -> Result<IDWriteFontCollection> {
    static FONT: &[u8] = include_bytes!("../../../assets/fonts/Archivo-SemiBold.ttf");
    unsafe {
        let loader = dwrite.CreateInMemoryFontFileLoader()?;
        dwrite.RegisterFontFileLoader(&loader)?;
        let file = loader.CreateInMemoryFontFileReference(dwrite, FONT.as_ptr().cast(), FONT.len() as u32, None)?;
        let builder = dwrite.CreateFontSetBuilder()?;
        builder.AddFontFile(&file)?;
        let set = builder.CreateFontSet()?;
        dwrite.CreateFontCollectionFromFontSet(&set)?.cast()
    }
}

fn color(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c[0], g: c[1], b: c[2], a: c[3] }
}

fn rectf(r: &Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: r.x, top: r.y, right: r.x + r.w, bottom: r.y + r.h }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Malt auf ein Bild im Arbeitsspeicher (ohne Grafikkarte) und liefert Punkte als Blau, Grün, Rot, Deckkraft.
    fn paint(symbols: Symbols, items: &[Draw], at: &[(u32, u32)]) -> Vec<[u8; 4]> {
        use windows::Win32::Foundation::HMODULE;
        use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
        use windows::Win32::Graphics::Direct3D11::*;
        use windows::Win32::Graphics::Dxgi::*;
        unsafe {
            let (mut d3d, mut ctx11) = (None, None);
            let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
            let none = HMODULE::default();
            let out: Option<*mut Option<ID3D11Device>> = Some(&mut d3d);
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                none,
                flags,
                None,
                D3D11_SDK_VERSION,
                out,
                None,
                Some(&mut ctx11),
            )
            .unwrap();
            let (d3d, ctx11) = (d3d.unwrap(), ctx11.unwrap());
            let desc = D3D11_TEXTURE2D_DESC {
                Width: 64,
                Height: 64,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            };
            let (mut target, mut back) = (None, None);
            d3d.CreateTexture2D(&desc, None, Some(&mut target)).unwrap();
            let staging = D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..desc
            };
            d3d.CreateTexture2D(&staging, None, Some(&mut back)).unwrap();
            let (target, back) = (target.unwrap(), back.unwrap());
            let dxgi: IDXGIDevice = d3d.cast().unwrap();
            let d2d: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).unwrap();
            let ctx = d2d.CreateDevice(&dxgi).unwrap().CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE).unwrap();
            let mut p = Painter::new(ctx).unwrap();
            p.load_game_font(std::path::Path::new(r"C:\Windows\Fonts\consola.ttf"));
            p.set_symbols(symbols);
            let surface: IDXGISurface = target.cast().unwrap();
            let props = target_properties(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED);
            let bitmap = p.ctx.CreateBitmapFromDxgiSurface(&surface, Some(&props)).unwrap();
            p.ctx.SetTarget(&bitmap);
            p.ctx.BeginDraw();
            p.ctx.Clear(Some(&color([0.0, 0.0, 0.0, 1.0])));
            p.items(items).unwrap();
            p.ctx.EndDraw(None, None).unwrap();
            ctx11.CopyResource(&back, &target);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            ctx11.Map(&back, 0, D3D11_MAP_READ, 0, Some(&mut m)).unwrap();
            let get =
                |(x, y): &(u32, u32)| *m.pData.cast::<u8>().add((y * m.RowPitch + x * 4) as usize).cast::<[u8; 4]>();
            let out = at.iter().map(get).collect();
            ctx11.Unmap(&back, 0);
            out
        }
    }

    #[test]
    fn symbol_erscheint_an_seiner_stelle_und_ohne_schatten() {
        // Tafel mit 32 Punkten Kante; Zelle 0xDF ist grün.
        let mut px = vec![0u8; 32 * 32 * 4];
        for (x, y) in [(30, 26), (31, 26), (30, 27), (31, 27)] {
            px[(y * 32 + x) * 4..][..4].copy_from_slice(&[0, 255, 0, 255]);
        }
        let sheet = hud::symbols::Sheet::new(0xE4, 32, px).unwrap();
        let all = Symbols([(0xE4, sheet)].into());
        let text = |x: f32, c: f32| Draw::Game {
            x,
            y: 8.0,
            unit: 2.0,
            color: [c, c, c, 1.0],
            text: "\u{E4DF}\u{E4DF}".into(),
        };
        let black = [0, 0, 0, 255];
        let green = [0, 255, 0, 255];
        // Symbol 16 Punkte breit, dann 2 Punkte Abstand, dann das zweite.
        let at = [(2, 10), (17, 22), (18, 10), (20, 23), (2, 6), (2, 25)];
        assert_eq!(paint(all.clone(), &[text(0.0, 1.0)], &at), [green, black, green, green, black, black]);
        // Ein dunkler Schatten zeichnet kein Symbol, ohne Tafel bleibt die Lücke.
        assert_eq!(paint(all, &[text(0.0, 0.25)], &[(2, 10)]), [black]);
        assert_eq!(paint(Symbols::default(), &[text(0.0, 1.0)], &[(2, 10)]), [black]);
    }

    #[test]
    fn appleskin_symbole_kommen_unveraendert_an() {
        let from = Rect { x: 27.0, y: 0.0, w: 9.0, h: 9.0 };
        let icon = Draw::Icon { rect: Rect { x: 0.0, y: 0.0, w: 27.0, h: 27.0 }, from, alpha: 1.0 };
        // Ein Punkt der Bilddatei mit Farbe, einer ohne.
        let a = hud::appleskin::atlas().unwrap();
        let px = |x: usize, y: usize| &a.rgba[(y * 256 + x) * 4..][..4];
        let full: Vec<usize> = (0..9).filter(|x| px(27 + x, 0)[3] == 255).collect();
        let x = full[0];
        let p = px(27 + x, 0);
        let got = paint(Symbols::default(), &[icon], &[(x as u32 * 3 + 1, 1), (1, 1)]);
        assert_eq!(got, [[p[2], p[1], p[0], 255], [0, 0, 0, 255]]);
    }

    #[test]
    fn fehlende_zeichen_haben_passenden_platz() {
        // Das Symbol aus dem Spiel: Lücke statt Fragezeichen.
        assert_eq!(missing_advance('\u{E4DF}'), 5.0);
        assert_eq!(missing_advance('😀'), 9.0);
        assert_eq!(missing_advance('\u{FE0F}'), 0.0);
        assert!(private('\u{E000}') && private('\u{F8FF}') && !private('é') && !private('語'));
    }
}
