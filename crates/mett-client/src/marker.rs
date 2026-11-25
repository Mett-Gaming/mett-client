//! Erkennt, ob der Zeiger auf unserem Knopf im Pausenmenü liegt: Das Pack färbt ihn dabei in einer eigenen Farbe.
//! Hängt nicht an Lage, Auflösung oder Spieldaten. Gelesen wird nur ein kleines Feld um den Zeiger, nur beim Klick.
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::*;

/// Muss zu `pack/textures/ui/mett_hover.png` passen.
pub const COLOR: [u8; 3] = [110, 139, 61];
const W: i32 = 33;
const H: i32 = 25;

pub fn at(p: POINT) -> bool {
    read(p).is_some_and(|px| matches(&px))
}

/// Mindestens ein Drittel; der Rest darf Beschriftung oder Rand sein.
fn matches(bgra: &[u8]) -> bool {
    let near = |a: u8, b: u8| a.abs_diff(b) <= 3;
    let hit = |p: &&[u8; 4]| near(p[2], COLOR[0]) && near(p[1], COLOR[1]) && near(p[0], COLOR[2]);
    let pixels = bgra.as_chunks::<4>().0;
    !pixels.is_empty() && pixels.iter().filter(hit).count() * 3 >= pixels.len()
}

fn read(p: POINT) -> Option<Vec<u8>> {
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, W, H);
        let old = SelectObject(mem, bmp.into());
        let copied = BitBlt(mem, 0, 0, W, H, Some(screen), p.x - W / 2, p.y - H / 2, SRCCOPY).is_ok();
        SelectObject(mem, old);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: W,
                biHeight: H,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (W * H * 4) as usize];
        let rows = GetDIBits(mem, bmp, 0, H as u32, Some(px.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        (copied && rows == H).then_some(px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(share: usize, c: [u8; 3]) -> Vec<u8> {
        (0..100).flat_map(|i| if i < share { [c[2], c[1], c[0], 255] } else { [200; 4] }).collect()
    }

    #[test]
    fn erkennt_farbe_trotz_schrift() {
        assert!(matches(&field(60, COLOR)));
        assert!(matches(&field(60, [COLOR[0] + 2, COLOR[1] - 2, COLOR[2]])));
        assert!(matches(&field(34, COLOR)));
        assert!(!matches(&field(30, COLOR)));
    }

    #[test]
    fn andere_knoepfe_loesen_nichts_aus() {
        // Vanilla-Grün, Vanilla-Grau, Eckknopf, leer.
        for c in [[32, 131, 8], [198, 198, 198], [126, 135, 88]] {
            assert!(!matches(&field(100, c)));
        }
        assert!(!matches(&[]));
    }
}
