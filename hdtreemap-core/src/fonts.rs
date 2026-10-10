//! Theme and fonts, installed once at startup by every shell (and by the headless tests) so
//! native, web and tests render with the same setup.
//!
//! The UI face is agg-gui's bundled Noto Sans with its standard fallback chain, which puts
//! Font Awesome 4.7 behind it: an icon is just its code point drawn as ordinary text. The
//! code points the app uses are named in [`fa`].

use std::sync::Arc;

use agg_gui::fonts::{standard_ui_font, FONT_AWESOME_4_7};
use agg_gui::text::Font;
use agg_gui::theme::{set_visuals, Visuals};
use agg_gui::{font_settings, set_device_scale};

/// Font Awesome 4.7 code points used by the app.
pub mod fa {
    /// fa-hdd-o — a drive.
    pub const HDD: char = '\u{f0a0}';
    /// fa-folder-open — "Choose Folder…".
    pub const FOLDER_OPEN: char = '\u{f07c}';
    /// fa-search — scan a drive.
    pub const SEARCH: char = '\u{f002}';
    /// fa-arrow-left — back to the start screen.
    pub const ARROW_LEFT: char = '\u{f060}';
    /// fa-info-circle — the web demo banner.
    pub const INFO_CIRCLE: char = '\u{f05a}';
    /// fa-th-large — the app mark next to the title.
    pub const TH_LARGE: char = '\u{f009}';
    /// fa-times — cancel a scan.
    pub const TIMES: char = '\u{f00d}';
    /// fa-refresh — rescan.
    pub const REFRESH: char = '\u{f021}';
    /// fa-exclamation-triangle — unreadable items.
    pub const WARNING: char = '\u{f071}';
    /// fa-chevron-right — between breadcrumb segments.
    pub const CHEVRON_RIGHT: char = '\u{f054}';
    /// fa-folder — a folder row in the folder tree.
    pub const FOLDER: char = '\u{f07b}';
    /// fa-file-o — a file row in the folder tree.
    pub const FILE_O: char = '\u{f016}';
    /// fa-ellipsis-h — the folder tree's "N more items" row.
    pub const ELLIPSIS_H: char = '\u{f141}';
    /// fa-caret-down — a legend column sorted largest first.
    pub const CARET_DOWN: char = '\u{f0d7}';
    /// fa-caret-up — a legend column sorted smallest (or A) first.
    pub const CARET_UP: char = '\u{f0d8}';
    /// fa-external-link — Open.
    pub const EXTERNAL_LINK: char = '\u{f08e}';
    /// fa-clipboard — Copy Path.
    pub const CLIPBOARD: char = '\u{f0ea}';
    /// fa-search-plus — Zoom Here / Zoom In.
    pub const SEARCH_PLUS: char = '\u{f00e}';
    /// fa-search-minus — Zoom Out.
    pub const SEARCH_MINUS: char = '\u{f010}';
    /// fa-level-up — Zoom to Parent.
    pub const LEVEL_UP: char = '\u{f148}';
    /// fa-home — Zoom to Root.
    pub const HOME: char = '\u{f015}';
    /// fa-trash — Move to Trash.
    pub const TRASH: char = '\u{f1f8}';
    /// fa-power-off — Quit.
    pub const POWER_OFF: char = '\u{f011}';
    /// fa-github — View on GitHub.
    pub const GITHUB: char = '\u{f09b}';
    /// fa-check-circle — a toast reporting success.
    pub const CHECK_CIRCLE: char = '\u{f058}';
    /// fa-eraser — Clear Highlight.
    pub const ERASER: char = '\u{f12d}';
}

/// The faces the widgets draw with.
#[derive(Clone)]
pub struct Fonts {
    /// Noto Sans with Font Awesome as fallback; used for all text.
    pub ui: Arc<Font>,
    /// Font Awesome alone, for `Button::with_icon`.
    pub icons: Arc<Font>,
}

/// Install the dark theme and the UI font for a display at `device_scale`, returning the
/// fonts the widget tree is built with.
pub fn install_theme_and_fonts(device_scale: f64) -> Result<Fonts, &'static str> {
    set_device_scale(device_scale);
    set_visuals(Visuals::dark());
    let ui = Arc::new(standard_ui_font());
    let icons = Arc::new(Font::from_slice(FONT_AWESOME_4_7)?);
    font_settings::set_system_font(Some(Arc::clone(&ui)));
    Ok(Fonts { ui, icons })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_cover_the_text_and_icons_the_app_draws() {
        let fonts = install_theme_and_fonts(1.0).expect("bundled fonts load");
        for ch in "HDTreeMap Choose Folder… 0123456789.% — · KB MB GB TB".chars() {
            assert!(fonts.ui.has_glyph(ch), "UI font is missing {ch:?}");
        }
        for icon in [
            fa::HDD,
            fa::FOLDER_OPEN,
            fa::SEARCH,
            fa::ARROW_LEFT,
            fa::INFO_CIRCLE,
            fa::TH_LARGE,
            fa::TIMES,
            fa::REFRESH,
            fa::WARNING,
            fa::CHEVRON_RIGHT,
            fa::FOLDER,
            fa::FILE_O,
            fa::ELLIPSIS_H,
            fa::CARET_DOWN,
            fa::CARET_UP,
            fa::EXTERNAL_LINK,
            fa::CLIPBOARD,
            fa::SEARCH_PLUS,
            fa::SEARCH_MINUS,
            fa::LEVEL_UP,
            fa::HOME,
            fa::TRASH,
            fa::POWER_OFF,
            fa::GITHUB,
            fa::CHECK_CIRCLE,
            fa::ERASER,
        ] {
            assert!(
                fonts.icons.has_glyph(icon),
                "Font Awesome is missing U+{:04X}",
                icon as u32
            );
            assert!(fonts.ui.has_glyph(icon), "UI fallback chain misses an icon");
        }
    }
}
