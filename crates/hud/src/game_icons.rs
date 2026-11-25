//! Bilder aus der Minecraft-Installation des Nutzers: Gegenstände, Effekte und Schnellleiste. Sie liegen dort offen als
//! PNG und TGA in einfachen Behältern (`.brarchive`, nicht verschlüsselt). Nichts davon wird mitgeliefert oder gespeichert.
//! Client und Plugin lesen dieselben Dateien, deshalb stimmt die Anordnung auf beiden Seiten überein.
use crate::appleskin::Atlas;
use crate::{Draw, Rect, SHEET_GAME};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// Feld je Bild auf der Tafel, Bilder je Zeile.
const CELL: u32 = 24;
const ROW: u32 = 32;
const MOST: usize = 2500;
const MAGIC: u64 = 0x2670_52A0_B125_277D;
/// Kopf des Behälters und jedes Eintrags.
const HEAD: usize = 16;
const ENTRY: usize = 256;
const LEATHER: [u32; 3] = [0xA0, 0x65, 0x40];

pub struct GameIcons {
    pub atlas: Atlas,
    /// Name → Platz, Breite, Höhe.
    index: HashMap<String, (u32, u32, u32)>,
}

struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

static ICONS: OnceLock<Option<GameIcons>> = OnceLock::new();

/// Ordner des Spiels; nur der erste Aufruf zählt.
pub fn init(game: &Path) {
    ICONS.get_or_init(|| load(game));
}

pub fn get() -> Option<&'static GameIcons> {
    ICONS.get()?.as_ref()
}

/// Packs von Minecraft nach Alter; neuere ersetzen Bilder älterer.
fn packs(root: &Path) -> Vec<std::path::PathBuf> {
    let version = |n: &str| -> Option<Vec<u32>> {
        match n.strip_prefix("vanilla")? {
            "" => Some(vec![]),
            v => v.strip_prefix('_')?.split('.').map(|p| p.parse().ok()).collect(),
        }
    };
    let dirs = std::fs::read_dir(root).into_iter().flatten().flatten();
    let mut all: Vec<_> = dirs.filter_map(|e| Some((version(&e.file_name().to_string_lossy())?, e.path()))).collect();
    all.sort();
    all.into_iter().map(|p| p.1).collect()
}

fn load(game: &Path) -> Option<GameIcons> {
    let mut found: HashMap<String, Vec<u8>> = HashMap::new();
    for pack in packs(&game.join("data").join("resource_packs")) {
        let dir = pack.join("__brarchive").join("textures");
        let items = std::fs::read(dir.join("items.brarchive")).unwrap_or_default();
        for (name, data) in entries(&items).unwrap_or_default() {
            // Zusätze für Beleuchtung sind keine Bilder.
            let extra = ["_mers.", "_mer.", "_normal.", "_heightmap."].iter().any(|e| name.contains(e));
            if !extra && (name.ends_with(".png") || name.ends_with(".tga")) {
                found.insert(format!("items/{name}"), data.to_vec());
            }
        }
        let ui = std::fs::read(dir.join("ui.brarchive")).unwrap_or_default();
        for (name, data) in entries(&ui).unwrap_or_default() {
            let wanted = name.ends_with("_effect.png") || name.starts_with("hotbar_");
            if wanted && name.ends_with(".png") {
                found.insert(format!("ui/{name}"), data.to_vec());
            }
        }
    }
    if found.is_empty() {
        return None;
    }
    let mut pictures: Vec<(String, Vec<u8>)> = found.into_iter().collect();
    // Feste Reihenfolge auf beiden Seiten.
    pictures.sort_by(|a, b| a.0.cmp(&b.0));
    pictures.truncate(MOST);
    let mut done = vec![];
    for (name, data) in pictures {
        let stem = name.rsplit_once('.').map_or(name.as_str(), |n| n.0).to_string();
        let picture = if name.ends_with(".png") {
            png(&data)
        } else if name.ends_with(".tga") {
            tga(&data)
        } else {
            None
        };
        let Some(mut p) = picture else { continue };
        if stem.contains("leather_") {
            p.tint(LEATHER);
        }
        if stem.starts_with("ui/hotbar_") {
            done.push((format!("{stem}@90"), p.turned()));
        }
        done.push((stem, p));
    }
    Some(arrange(done))
}

