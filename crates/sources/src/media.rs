//! Laufende Musik aus der Windows-Mediensteuerung (Spotify, Browser usw.).
use windows::Media::Control::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::Result;

pub fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

pub fn current() -> Result<Option<String>> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
    // Ohne Sitzung liefert Windows einen Fehler statt eines leeren Werts.
    let Ok(session) = manager.GetCurrentSession() else { return Ok(None) };
    let status = session.GetPlaybackInfo()?.PlaybackStatus()?;
    if status != GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
        return Ok(None);
    }
    let p = session.TryGetMediaPropertiesAsync()?.join()?;
    Ok(text(&p.Artist()?.to_string_lossy(), &p.Title()?.to_string_lossy()))
}

/// Gekürzt wird erst beim Zeichnen; das hier fängt nur Unsinn ab.
const MOST: usize = 200;

fn text(artist: &str, title: &str) -> Option<String> {
    let full = match (artist.trim(), title.trim()) {
        (_, "") => return None,
        ("", t) => t.to_string(),
        (a, t) => format!("{a} – {t}"),
    };
    if full.chars().count() <= MOST {
        return Some(full);
    }
    Some(full.chars().take(MOST - 1).chain(['…']).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nur_unsinn_wird_gekuerzt() {
        assert_eq!(text("A", "B").as_deref(), Some("A – B"));
        assert_eq!(text("", "B").as_deref(), Some("B"));
        assert_eq!(text("A", " "), None);
        assert_eq!(text("", &"ä".repeat(500)).unwrap().chars().count(), MOST);
        assert_eq!(text("A", &"b".repeat(60)).unwrap().chars().count(), 64);
    }
}
