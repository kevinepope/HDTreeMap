//! Toasts: short messages that report what an action did ("Copied path", "Moved X to the
//! Trash", a demo note, an error), stacked in the bottom-right corner over the map and gone
//! after a few seconds (`AppState::notify`, `state/notices.rs`).
//!
//! agg-gui has no toast or notification widget, so this is a small one: it paints the
//! current toasts itself each frame (newest at the bottom), never takes the pointer, and
//! asks for a redraw when the next toast is due to expire, so a reactive frame loop clears
//! it on time.

use std::sync::Arc;
use std::time::Duration;

use agg_gui::text::Font;
use agg_gui::{Color, DrawCtx, EventResult, Point, Rect, Size, Widget};

use crate::fonts::fa;
use crate::state::{AppState, ToastKind};

/// Widget id of the toast area.
pub const TOAST_ID: &str = "toasts";

const FONT_SIZE: f64 = 13.0;
const PAD_X: f64 = 12.0;
const PAD_Y: f64 = 8.0;
const GAP: f64 = 8.0;
/// Distance from the right edge.
const MARGIN_RIGHT: f64 = 16.0;
/// Distance from the bottom: clears the status bar, so toasts sit over the map.
const MARGIN_BOTTOM: f64 = 44.0;
/// Widest toast; longer text is cut with an ellipsis.
const MAX_WIDTH: f64 = 460.0;

const INFO_BG: Color = Color::rgba(0.12, 0.13, 0.15, 0.94);
const ERROR_BG: Color = Color::rgba(0.36, 0.10, 0.10, 0.95);
const TEXT: Color = Color::rgb(0.95, 0.95, 0.95);
const INFO_ICON: Color = Color::rgb(0.45, 0.85, 0.55);
const ERROR_ICON: Color = Color::rgb(1.0, 0.55, 0.45);

/// See the module docs.
pub struct Toasts {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    state: AppState,
    font: Arc<Font>,
    /// Texts shown by the last paint, oldest first (for tests).
    shown: Vec<String>,
}

impl Toasts {
    pub fn new(state: &AppState, font: Arc<Font>) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            state: state.clone(),
            font,
            shown: Vec::new(),
        }
    }

    /// The toasts the last paint showed, oldest first.
    pub fn shown(&self) -> &[String] {
        &self.shown
    }
}

/// `text` cut to fit `max` pixels (with an ellipsis) by `ctx`'s current font.
fn fit(ctx: &dyn DrawCtx, text: &str, max: f64) -> String {
    let width = |t: &str| ctx.measure_text(t).map_or(0.0, |m| m.width);
    if width(text) <= max {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "\u{2026}";
        if width(&candidate) <= max {
            return candidate;
        }
    }
    String::new()
}

impl Widget for Toasts {
    fn type_name(&self) -> &'static str {
        "Toasts"
    }

    fn id(&self) -> Option<&str> {
        Some(TOAST_ID)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        available
    }

    fn on_event(&mut self, _event: &agg_gui::Event) -> EventResult {
        EventResult::Ignored
    }

    /// Toasts never take the pointer: the map stays usable under them.
    fn hit_test(&self, _local_pos: Point) -> bool {
        false
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let toasts = self.state.toasts();
        self.shown = toasts.iter().map(|t| t.text.clone()).collect();
        if toasts.is_empty() {
            return;
        }
        ctx.set_font(Arc::clone(&self.font));
        ctx.set_font_size(FONT_SIZE);
        let line = ctx
            .measure_text("Ag")
            .map_or(FONT_SIZE * 1.3, |m| m.ascent + m.descent);
        let descent = ctx.measure_text("Ag").map_or(3.0, |m| m.descent);
        let height = line + 2.0 * PAD_Y;
        let mut y = MARGIN_BOTTOM;
        let mut next_expiry = Duration::MAX;
        // Newest at the bottom: walk from the newest up.
        for toast in toasts.iter().rev() {
            next_expiry = next_expiry.min(toast.lifetime().saturating_sub(toast.shown.elapsed()));
            let (bg, icon, icon_color) = match toast.kind {
                ToastKind::Info => (INFO_BG, fa::CHECK_CIRCLE, INFO_ICON),
                ToastKind::Error => (ERROR_BG, fa::WARNING, ERROR_ICON),
            };
            let icon_text = icon.to_string();
            let icon_w = ctx.measure_text(&icon_text).map_or(FONT_SIZE, |m| m.width);
            let text_max =
                (MAX_WIDTH.min(self.bounds.width - 2.0 * MARGIN_RIGHT) - icon_w - 3.0 * PAD_X)
                    .max(20.0);
            let text = fit(ctx, &toast.text, text_max);
            let text_w = ctx.measure_text(&text).map_or(0.0, |m| m.width);
            let width = icon_w + text_w + 3.0 * PAD_X;
            let x = self.bounds.width - MARGIN_RIGHT - width;
            if y + height > self.bounds.height {
                break;
            }
            ctx.set_fill_color(bg);
            ctx.begin_path();
            ctx.rounded_rect(x, y, width, height, 6.0);
            ctx.fill();
            let baseline = y + PAD_Y + descent;
            ctx.set_fill_color(icon_color);
            ctx.fill_text(&icon_text, x + PAD_X, baseline);
            ctx.set_fill_color(TEXT);
            ctx.fill_text(&text, x + 2.0 * PAD_X + icon_w, baseline);
            y += height + GAP;
        }
        if next_expiry < Duration::MAX {
            agg_gui::animation::request_draw_after(next_expiry + Duration::from_millis(20));
        }
    }
}