fn arrange(all: Vec<(String, Picture)>) -> GameIcons {
    let rows = (all.len() as u32).div_ceil(ROW).max(1);
    let (width, height) = (ROW * CELL, rows * CELL);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut index = HashMap::new();
    for (i, (name, p)) in all.into_iter().enumerate() {
        let i = i as u32;
        // Bewegte Bilder sind senkrechte Streifen; das erste zählt.
        let (w, h) = (p.width.min(CELL), p.height.min(CELL).min(if p.height > CELL { p.width } else { CELL }));
        for y in 0..h {
            let from = (y * p.width * 4) as usize;
            let to = ((i / ROW * CELL + y) * width + i % ROW * CELL) as usize * 4;
            rgba[to..to + (w * 4) as usize].copy_from_slice(&p.rgba[from..from + (w * 4) as usize]);
        }
        index.insert(name, (i, w, h));
    }
    GameIcons { atlas: Atlas { width, height, rgba }, index }
}

/// Namen und Daten eines Behälters.
fn entries(b: &[u8]) -> Option<Vec<(String, &[u8])>> {
    let int = |o: usize| Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize);
    if u64::from_le_bytes(b.get(..8)?.try_into().ok()?) != MAGIC || int(12)? != 1 {
        return None;
    }
    let count = int(8)?.min(10_000);
    let data = HEAD + count * ENTRY;
    let mut out = vec![];
    for i in 0..count {
        let o = HEAD + i * ENTRY;
        let len = *b.get(o)? as usize;
        let name = std::str::from_utf8(b.get(o + 1..o + 1 + len)?).ok()?;
        let (from, size) = (data + int(o + ENTRY - 8)?, int(o + ENTRY - 4)?);
        out.push((name.to_string(), b.get(from..from.checked_add(size)?)?));
    }
    Some(out)
}

fn png(data: &[u8]) -> Option<Picture> {
    let mut d = png::Decoder::new(data);
    d.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut r = d.read_info().ok()?;
    let size = r.output_buffer_size();
    if size > 1 << 22 {
        return None;
    }
    let mut b = vec![0; size];
    let i = r.next_frame(&mut b).ok()?;
    let rgba = match i.color_type {
        png::ColorType::Rgba => b,
        png::ColorType::GrayscaleAlpha => b.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        _ => return None,
    };
    let fits = rgba.len() == (i.width * i.height * 4) as usize && i.width > 0;
    fits.then_some(Picture { width: i.width, height: i.height, rgba })
}

/// TGA mit 32 Bit, gepackt oder nicht.
fn tga(b: &[u8]) -> Option<Picture> {
    let short = |o: usize| Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]) as u32);
    let (kind, width, height) = (*b.get(2)?, short(12)?, short(14)?);
    if *b.get(1)? != 0 || *b.get(16)? != 32 || !matches!(kind, 2 | 10) || width == 0 || width * height > 1 << 20 {
        return None;
    }
    let mut at = 18 + *b.first()? as usize;
    let total = (width * height) as usize;
    let mut px: Vec<[u8; 4]> = vec![];
    let read = |at: &mut usize| -> Option<[u8; 4]> {
        let p = b.get(*at..*at + 4)?;
        *at += 4;
        Some([p[2], p[1], p[0], p[3]])
    };
    while px.len() < total {
        if kind == 2 {
            px.push(read(&mut at)?);
            continue;
        }
        let head = *b.get(at)?;
        at += 1;
        let n = (head & 0x7F) as usize + 1;
        if head & 0x80 != 0 {
            let p = read(&mut at)?;
            px.extend(std::iter::repeat_n(p, n));
        } else {
            for _ in 0..n {
                px.push(read(&mut at)?);
            }
        }
    }
    px.truncate(total);
    // Sonst steht die unterste Zeile zuerst.
    if *b.get(17)? & 0x20 == 0 {
        px = px.chunks(width as usize).rev().flatten().copied().collect();
    }
    Some(Picture { width, height, rgba: px.into_iter().flatten().collect() })
}

