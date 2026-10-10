//! The theme on the state side: the user's [`Theme`] (View ▸ Theme, the toolbar's sun/moon
//! button, the remembered settings), the OS appearance the shell reports for Match System
//! ([`AppState::set_system_dark`]), and installing the resolved look.
//!
//! [`AppState::apply_theme`] installs agg-gui's `Visuals` for the look (`theme::visuals`)
//! and bumps [`AppState::theme_version`], which the widgets built with a theme colour
//! (start screen, scan strip, status bar, toolbar button, folder-tree icons) watch to
//! rebuild or recolour. When the look flips between light and dark the map changes too:
//! the palette is keyed on the look (`state/derived.rs`), so the treemap re-rasterises
//! with the look's greys. Everything else paints with `ctx.visuals()` and follows at once.
//!
//! Switching is safe at any time, mid-scan included: it touches no scan state.

use std::cell::Cell;

use agg_gui::theme::set_visuals;

use super::AppState;
use crate::theme::{self, Theme};

/// See the module docs.
#[derive(Default)]
pub(super) struct ThemeState {
    theme: Cell<Theme>,
    /// The OS appearance: `Some(true)` dark, `None` unknown.
    system_dark: Cell<Option<bool>>,
    /// The look last installed (`None` before the first [`AppState::apply_theme`]).
    applied_dark: Cell<Option<bool>>,
    version: Cell<u64>,
}

impl AppState {
    fn theme_state(&self) -> &ThemeState {
        &self.inner.theme
    }

    /// The user's choice: Light, Dark or Match System.
    pub fn theme(&self) -> Theme {
        self.theme_state().theme.get()
    }

    /// Use `theme`, switching the look now if it changes it.
    pub fn set_theme(&self, theme: Theme) {
        if self.theme() != theme {
            self.theme_state().theme.set(theme);
            self.apply_theme_if_changed();
            // Menu marks and the remembered settings read the choice itself.
            agg_gui::animation::request_draw();
        }
    }

    /// Flip between light and dark (the toolbar button): the opposite of the look shown.
    pub fn toggle_theme(&self) {
        self.set_theme(if self.is_dark() {
            Theme::Light
        } else {
            Theme::Dark
        });
    }

    /// The OS appearance, as the shell last reported it (`None`: it cannot tell).
    pub fn system_dark(&self) -> Option<bool> {
        self.theme_state().system_dark.get()
    }

    /// The shell reports the OS appearance (`Some(true)` dark, `None` when it cannot
    /// tell). Under Match System the look follows it at once. Cheap to call every idle
    /// pass: nothing happens unless it changed.
    pub fn set_system_dark(&self, dark: Option<bool>) {
        if self.system_dark() != dark {
            self.theme_state().system_dark.set(dark);
            self.apply_theme_if_changed();
        }
    }

    /// Whether the look is dark: the choice, resolved against the OS appearance.
    pub fn is_dark(&self) -> bool {
        self.theme().is_dark(self.system_dark())
    }

    /// Bumped each time a look is installed.
    pub fn theme_version(&self) -> u64 {
        self.theme_state().version.get()
    }

    /// Install the look's `Visuals` and tell the widgets (see the module docs). `build_app`
    /// calls it once; later changes go through [`AppState::set_theme`] and
    /// [`AppState::set_system_dark`].
    pub fn apply_theme(&self) {
        let dark = self.is_dark();
        let flipped = self.theme_state().applied_dark.get() != Some(dark);
        set_visuals(theme::visuals(dark));
        self.theme_state().applied_dark.set(Some(dark));
        let version = &self.theme_state().version;
        version.set(version.get().wrapping_add(1));
        if flipped {
            // The palette's greys follow the look: re-rasterise.
            self.look_changed();
        }
        agg_gui::animation::request_draw();
    }

    fn apply_theme_if_changed(&self) {
        if self.theme_state().applied_dark.get() != Some(self.is_dark()) {
            self.apply_theme();
        }
    }

    /// The look the palette was made for (light until a theme is applied).
    pub(super) fn applied_dark(&self) -> bool {
        self.theme_state().applied_dark.get().unwrap_or(false)
    }
}
