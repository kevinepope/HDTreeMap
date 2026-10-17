//! Remembered settings: the size mode, shading style, colour scheme, the theme (light,
//! dark or match the system; light when missing, as in blobs from before it existed), the
//! two splitter positions and the recently scanned folders and drives, as one small JSON blob.
//!
//! Where the blob lives is the shell's business ([`SettingsStore`]): natively a file in the
//! OS config directory (`hdtreemap-native`), on the web `localStorage`
//! (`hdtreemap-wasm`). `AppState` (`state/persist.rs`) applies the loaded settings at
//! start-up and writes them back when they change, through agg-gui's diff-guarded
//! `AutoSave`, from the shell's idle hook while no mouse button is held.
//!
//! Reading never fails: a missing, unreadable or corrupt blob, or an unknown value in it,
//! falls back to the defaults field by field, and a failed write is ignored, so settings
//! can never stop the app from starting or running.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::SizeMode;
use crate::render::{ColorBy, Shading};
use crate::state::{SIDE_SPLIT_DEFAULT, TREE_SPLIT_DEFAULT};
use crate::theme::Theme;

/// How many recent scans are remembered (and listed on the start screen).
pub const MAX_RECENT: usize = 5;

/// Where a shell keeps the settings blob.
pub trait SettingsStore {
    /// The stored blob, or `None` when there is none or it cannot be read.
    fn load(&self) -> Option<String>;
    /// Replace the stored blob. Failures are the store's to swallow (or log).
    fn save(&self, blob: &str);
}

/// A folder or drive scanned before.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentScan {
    pub path: PathBuf,
    /// The drive's name, when a drive (not a folder) was scanned.
    #[serde(default)]
    pub volume_name: Option<String>,
    /// The drive's stable id (a demo profile on the web), when a drive was scanned.
    #[serde(default)]
    pub volume_id: Option<String>,
}

impl RecentScan {
    /// The drive's name, or the folder's path.
    pub fn display_name(&self) -> String {
        match &self.volume_name {
            Some(name) => name.clone(),
            None => self.path.to_string_lossy().into_owned(),
        }
    }
}

/// Everything remembered. See the module docs.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub size_mode: SizeMode,
    pub shading: Shading,
    pub color_by: ColorBy,
    /// Light, dark, or the OS appearance.
    pub theme: Theme,
    /// Share of the window width for the left column (folder tree and legend).
    pub side_split: f64,
    /// Share of the left column's height for the folder tree.
    pub tree_split: f64,
    /// Most recent first, at most [`MAX_RECENT`].
    pub recent: Vec<RecentScan>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            size_mode: SizeMode::default(),
            shading: Shading::default(),
            color_by: ColorBy::default(),
            theme: Theme::default(),
            side_split: SIDE_SPLIT_DEFAULT,
            tree_split: TREE_SPLIT_DEFAULT,
            recent: Vec::new(),
        }
    }
}

/// The blob's shape: plain strings and numbers, every field optional.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Blob {
    size_mode: Option<String>,
    /// `cushion`, `flat`, `bevel` or `cityscape`; files from before 0.1.2 have only `cushions`.
    shading: Option<String>,
    cushions: Option<bool>,
    color_by: Option<String>,
    theme: Option<String>,
    side_split: Option<f64>,
    tree_split: Option<f64>,
    recent: Vec<serde_json::Value>,
}

impl Settings {
    /// Read a blob, keeping the default for anything missing or not understood.
    pub fn from_json(text: &str) -> Self {
        let blob: Blob = serde_json::from_str(text).unwrap_or_default();
        let defaults = Settings::default();
        let split = |v: Option<f64>, default: f64| {
            v.filter(|r| r.is_finite() && (0.05..=0.95).contains(r))
                .unwrap_or(default)
        };
        let mut recent: Vec<RecentScan> = Vec::new();
        for value in blob.recent {
            if let Ok(scan) = serde_json::from_value::<RecentScan>(value) {
                if !scan.path.as_os_str().is_empty() && !recent.iter().any(|r| r.path == scan.path)
                {
                    recent.push(scan);
                }
            }
        }
        recent.truncate(MAX_RECENT);
        Self {
            size_mode: match blob.size_mode.as_deref() {
                Some("apparent") => SizeMode::Apparent,
                Some("allocated") => SizeMode::Allocated,
                _ => defaults.size_mode,
            },
            shading: match (
                blob.shading.as_deref().and_then(Shading::from_key),
                blob.cushions,
            ) {
                (Some(shading), _) => shading,
                (None, Some(true)) => Shading::Cushion,
                (None, Some(false)) => Shading::Flat,
                (None, None) => defaults.shading,
            },
            color_by: match blob.color_by.as_deref() {
                Some("folder") => ColorBy::TopFolder,
                Some("type") => ColorBy::Extension,
                _ => defaults.color_by,
            },
            theme: blob
                .theme
                .as_deref()
                .and_then(Theme::from_key)
                .unwrap_or(defaults.theme),
            side_split: split(blob.side_split, defaults.side_split),
            tree_split: split(blob.tree_split, defaults.tree_split),
            recent,
        }
    }

    /// The blob, pretty-printed (people do open these files).
    pub fn to_json(&self) -> String {
        let blob = Blob {
            size_mode: Some(
                match self.size_mode {
                    SizeMode::Allocated => "allocated",
                    SizeMode::Apparent => "apparent",
                }
                .to_string(),
            ),
            shading: Some(self.shading.key().to_string()),
            // Kept for 0.1.1 and earlier, which know only cushions on or off.
            cushions: Some(self.shading == Shading::Cushion),
            color_by: Some(
                match self.color_by {
                    ColorBy::Extension => "type",
                    ColorBy::TopFolder => "folder",
                }
                .to_string(),
            ),
            theme: Some(self.theme.key().to_string()),
            // Rounded, so a splitter nudged by a rounding error does not rewrite the file.
            side_split: Some((self.side_split * 1000.0).round() / 1000.0),
            tree_split: Some((self.tree_split * 1000.0).round() / 1000.0),
            recent: self
                .recent
                .iter()
                .filter_map(|r| serde_json::to_value(r).ok())
                .collect(),
        };
        serde_json::to_string_pretty(&blob).unwrap_or_default()
    }

