//! `HDTREEMAP_SCREENSHOT_SHOW`: for README and review screenshots (with
//! `HDTREEMAP_SCREENSHOT`), open a piece of UI that normally needs a person at the mouse,
//! by sending the app the same input a person would, once the results are on screen:
//!
//! - `context-menu`: right-click the middle of the map;
//! - `trash`: that, then choose Move to Trash… (the confirmation opens; nothing is trashed
//!   unless it is confirmed, which this never does);
//! - `menu`: click the in-window menu bar's View menu (use with `HDTREEMAP_IN_WINDOW_MENU=1`
//!   on macOS, where the system bar is used otherwise);
//! - `about`: open the About sheet.
//!
//! Only input is synthesised here; every widget is the core's.

use agg_gui::widget::{find_widget_by_id, find_widget_screen_rect};
use agg_gui::{App, Modifiers, MouseButton, Rect};
use hdtreemap_core::state::Screen;
use hdtreemap_core::ui::context_menu::{ContextMenuHost, CONTEXT_MENU_ID};
use hdtreemap_core::ui::treemap_view::TREEMAP_ID;
use hdtreemap_core::AppState;

/// What to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    ContextMenu,
    Trash,
    Menu,
    About,
}

impl Show {
    /// From `HDTREEMAP_SCREENSHOT_SHOW`.
    pub fn from_env() -> Option<Self> {
        match std::env::var("HDTREEMAP_SCREENSHOT_SHOW").ok()?.as_str() {
            "context-menu" => Some(Show::ContextMenu),
            "trash" => Some(Show::Trash),
            "menu" => Some(Show::Menu),
            "about" => Some(Show::About),
            _ => None,
        }
    }
}

/// Steps through a [`Show`], one step per frame once the results are up.
pub struct Script {
    show: Show,
    step: u32,
}

impl Script {
    pub fn new(show: Show) -> Self {
        Self { show, step: 0 }
    }

    /// Called at the start of every frame.
    pub fn on_frame(&mut self, app: &mut App, state: &AppState) {
        if !matches!(state.screen(), Screen::Results(_)) {
            return;
        }
        self.step += 1;
        // Let the map lay out and paint first.
        match (self.show, self.step) {
            (Show::ContextMenu | Show::Trash, 5) => {
                if let Some(map) = find_widget_screen_rect(app.root(), TREEMAP_ID) {
                    let (x, y) = centre_down(app, map, 0.45, 0.4);
                    app.on_mouse_move(x, y);
                    app.on_mouse_down(x, y, MouseButton::Right, Modifiers::default());
                    app.on_mouse_up(x, y, MouseButton::Right, Modifiers::default());
                }
            }
            (Show::Trash, 8) => {
                let row = find_widget_by_id(app.root(), CONTEXT_MENU_ID)
                    .and_then(|w| w.as_any())
                    .and_then(|a| a.downcast_ref::<ContextMenuHost>())
                    .and_then(|m| m.item_rect("Move to Trash\u{2026}"));
                if let Some(row) = row {
                    let (x, y) = centre_down(app, row, 0.5, 0.5);
                    app.on_mouse_move(x, y);
                    app.on_mouse_down(x, y, MouseButton::Left, Modifiers::default());
                    app.on_mouse_up(x, y, MouseButton::Left, Modifiers::default());
                }
            }
            (Show::Menu, 5) => {
                if let Some(title) = find_widget_screen_rect(app.root(), "View Menu") {
                    let (x, y) = centre_down(app, title, 0.5, 0.5);
                    app.on_mouse_move(x, y);
                    app.on_mouse_down(x, y, MouseButton::Left, Modifiers::default());
                    app.on_mouse_up(x, y, MouseButton::Left, Modifiers::default());
                }
            }
            (Show::About, 5) => state.show_about(),
            _ => {}
        }
    }
}

/// The point at fractions `(fx, fy)` of `rect` (root, Y up, logical; `fy` from the top),
/// in the Y-down physical pixels the app's mouse entry points take.
fn centre_down(app: &App, rect: Rect, fx: f64, fy: f64) -> (f64, f64) {
    let scale = agg_gui::ux_scale::effective_scale();
    let height = app.root().bounds().height;
    let x = rect.x + rect.width * fx;
    let y_up = rect.y + rect.height * (1.0 - fy);
    (x * scale, (height - y_up) * scale)
}
