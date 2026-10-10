//! The right-click menu of a file or folder, an agg-gui `PopupMenu` hosted by one widget at
//! the root of the app.
//!
//! The treemap and the folder tree only ask for a menu (`AppState::request_context_menu`,
//! with the point in root coordinates and the node). [`ContextMenuHost`] takes the request
//! on its next layout and opens the popup there; while it is open the host owns the input
//! (`has_active_modal`, `hit_test_global_overlay`) and paints the popup above everything
//! (`paint_global_overlay`). A chosen item runs its node action
//! (`ui/node_actions.rs`); a click elsewhere or Escape closes it.
//!
//! The host sits at the root's origin with no size, so it is never under the pointer while
//! closed.

use std::sync::Arc;

use agg_gui::text::Font;
use agg_gui::widgets::menu::{MenuEntry, MenuResponse, PopupMenu};
use agg_gui::{current_viewport, DrawCtx, Event, EventResult, Point, Rect, Size, Widget};

use super::menus::context_entries;
use super::node_actions::{NodeAction, NodeActions};
use crate::model::NodeId;
use crate::state::AppState;

/// Widget id of the host.
pub const CONTEXT_MENU_ID: &str = "context-menu";

/// Menu text size.
const FONT_SIZE: f64 = 13.0;

/// See the module docs.
pub struct ContextMenuHost {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    state: AppState,
    actions: NodeActions,
    font: Arc<Font>,
    popup: Option<(PopupMenu, NodeId)>,
    /// This widget's (0, 0) in root coordinates (logical), from the last paint.
    origin: Point,
}

impl ContextMenuHost {
    pub fn new(state: &AppState, actions: NodeActions, font: Arc<Font>) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            state: state.clone(),
            actions,
            font,
            popup: None,
            origin: Point::ORIGIN,
        }
    }

    /// Whether a menu is open.
    pub fn is_open(&self) -> bool {
        self.popup.is_some()
    }

    /// The node the open menu acts on.
    pub fn node(&self) -> Option<NodeId> {
        self.popup.as_ref().map(|(_, node)| *node)
    }

    /// The open menu's rows: `(label, enabled)`, separators left out.
    pub fn items(&self) -> Vec<(String, bool)> {
        let Some((menu, _)) = &self.popup else {
            return Vec::new();
        };
        menu.items
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item(item) => Some((item.label.clone(), item.enabled)),
                MenuEntry::Separator => None,
            })
            .collect()
    }

    /// The open popup's panel in root coordinates (Y up), for tests and screenshots.
    pub fn panel(&self) -> Option<Rect> {
        let (menu, _) = self.popup.as_ref()?;
        let layouts = menu.state.layouts(&menu.items, current_viewport());
        layouts.first().map(|l| l.rect)
    }

    /// The open menu's row labelled `label`, in root coordinates (Y up).
    pub fn item_rect(&self, label: &str) -> Option<Rect> {
        let (menu, _) = self.popup.as_ref()?;
        let layouts = menu.state.layouts(&menu.items, current_viewport());
        let panel = layouts.first()?;
        panel.rows.iter().find_map(|row| {
            let index = row.item_index?;
            match menu.items.get(index)? {
                MenuEntry::Item(item) if item.label == label => Some(row.rect),
                _ => None,
            }
        })
    }

    /// Run `action` on the open menu's node and close it (what clicking its row does).
    pub fn choose(&mut self, action: NodeAction) {
        if let Some((_, node)) = self.popup.take() {
            self.actions.run(node, action);
        }
    }

    fn open_requested(&mut self) {
        let Some(request) = self.state.take_context_request() else {
            return;
        };
        let entries = context_entries(&self.actions, request.node);
        let mut menu = PopupMenu::new(entries);
        menu.set_measure_font(Arc::clone(&self.font), FONT_SIZE);
        menu.open_at(request.at);
        self.popup = Some((menu, request.node));
    }

    /// `event` with its position moved from this widget's space to root space.
    fn to_root(&self, event: &Event) -> Event {
        let o = self.origin;
        let shift = |p: &Point| Point::new(p.x + o.x, p.y + o.y);
        match event {
            Event::MouseMove { pos } => Event::MouseMove { pos: shift(pos) },
            Event::MouseDown {
                pos,
                button,
                modifiers,
            } => Event::MouseDown {
                pos: shift(pos),
                button: *button,
                modifiers: *modifiers,
            },
            Event::MouseUp {
                pos,
                button,
                modifiers,
            } => Event::MouseUp {
                pos: shift(pos),
                button: *button,
                modifiers: *modifiers,
            },
            other => other.clone(),
        }
    }
}

impl Widget for ContextMenuHost {
    fn type_name(&self) -> &'static str {
        "ContextMenuHost"
    }

    fn id(&self) -> Option<&str> {
        Some(CONTEXT_MENU_ID)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        // No area of its own: the popup is a global overlay.
        self.bounds = Rect::new(bounds.x, bounds.y, 0.0, 0.0);
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, _available: Size) -> Size {
        self.open_requested();
        Size::new(0.0, 0.0)
    }

    fn hit_test(&self, _local_pos: Point) -> bool {
        false
    }

    fn has_active_modal(&self) -> bool {
        self.popup.is_some()
    }

    fn hit_test_global_overlay(&self, _local_pos: Point) -> bool {
        self.popup.is_some()
    }

    fn needs_draw(&self) -> bool {
        self.state.has_context_request()
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let (mut x, mut y) = (0.0, 0.0);
        agg_gui::widget::logical_root_transform(ctx).transform(&mut x, &mut y);
        self.origin = Point::new(x, y);
    }

    fn paint_global_overlay(&mut self, ctx: &mut dyn DrawCtx) {
        let font = Arc::clone(&self.font);
        let o = self.origin;
        if let Some((menu, _)) = self.popup.as_mut() {
            ctx.save();
            ctx.translate(-o.x, -o.y);
            menu.paint(ctx, font, FONT_SIZE, current_viewport());
            ctx.restore();
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let event = self.to_root(event);
        let Some((menu, _)) = self.popup.as_mut() else {
            return EventResult::Ignored;
        };
        let (result, response) = menu.handle_event(&event, current_viewport());
        match response {
            MenuResponse::Action(id) => {
                if let Some(action) = NodeAction::from_id(&id) {
                    self.choose(action);
                }
                self.popup = None;
                agg_gui::animation::request_draw();
                EventResult::Consumed
            }
            MenuResponse::Closed => {
                self.popup = None;
                agg_gui::animation::request_draw();
                EventResult::Consumed
            }
            MenuResponse::None => result,
        }
    }

    fn properties(&self) -> Vec<(&'static str, String)> {
        vec![
            ("open", self.is_open().to_string()),
            (
                "items",
                self.items()
                    .into_iter()
                    .map(|(l, _)| l)
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
        ]
    }
}