    /// Put `scan` first in the recent list (dropping an older entry for the same path),
    /// keeping at most [`MAX_RECENT`].
    pub fn remember(&mut self, scan: RecentScan) {
        self.recent.retain(|r| r.path != scan.path);
        self.recent.insert(0, scan);
        self.recent.truncate(MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(path: &str) -> RecentScan {
        RecentScan {
            path: PathBuf::from(path),
            volume_name: None,
            volume_id: None,
        }
    }

    #[test]
    fn settings_round_trip_through_json() {
        let mut s = Settings {
            size_mode: SizeMode::Apparent,
            shading: Shading::Bevel,
            color_by: ColorBy::TopFolder,
            theme: Theme::Dark,
            side_split: 0.333_333,
            tree_split: 0.7,
            recent: Vec::new(),
        };
        s.remember(RecentScan {
            path: PathBuf::from("/Volumes/Backup"),
            volume_name: Some("Backup".into()),
            volume_id: Some("disk4s1".into()),
        });
        s.remember(folder("/Users/me/Code"));
        let back = Settings::from_json(&s.to_json());
        assert_eq!(back.size_mode, SizeMode::Apparent);
        assert_eq!(back.shading, Shading::Bevel);
        assert_eq!(back.color_by, ColorBy::TopFolder);
        assert_eq!(back.theme, Theme::Dark);
        assert!((back.side_split - 0.333).abs() < 1e-9);
        assert_eq!(back.tree_split, 0.7);
        assert_eq!(back.recent.len(), 2);
        assert_eq!(back.recent[0].path, PathBuf::from("/Users/me/Code"));
        assert_eq!(back.recent[1].display_name(), "Backup");
    }

    #[test]
    fn bad_blobs_fall_back_to_the_defaults_field_by_field() {
        assert_eq!(Settings::from_json(""), Settings::default());
        assert_eq!(Settings::from_json("not json {"), Settings::default());
        assert_eq!(Settings::from_json("[1, 2]"), Settings::default());
        let s = Settings::from_json(
            r#"{"size_mode": "huge", "cushions": false, "side_split": 7.5,
                "tree_split": null, "recent": [{"path": "/a"}, {"nope": 1}, {"path": ""},
                {"path": "/a"}, 42], "future_field": true}"#,
        );
        assert_eq!(s.size_mode, SizeMode::Allocated);
        assert_eq!(s.shading, Shading::Flat);
        assert_eq!(s.side_split, SIDE_SPLIT_DEFAULT);
        assert_eq!(s.tree_split, TREE_SPLIT_DEFAULT);
        assert_eq!(s.recent, vec![folder("/a")]);
    }

    #[test]
    fn the_shading_key_wins_over_the_old_cushions_flag() {
        // A file from 0.1.1 has only the flag.
        assert_eq!(
            Settings::from_json(r#"{"cushions": false}"#).shading,
            Shading::Flat
        );
        // A newer file carries both, written for either version to read.
        assert_eq!(
            Settings::from_json(r#"{"shading": "bevel", "cushions": false}"#).shading,
            Shading::Bevel
        );
        assert_eq!(
            Settings::from_json(r#"{"shading": "velvet", "cushions": true}"#).shading,
            Shading::Cushion
        );
        for shading in Shading::ALL {
            let s = Settings {
                shading,
                ..Settings::default()
            };
            let json = s.to_json();
            assert!(json.contains(&format!("\"shading\": \"{}\"", shading.key())));
            assert_eq!(Settings::from_json(&json).shading, shading);
        }
    }

    #[test]
    fn the_theme_is_light_unless_a_known_one_is_stored() {
        assert_eq!(Settings::default().theme, Theme::Light);
        // A settings.json from v0.1.0, before the theme was remembered.
        let old = Settings::from_json(
            r#"{"size_mode": "apparent", "cushions": true, "color_by": "type",
                "side_split": 0.3, "tree_split": 0.6, "recent": []}"#,
        );
        assert_eq!(old.theme, Theme::Light);
        assert_eq!(old.size_mode, SizeMode::Apparent);
        for (stored, want) in [
            (r#"{"theme": "dark"}"#, Theme::Dark),
            (r#"{"theme": "system"}"#, Theme::System),
            (r#"{"theme": "light"}"#, Theme::Light),
            (r#"{"theme": "neon"}"#, Theme::Light),
            (r#"{"theme": 3}"#, Theme::Light),
            (r#"{"theme": null}"#, Theme::Light),
        ] {
            assert_eq!(Settings::from_json(stored).theme, want, "{stored}");
        }
        let mut s = Settings::default();
        for theme in Theme::ALL {
            s.theme = theme;
            assert_eq!(Settings::from_json(&s.to_json()).theme, theme);
        }
    }

    #[test]
    fn the_recent_list_is_most_recent_first_without_repeats() {
        let mut s = Settings::default();
        for p in ["/1", "/2", "/3", "/4", "/5", "/6", "/3"] {
            s.remember(folder(p));
        }
        let paths: Vec<_> = s.recent.iter().map(|r| r.path.clone()).collect();
        let want: Vec<PathBuf> = ["/3", "/6", "/5", "/4", "/2"]
            .iter()
            .map(PathBuf::from)
            .collect();
        assert_eq!(paths, want);
    }
}
