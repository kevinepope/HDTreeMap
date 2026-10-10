//! The native side of the theme: the window's own appearance (its title bar, on macOS and
//! Windows) follows the app's theme, and for Match System the OS light / dark appearance is
//! read from winit's `Window::theme` and handed to the core (`AppState::set_system_dark`).
//!
//! winit reports the appearance on macOS (the app's effective appearance) and Windows (the
//! system setting); on X11 it reports nothing and on Wayland only an override, so there
//! Match System stays light. An OS appearance change wakes the event loop (winit's
//! `ThemeChanged`), and the next idle pass ([`WindowTheme::sync`]) picks it up.
//!
//! The window's appearance is set only when the choice changes: Light and Dark pin it, and
//! Match System clears the pin so `Window::theme` reports the OS again.

use std::sync::Arc;

use agg_gui_shell::winit::window::{Theme as WinitTheme, Window};
use hdtreemap_core::theme::Theme;
use hdtreemap_core::AppState;

/// Keeps the window and the OS appearance in step with the app's theme.
pub struct WindowTheme {
    window: Arc<Window>,
    /// The choice the window's appearance was last set for.
    applied: Option<Theme>,
}

impl WindowTheme {
    pub fn new(window: Arc<Window>) -> Self {
        Self {
            window,
            applied: None,
        }
    }

    /// Pin or release the window's appearance if the choice changed, then report the OS
    /// appearance while the choice is Match System. Cheap enough for every idle pass.
    pub fn sync(&mut self, state: &AppState) {
        let theme = state.theme();
        if self.applied != Some(theme) {
            self.window.set_theme(pinned(theme));
            self.applied = Some(theme);
        }
        if theme == Theme::System {
            state.set_system_dark(self.window.theme().map(|t| t == WinitTheme::Dark));
        }
    }
}

/// The window appearance for a choice: pinned for Light and Dark, the OS's for System.
fn pinned(theme: Theme) -> Option<WinitTheme> {
    match theme {
        Theme::Light => Some(WinitTheme::Light),
        Theme::Dark => Some(WinitTheme::Dark),
        Theme::System => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_match_system_leaves_the_window_to_the_os() {
        assert_eq!(pinned(Theme::Light), Some(WinitTheme::Light));
        assert_eq!(pinned(Theme::Dark), Some(WinitTheme::Dark));
        assert_eq!(pinned(Theme::System), None);
    }
}
