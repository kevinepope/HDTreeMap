//! The light and dark looks: the [`Theme`] setting (Light, Dark or Match System), the
//! agg-gui `Visuals` each look installs, and the few colours HDTreeMap draws itself, keyed
//! by light or dark so they read well on either: the treemap's neutral greys and overlay
//! outlines, and the folder tree's icon colours.
//!
//! `AppState` (`state/theme.rs`) holds the setting and the OS appearance the shell reports,
//! resolves them to light or dark, and installs the matching `Visuals`; widgets that paint
//! with `ctx.visuals()` follow at once, and the few built with a colour rebuild on
//! `AppState::theme_version`. The cushion hues are the same in both looks; only the greys
//! of folder blocks and the empty background change, so the map re-rasterises on a switch
//! (the palette is keyed on the look, `state/derived.rs`).

use agg_gui::theme::Visuals;
use agg_gui::Color;

use crate::render::Rgb;

/// Which look the app uses. Remembered in the settings; Light by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    /// Follow the OS appearance; Light where the shell cannot tell.
    System,
}

impl Theme {
    /// Every choice, in menu order.
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::System];

    /// The settings key.
    pub fn key(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }

    /// The theme stored as `key` (`None` for anything else).
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    /// The menu label.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::System => "Match System",
        }
    }

    /// Whether this theme is dark, given the OS appearance (`Some(true)` = dark; `None`
    /// when the shell cannot tell, which counts as light).
    pub fn is_dark(self, system_dark: Option<bool>) -> bool {
        match self {
            Theme::Light => false,
            Theme::Dark => true,
            Theme::System => system_dark.unwrap_or(false),
        }
    }
}

/// The agg-gui palette of the light or dark look.
pub fn visuals(dark: bool) -> Visuals {
    if dark {
        Visuals::dark()
    } else {
        let mut v = Visuals::light();
        // agg-gui's light dim text (50% black) is faint on the light panels; a little
        // darker keeps the secondary lines (paths, totals, hints) easy to read.
        v.text_dim = Color::rgba(0.08, 0.08, 0.10, 0.62);
        v
    }
}

/// The treemap's neutral colours for a look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapTones {
    /// Folder blocks and merged leftovers.
    pub block: Rgb,
    /// Raster pixels no rectangle covers (an empty folder).
    pub background: [u8; 4],
}

/// The treemap's neutral colours: dark greys on the dark look, light greys on the light.
pub fn map_tones(dark: bool) -> MapTones {
    if dark {
        MapTones {
            block: crate::render::FOLDER_BLOCK,
            background: crate::render::BACKGROUND,
        }
    } else {
        MapTones {
            block: [175.0 / 255.0, 175.0 / 255.0, 180.0 / 255.0],
            background: [226, 226, 230, 255],
        }
    }
}

/// The treemap's overlay outlines. Each is drawn over a darker `halo` line just inside it,
/// so it shows on light and dark rectangles alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapOverlay {
    /// The hovered item.
    pub hover: Color,
    /// The selected item.
    pub selected: Color,
    /// The hovered item's folder.
    pub folder: Color,
    /// The contrast line inside the hover and selection outlines.
    pub halo: Color,
    /// The contrast line inside the folder outline.
    pub folder_halo: Color,
}

/// The overlay outlines for a look.
pub fn map_overlay(dark: bool) -> MapOverlay {
    if dark {
        MapOverlay {
            hover: Color::rgb(1.0, 0.93, 0.35),
            selected: Color::rgb(0.25, 0.85, 1.0),
            folder: Color::rgba(1.0, 1.0, 1.0, 0.6),
            halo: Color::rgba(0.0, 0.0, 0.0, 0.75),
            folder_halo: Color::rgba(0.0, 0.0, 0.0, 0.45),
        }
    } else {
        MapOverlay {
            hover: Color::rgb(1.0, 0.90, 0.20),
            selected: Color::rgb(0.10, 0.70, 1.0),
            folder: Color::rgba(0.0, 0.0, 0.0, 0.6),
            halo: Color::rgba(0.0, 0.0, 0.0, 0.8),
            folder_halo: Color::rgba(1.0, 1.0, 1.0, 0.5),
        }
    }
}

/// The folder tree's icon colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeGlyphs {
    pub folder: Color,
    pub file: Color,
    /// The "N more items" row.
    pub more: Color,
}

/// The folder tree's icon colours for a look (darker on the light one).
pub fn tree_glyphs(dark: bool) -> TreeGlyphs {
    if dark {
        TreeGlyphs {
            folder: Color::rgb(0.90, 0.72, 0.20),
            file: Color::rgb(0.62, 0.74, 0.86),
            more: Color::rgb(0.6, 0.6, 0.6),
        }
    } else {
        TreeGlyphs {
            folder: Color::rgb(0.86, 0.60, 0.06),
            file: Color::rgb(0.33, 0.48, 0.66),
            more: Color::rgb(0.45, 0.45, 0.48),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_unknown_keys_are_none() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_key(theme.key()), Some(theme));
        }
        assert_eq!(Theme::from_key("solarized"), None);
        assert_eq!(Theme::default(), Theme::Light);
    }

    #[test]
    fn system_follows_the_os_and_falls_back_to_light() {
        assert!(Theme::System.is_dark(Some(true)));
        assert!(!Theme::System.is_dark(Some(false)));
        assert!(!Theme::System.is_dark(None));
        assert!(Theme::Dark.is_dark(None));
        assert!(!Theme::Light.is_dark(Some(true)));
    }

    #[test]
    fn each_look_installs_its_own_palette() {
        assert!(visuals(true).is_dark());
        assert!(!visuals(false).is_dark());
        assert_ne!(map_tones(true), map_tones(false));
    }
}
