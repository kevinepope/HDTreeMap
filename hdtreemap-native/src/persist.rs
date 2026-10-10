//! Where the desktop app keeps what it remembers: `settings.json` (the core's settings
//! blob, `hdtreemap_core::settings`) and `window.json` (the window size, through
//! agg-gui-shell's `WindowBoundsStore`), both in `<OS config dir>/HDTreeMap/`.
//!
//! `HDTREEMAP_SETTINGS_DIR` points both somewhere else; a screenshot run
//! (`HDTREEMAP_SCREENSHOT`) without it uses neither, so screenshots start from the defaults
//! and never change the user's files. Files are written whole through a temporary file
//! and a rename, so a crash mid-write leaves the old file. Read and write failures are
//! ignored: the app then starts from the defaults or forgets the change.

use std::path::{Path, PathBuf};

use agg_gui_shell::{SavedBounds, WindowBoundsStore};
use hdtreemap_core::settings::SettingsStore;
use serde_json::json;

/// The folder the files live in, if any (see the module docs).
pub fn settings_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("HDTREEMAP_SETTINGS_DIR") {
        return Some(PathBuf::from(dir));
    }
    if std::env::var_os("HDTREEMAP_SCREENSHOT").is_some() {
        return None;
    }
    dirs::config_dir().map(|d| d.join("HDTreeMap"))
}

/// Write `text` to `path` through a temporary file and a rename.
fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// `settings.json`.
pub struct FileSettings {
    path: PathBuf,
}

impl FileSettings {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("settings.json"),
        }
    }
}

impl SettingsStore for FileSettings {
    fn load(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok()
    }

    fn save(&self, blob: &str) {
        // Losing a setting is harmless; never interrupt the event loop for it.
        let _ = write_atomically(&self.path, blob);
    }
}

/// `window.json`: `{"width": .., "height": .., "maximized": ..}` in physical pixels (the
/// shell sanitises whatever is loaded).
pub struct FileBounds {
    path: PathBuf,
}

impl FileBounds {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("window.json"),
        }
    }
}

/// Bounds from `window.json`'s text.
pub fn parse_bounds(text: &str) -> Option<SavedBounds> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    Some(SavedBounds {
        width: u32::try_from(v.get("width")?.as_u64()?).ok()?,
        height: u32::try_from(v.get("height")?.as_u64()?).ok()?,
        maximized: v
            .get("maximized")
            .and_then(|m| m.as_bool())
            .unwrap_or(false),
    })
}

/// `window.json`'s text for `bounds`.
pub fn format_bounds(bounds: SavedBounds) -> String {
    json!({
        "width": bounds.width,
        "height": bounds.height,
        "maximized": bounds.maximized,
    })
    .to_string()
}

impl WindowBoundsStore for FileBounds {
    fn load(&self) -> Option<SavedBounds> {
        parse_bounds(&std::fs::read_to_string(&self.path).ok()?)
    }

    fn save(&self, bounds: SavedBounds) {
        let _ = write_atomically(&self.path, &format_bounds(bounds));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_and_bounds_round_trip_through_their_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let settings = FileSettings::new(dir.path());
        assert_eq!(settings.load(), None);
        settings.save("{\"size_mode\": \"apparent\"}");
        assert_eq!(
            settings.load().as_deref(),
            Some("{\"size_mode\": \"apparent\"}")
        );
        let bounds = FileBounds::new(dir.path());
        assert_eq!(bounds.load(), None);
        let saved = SavedBounds {
            width: 2400,
            height: 1600,
            maximized: true,
        };
        bounds.save(saved);
        assert_eq!(bounds.load(), Some(saved));
        assert!(!dir.path().join("window.json.tmp").exists());
    }

    #[test]
    fn broken_bounds_files_are_ignored() {
        assert_eq!(parse_bounds(""), None);
        assert_eq!(parse_bounds("{\"width\": -3, \"height\": 10}"), None);
        assert_eq!(parse_bounds("{\"width\": 10}"), None);
        assert_eq!(
            parse_bounds("{\"width\": 10, \"height\": 20}"),
            Some(SavedBounds {
                width: 10,
                height: 20,
                maximized: false
            })
        );
    }

    #[test]
    fn an_unwritable_folder_is_not_an_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("a-file");
        std::fs::write(&file, "x").expect("write");
        // The "folder" is a file: every write fails, quietly.
        FileSettings::new(&file).save("{}");
        FileBounds::new(&file).save(SavedBounds {
            width: 1,
            height: 1,
            maximized: false,
        });
        assert_eq!(FileSettings::new(&file).load(), None);
    }
}