impl Picture {
    /// Leder ist grau und wird erst im Spiel gefärbt.
    fn tint(&mut self, c: [u32; 3]) {
        for p in self.rgba.as_chunks_mut::<4>().0 {
            let gray = p[0] == p[1] && p[1] == p[2];
            if gray && p[3] > 0 {
                for (v, c) in p.iter_mut().zip(c) {
                    *v = (*v as u32 * c / 255) as u8;
                }
                p[3] = 255;
            }
        }
    }

    /// Eine Vierteldrehung nach rechts.
    fn turned(&self) -> Self {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut rgba = vec![0u8; self.rgba.len()];
        for y in 0..h {
            for x in 0..w {
                let to = (x * h + (h - 1 - y)) * 4;
                rgba[to..to + 4].copy_from_slice(&self.rgba[(y * w + x) * 4..][..4]);
            }
        }
        Self { width: self.height, height: self.width, rgba }
    }
}

impl GameIcons {
    pub fn has(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// Zeichnet mit `unit` Pixeln je Bildpunkt; `false`, wenn es das Bild nicht gibt.
    pub fn draw(&self, name: &str, (x, y): (f32, f32), unit: f32, alpha: f32, out: &mut Vec<Draw>) -> bool {
        let Some((i, w, h)) = self.index.get(name) else { return false };
        let from = Rect { x: (i % ROW * CELL) as f32, y: (i / ROW * CELL) as f32, w: *w as f32, h: *h as f32 };
        let (l, t) = (x.round(), y.round());
        let rect = Rect { x: l, y: t, w: (x + from.w * unit).round() - l, h: (y + from.h * unit).round() - t };
        out.push(Draw::Icon { sheet: SHEET_GAME, rect, from, color: [1.0, 1.0, 1.0, alpha] });
        true
    }

    /// Bild eines Gegenstands.
    pub fn item(&self, name: &str) -> Option<String> {
        let n = name.split_once(':').map_or(name, |n| n.1);
        let n = match n {
            "golden_apple" | "enchanted_golden_apple" => "apple_golden",
            "totem_of_undying" => "totem",
            "cooked_beef" => "beef_cooked",
            "beef" => "beef_raw",
            "cooked_chicken" => "chicken_cooked",
            "cooked_porkchop" => "porkchop_cooked",
            "cooked_mutton" => "mutton_cooked",
            "golden_carrot" => "carrot_golden",
            "firework_rocket" => "fireworks",
            "bow" => "bow_standby",
            "water_bucket" => "bucket_water",
            "lava_bucket" => "bucket_lava",
            n => n,
        };
        let gold = n.strip_prefix("golden_").map(|r| format!("gold_{r}"));
        [Some(n.to_string()), gold].into_iter().flatten().map(|n| format!("items/{n}")).find(|n| self.has(n))
    }
}

/// Kleine Auswahl für Prüfungen.
#[cfg(test)]
pub fn tests_support() -> GameIcons {
    let p = |w, h| Picture { width: w, height: h, rgba: vec![255; (w * h * 4) as usize] };
    let mut all = vec![("ui/hotbar_start_cap".to_string(), p(1, 22)), ("ui/hotbar_end_cap".to_string(), p(1, 22))];
    all.extend((0..9).map(|i| (format!("ui/hotbar_{i}"), p(20, 22))));
    for n in ["diamond_helmet", "netherite_chestplate", "iron_boots", "empty_armor_slot_leggings"] {
        all.push((format!("items/{n}"), p(16, 16)));
    }
    arrange(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = MAGIC.to_le_bytes().to_vec();
        b.extend((files.len() as u32).to_le_bytes());
        b.extend(1u32.to_le_bytes());
        let mut from = 0u32;
        for (name, data) in files {
            let mut e = vec![0u8; ENTRY];
            e[0] = name.len() as u8;
            e[1..1 + name.len()].copy_from_slice(name.as_bytes());
            e[ENTRY - 8..ENTRY - 4].copy_from_slice(&from.to_le_bytes());
            e[ENTRY - 4..].copy_from_slice(&(data.len() as u32).to_le_bytes());
            from += data.len() as u32;
            b.extend(e);
        }
        files.iter().for_each(|f| b.extend(f.1));
        b
    }

    #[test]
    fn behaelter_und_kaputte_daten() {
        let b = archive(&[("a.png", b"123"), ("b.png", b"4567")]);
        let e = entries(&b).unwrap();
        assert_eq!((e[0].0.as_str(), e[0].1, e[1].1), ("a.png", &b"123"[..], &b"4567"[..]));
        for n in 0..b.len() {
            assert!(entries(&b[..n]).is_none(), "{n}");
        }
        assert!(entries(b"keinbehaelter").is_none());
    }

    #[test]
    fn packs_nach_alter() {
        let root = std::env::temp_dir().join(format!("mett-packs-{}", std::process::id()));
        for n in ["vanilla_1.9", "vanilla_1.21.110", "vanilla", "vanilla_1.21.90", "chemistry", "vanilla_base"] {
            std::fs::create_dir_all(root.join(n)).unwrap();
        }
        let names: Vec<_> =
            packs(&root).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["vanilla", "vanilla_1.9", "vanilla_1.21.90", "vanilla_1.21.110"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tga_in_beiden_arten() {
        let mut head = vec![0u8; 18];
        (head[2], head[12], head[14], head[16], head[17]) = (2, 1, 2, 32, 0);
        let mut b = head.clone();
        b.extend([1, 2, 3, 255, 4, 5, 6, 255]);
        // Von unten nach oben, Blau zuerst.
        assert_eq!(tga(&b).unwrap().rgba, [6, 5, 4, 255, 3, 2, 1, 255]);
        (head[2], head[17]) = (10, 0x20);
        let mut b = head;
        b.extend([0x81, 1, 2, 3, 255]);
        assert_eq!(tga(&b).unwrap().rgba, [3, 2, 1, 255, 3, 2, 1, 255]);
        assert!(tga(&b[..20]).is_none());
    }

    #[test]
    fn drehen_und_anordnen() {
        let p = Picture { width: 2, height: 1, rgba: vec![1, 1, 1, 255, 2, 2, 2, 255] };
        let t = p.turned();
        assert_eq!((t.width, t.height, &t.rgba[..]), (1, 2, &[1, 1, 1, 255, 2, 2, 2, 255][..]));
        let icons = arrange(vec![("items/arrow".into(), p), ("items/gold_boots".into(), t)]);
        assert_eq!(icons.item("minecraft:arrow").as_deref(), Some("items/arrow"));
        assert_eq!(icons.item("minecraft:golden_boots").as_deref(), Some("items/gold_boots"));
        assert_eq!(icons.item("x:unbekannt"), None);
        let mut out = vec![];
        assert!(icons.draw("items/gold_boots", (10.0, 5.0), 3.0, 1.0, &mut out));
        let want = Rect { x: 24.0, y: 0.0, w: 1.0, h: 2.0 };
        assert!(matches!(out[0], Draw::Icon { from, rect, .. } if from == want && rect.w == 3.0 && rect.h == 6.0));
        assert!(!icons.draw("fehlt", (0.0, 0.0), 1.0, 1.0, &mut out));
    }
}
