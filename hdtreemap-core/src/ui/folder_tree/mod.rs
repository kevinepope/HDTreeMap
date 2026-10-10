//! `FolderTree`: the folder tree beside the map (`docs/PLAN.md` §2, view 2), an agg-gui
//! `TreeView` kept in step with `AppState`.
//!
//! Rows show a Font Awesome folder or file glyph, the name, the size in the active size mode
//! (the row's secondary text) and the share of the parent folder (its fraction bar);
//! children are sorted by size and added lazily, at most [`rows::CHILD_CAP`] per folder
//! ([`rows`]). The `TreeView` does the drawing, scrolling, selection, focus and keyboard
//! navigation; this wrapper links it to the other views, once per layout:
//!
//! - **tree → state**: the `TreeView`'s events ([`TreeViewEvent`]) are drained: a folder
//!   the user expands gets its children; a selection becomes `AppState::selected` (the map
//!   outlines it, the status bar describes it); a double click or Enter on a folder
//!   (`Activated`) zooms the map to it. The hovered row becomes `AppState::hovered`.
//! - **state → tree**: a selection made elsewhere (the map) opens the folders above it,
//!   selects its row (`select_single`) and scrolls it into view
//!   (`scroll_node_into_view`); zooming the map opens the path down to the zoom root; a new
//!   tree version (size mode, Move to Trash, a rescan, a live tick of a running scan)
//!   rebuilds the rows with new sizes and order, keeping what was open. A live tick does
//!   not scroll the selection back into view, so the user can scroll while the scan runs.
//!
//! The tree stays small: folders it opened on its own to show an earlier zoom root or
//! selection are closed again, and their rows dropped (`remove_children`), when the next
//! one is shown elsewhere. Folders the user opened are left alone.
//!
//! The root row is labelled with the drive's name or the scanned folder's own name
//! ([`rows::root_label`]); hovering it shows the full path as a tooltip.
//!
//! A right click on a row selects it and opens the item's context menu
//! ([`FolderTree::node_at`] finds the row from the `TreeView`'s own row geometry, so it is
//! right however far the tree is scrolled).

pub mod rows;

use std::collections::HashSet;
use std::sync::Arc;

use agg_gui::widgets::tree_view::TreeNode;
use agg_gui::{
    DrawCtx, Event, EventResult, HAnchor, MouseButton, Point, Rect, ScrollAlign, Size, TreeView,
    TreeViewEvent, VAnchor, Widget, WidgetBase,
};
use web_time::Instant;

use crate::fonts::Fonts;
use crate::model::{NodeId, Tree};
use crate::state::{AppState, Hover};
use rows::Rows;

/// Widget id of the folder tree.
pub const FOLDER_TREE_ID: &str = "folder-tree";

/// Row height (logical pixels).
const ROW_HEIGHT: f64 = 22.0;
/// Text size of the rows.
const FONT_SIZE: f64 = 13.0;

/// What a row shows (see [`FolderTree::row`]).
#[derive(Clone, Debug, PartialEq)]
pub struct RowText {
    /// The name.
    pub label: String,
    /// The size in the active mode.
    pub secondary: Option<String>,
    /// The share of the parent folder (none for the scan root).
    pub fraction: Option<f32>,
}

/// See the module docs.
pub struct FolderTree {
    bounds: Rect,
    /// `[TreeView]`.
    children: Vec<Box<dyn Widget>>,
    base: WidgetBase,
    state: AppState,
    rows: Rows,
    /// Tree version the rows were built for.
    built_for: Option<u64>,
    /// Zoom root whose path was last opened.
    zoom_synced: Option<NodeId>,
    /// The `AppState` selection the tree shows.
    selection_synced: Option<NodeId>,
    /// Whether showing the selection again scrolls to it (not after a live tick's rebuild).
    scroll_to_selection: bool,
    /// The `TreeView` row last seen hovered.
    row_hovered: Option<usize>,
    /// The hover this view put into `AppState` (cleared when the pointer leaves).
    hover_pushed: Option<Hover>,
    /// Folders the tree opened itself to show a zoom root or selection (closed again when
    /// the next one is shown elsewhere).
    auto_expanded: HashSet<NodeId>,
    /// This widget's (0, 0) in root coordinates (logical, Y up), from the last paint.
    origin: Point,
}

impl FolderTree {
    pub fn new(state: &AppState, fonts: &Fonts) -> Self {
        let view = TreeView::new(Arc::clone(&fonts.ui))
            .with_icon_font(Arc::clone(&fonts.icons))
            .with_row_height(ROW_HEIGHT)
            .with_font_size(FONT_SIZE)
            // Enter zooms (Activated), it does not toggle the folder.
            .with_enter_toggles_expansion(false)
            .with_h_anchor(HAnchor::STRETCH)
            .with_v_anchor(VAnchor::STRETCH);
        Self {
            bounds: Rect::default(),
            children: vec![Box::new(view)],
            base: WidgetBase::new()
                .with_h_anchor(HAnchor::STRETCH)
                .with_v_anchor(VAnchor::STRETCH),
            state: state.clone(),
            rows: Rows::default(),
            built_for: None,
            zoom_synced: None,
            selection_synced: None,
            scroll_to_selection: true,
            row_hovered: None,
            hover_pushed: None,
            auto_expanded: HashSet::new(),
            origin: Point::ORIGIN,
        }
    }

    fn view(&self) -> Option<&TreeView> {
        self.children.first()?.as_any()?.downcast_ref::<TreeView>()
    }

    fn view_mut(&mut self) -> Option<&mut TreeView> {
        self.children
            .first_mut()?
            .as_any_mut()?
            .downcast_mut::<TreeView>()
    }

    /// How many rows (`TreeView` nodes) exist: the lazy tree's size.
    pub fn node_count(&self) -> usize {
        self.view().map_or(0, |v| v.nodes.len())
    }

    /// How far the `TreeView` is scrolled down (logical pixels).
    pub fn scroll_offset(&self) -> f64 {
        self.view().map_or(0.0, TreeView::scroll_offset)
    }

    /// The scanned node of the selected row.
    pub fn selected_node(&self) -> Option<NodeId> {
        let idx = *self.view()?.selected_nodes().first()?;
        self.rows.node_at(idx)
    }

    /// Whether `node` has a row and it is expanded.
    pub fn is_expanded(&self, node: NodeId) -> bool {
        let Some(&idx) = self.rows.index.get(&node) else {
            return false;
        };
        self.view().is_some_and(|v| v.is_expanded(idx))
    }

    /// `node`'s row as shown: its label (the name), secondary text (the size) and fraction
    /// bar (the share of its folder).
    pub fn row(&self, node: NodeId) -> Option<RowText> {
        let idx = *self.rows.index.get(&node)?;
        let n = self.view()?.nodes.get(idx)?;
        Some(RowText {
            label: n.label.clone(),
            secondary: n.secondary_text.clone(),
            fraction: n.fraction,
        })
    }

    /// The label of `node`'s row (its name).
    pub fn row_label(&self, node: NodeId) -> Option<String> {
        self.row(node).map(|r| r.label)
    }

    /// The labels of the rows below `folder` (expanded), top first.
    pub fn child_labels(&self, folder: NodeId) -> Vec<String> {
        let (Some(view), Some(&parent)) = (self.view(), self.rows.index.get(&folder)) else {
            return Vec::new();
        };
        let mut kids: Vec<_> = view
            .nodes
            .iter()
            .filter(|n| n.parent == Some(parent))
            .collect();
        kids.sort_by_key(|n| n.order);
        kids.into_iter().map(|n| n.label.clone()).collect()
    }

    /// `node`'s row in this widget's local (Y-up) coordinates, when it is in view.
    pub fn row_rect(&self, node: NodeId) -> Option<Rect> {
        let idx = *self.rows.index.get(&node)?;
        let view = self.view()?;
        let position = Rows::visible_position(&view.nodes, idx)?;
        let y = self.bounds.height - (position as f64 + 1.0) * ROW_HEIGHT + view.scroll_offset();
        let rect = Rect::new(0.0, y, self.bounds.width, ROW_HEIGHT);
        (y >= 0.0 && y + ROW_HEIGHT <= self.bounds.height).then_some(rect)
    }

    /// The scanned node whose row is under `pos` (local, Y-up), from the `TreeView`'s row
    /// geometry at its current scroll offset.
    pub fn node_at(&mut self, pos: Point) -> Option<NodeId> {
        if pos.y < 0.0 || pos.y > self.bounds.height {
            return None;
        }
        let view = self.view_mut()?;
        let from_top = self_height(view) - pos.y + view.scroll_offset();
        if from_top < 0.0 {
            return None;
        }
        let row = (from_top / ROW_HEIGHT).floor() as usize;
        let idx = view.node_at_row(row)?;
        let rect = view.node_row_rect(idx)?;
        let inside = pos.y >= rect.y && pos.y <= rect.y + rect.height;
        if !inside {
            return None;
        }
        self.rows.node_at(idx)
    }

    /// Run `f` with the `TreeView`'s nodes, the rows and the finished tree.
    fn with_tree<R>(
        &mut self,
        f: impl FnOnce(&mut Vec<TreeNode>, &mut Rows, &Tree) -> R,
    ) -> Option<R> {
        let state = self.state.clone();
        let mut rows = std::mem::take(&mut self.rows);
        let out = match self.view_mut() {
            Some(view) => state.with_result(|r, _| f(&mut view.nodes, &mut rows, r.tree)),
            None => None,
        };
        self.rows = rows;
        out
    }

    /// Rebuild the rows when the tree changed (a new scan, size mode or edit).
    fn sync_tree(&mut self) {
        let version = self.state.tree_version();
        if self.built_for == Some(version) {
            return;
        }
        let started = Instant::now();
        let expanded = self.expanded_folders();
        self.rebuild(&expanded);
        self.scroll_to_selection = !self.state.is_live();
        self.built_for = Some(version);
        self.zoom_synced = None;
        self.auto_expanded.clear();
        self.state.add_live_work(started.elapsed());
    }

    /// The scanned folders whose rows are expanded.
    fn expanded_folders(&self) -> HashSet<NodeId> {
        match self.view() {
            Some(view) => view
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.is_expanded)
                .filter_map(|(i, _)| self.rows.node_at(i))
                .collect(),
            None => HashSet::new(),
        }
    }

    /// Build the rows again from the scan root, opening exactly the folders in `expanded`
    /// (top down: a folder's children exist once its parent is populated). Row indices
    /// change, so the selection is looked up again on the next sync.
    fn rebuild(&mut self, expanded: &HashSet<NodeId>) {
        if let Some(view) = self.view_mut() {
            view.clear_selection();
            view.set_cursor_node(None);
            view.take_events();
        }
        let volume = self
            .state
            .screen()
            .target()
            .and_then(|t| t.volume_name.clone());
        self.with_tree(|nodes, rows, tree| {
            rows.reset(nodes, tree, &rows::root_label(tree, volume.as_deref()));
            let mut stack = vec![tree.root()];
            while let Some(folder) = stack.pop() {
                if !expanded.contains(&folder) {
                    continue;
                }
                if let Some(&idx) = rows.index.get(&folder) {
                    rows.populate(nodes, tree, idx);
                    nodes[idx].is_expanded = true;
                    stack.extend(tree.children(folder));
                }
            }
        });
        self.selection_synced = None;
        self.row_hovered = None;
        self.base.tooltip = None;
    }

    /// `node` and the folders above it.
    fn path_to(&self, node: Option<NodeId>) -> Vec<NodeId> {
        let Some(node) = node else {
            return Vec::new();
        };
        self.state
            .with_result(|r, _| {
                std::iter::once(node)
                    .chain(r.tree.ancestors(node))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Before revealing a new zoom root or selection: close the folders the tree opened on
    /// its own for earlier ones (not those the user opened), unless they lead to the zoom
    /// root or the selection, and drop their rows. Returns the folders on `paths` that are
    /// closed now, so the caller can record which ones the reveal opens.
    fn close_auto_expanded(&mut self, paths: &[NodeId]) -> Vec<NodeId> {
        let keep: HashSet<NodeId> = paths.iter().copied().collect();
        let close: Vec<NodeId> = self
            .auto_expanded
            .iter()
            .copied()
            .filter(|f| !keep.contains(f))
            .collect();
        self.auto_expanded.retain(|f| keep.contains(f));
        for folder in close {
            self.prune(folder);
        }
        paths
            .iter()
            .copied()
            .filter(|&f| !self.is_expanded(f))
            .collect()
    }

    /// Collapse `folder` and drop its rows (they are added again when it opens).
    fn prune(&mut self, folder: NodeId) {
        let Some(&idx) = self.rows.index.get(&folder) else {
            return;
        };
        let Some(map) = self.view_mut().map(|v| v.remove_children(idx)) else {
            return;
        };
        self.with_tree(|nodes, rows, tree| rows.remap(nodes, tree, idx, &map));
        self.row_hovered = self.row_hovered.and_then(|r| map.get(r).copied().flatten());
    }

    /// Remember which of `were_closed` are open now (the tree opened them).
    fn note_auto_expanded(&mut self, were_closed: Vec<NodeId>) {
        for f in were_closed {
            if self.is_expanded(f) {
                self.auto_expanded.insert(f);
            }
        }
    }

    /// Carry what the user did in the `TreeView` (expand, select, activate, hover) into the
    /// rows and the state.
    fn pull_from_view(&mut self) {
        let events = self
            .view_mut()
            .map(TreeView::take_events)
            .unwrap_or_default();
        for event in events {
            match event {
                TreeViewEvent::Expanded(idx) => {
                    self.with_tree(|nodes, rows, tree| rows.populate(nodes, tree, idx));
                    if let Some(node) = self.rows.node_at(idx) {
                        // The user opened it: it is theirs, never auto-closed.
                        self.auto_expanded.remove(&node);
                    }
                }
                TreeViewEvent::Collapsed(idx) => {
                    if let Some(node) = self.rows.node_at(idx) {
                        self.auto_expanded.remove(&node);
                    }
                }
                TreeViewEvent::SelectionChanged { .. } => self.selection_from_view(),
                TreeViewEvent::Activated(idx) => self.activate(idx),
            }
        }
        let hovered = self.view().and_then(TreeView::hovered_node_idx);
        if hovered != self.row_hovered {
            self.row_hovered = hovered;
            self.base.tooltip = self.root_tooltip(hovered);
            let hover = hovered
                .and_then(|i| self.rows.node_at(i))
                .map(|node| Hover {
                    node,
                    aggregate: None,
                });
            self.push_hover(hover);
        }
    }

    /// The full scanned path, when row `row` is the root's (its label is a short name).
    fn root_tooltip(&self, row: Option<usize>) -> Option<String> {
        let node = self.rows.node_at(row?)?;
        self.state
            .with_result(|r, _| {
                (node == r.tree.root()).then(|| r.tree.path(node).to_string_lossy().into_owned())
            })
            .flatten()
    }

    /// The user selected a row: make its node the app's selection ("more" rows have none).
    fn selection_from_view(&mut self) {
        let row = self
            .view()
            .and_then(|v| v.selected_nodes().first().copied());
        match row {
            Some(idx) => {
                if let Some(node) = self.rows.node_at(idx) {
                    self.selection_synced = Some(node);
                    self.state.set_selected(Some(node));
                }
            }
            None => {
                self.selection_synced = None;
                self.state.set_selected(None);
            }
        }
    }

    /// A double click or Enter on row `idx`: zoom the map to it if it is a folder.
    fn activate(&mut self, idx: usize) {
        let Some(node) = self.rows.node_at(idx) else {
            return;
        };
        let is_dir = self
            .state
            .with_result(|r, _| r.tree.node(node).is_some_and(|n| n.is_dir()))
            .unwrap_or(false);
        if is_dir {
            self.state.set_zoom_root(node);
        }
    }

    /// Show `hover` on the other views (clearing only a hover this view set).
    fn push_hover(&mut self, hover: Option<Hover>) {
        match hover {
            Some(h) => {
                self.hover_pushed = Some(h);
                self.state.set_hovered(Some(h));
            }
            None => {
                if let Some(mine) = self.hover_pushed.take() {
                    if self.state.hovered() == Some(mine) {
                        self.state.set_hovered(None);
                    }
                }
            }
        }
    }

    /// Carry the state into the tree: the zoom path and a selection made elsewhere.
    fn push_from_state(&mut self) {
        let scroll = std::mem::replace(&mut self.scroll_to_selection, true);
        let zoom = self.state.zoom_root();
        let selected = self.state.selected();
        if self.zoom_synced != Some(zoom) {
            self.zoom_synced = Some(zoom);
            let mut paths = self.path_to(Some(zoom));
            paths.extend(self.path_to(selected));
            let closed = self.close_auto_expanded(&paths);
            self.with_tree(|nodes, rows, tree| rows.expand_to(nodes, tree, zoom));
            self.note_auto_expanded(closed);
        }
        if self.selection_synced == selected {
            return;
        }
        self.selection_synced = selected;
        let row = match selected {
            Some(node) => {
                let from_tree = self.selected_node() == Some(node);
                let closed = if from_tree {
                    Vec::new()
                } else {
                    let mut paths = self.path_to(Some(zoom));
                    paths.extend(self.path_to(Some(node)).into_iter().skip(1));
                    self.close_auto_expanded(&paths)
                };
                let row = self
                    .with_tree(|nodes, rows, tree| rows.reveal(nodes, tree, node))
                    .flatten();
                self.note_auto_expanded(closed);
                row
            }
            None => None,
        };
        if let Some(view) = self.view_mut() {
            match row {
                Some(idx) => {
                    view.select_single(idx);
                    if scroll {
                        view.scroll_node_into_view(idx, ScrollAlign::Center);
                    }
                }
                None => view.clear_selection(),
            }
        }
    }
}

/// The `TreeView`'s viewport height (its bounds; rows are positioned from its top).
fn self_height(view: &TreeView) -> f64 {
    view.bounds().height
}

impl Widget for FolderTree {
    fn type_name(&self) -> &'static str {
        "FolderTree"
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
        self.bounds = bounds;
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }

    fn widget_base_mut(&mut self) -> Option<&mut WidgetBase> {
        Some(&mut self.base)
    }

    fn h_anchor(&self) -> HAnchor {
        self.base.h_anchor
    }

    fn v_anchor(&self) -> VAnchor {
        self.base.v_anchor
    }

    /// The hovered row: its tip (the root's path) belongs to that row only.
    fn tooltip_key(&self) -> Option<u64> {
        self.row_hovered.map(|r| r as u64)
    }

    fn layout(&mut self, available: Size) -> Size {
        let size = Size::new(available.width.max(0.0), available.height.max(0.0));
        self.bounds = Rect::new(0.0, 0.0, size.width, size.height);
        self.sync_tree();
        self.pull_from_view();
        self.push_from_state();
        let local = Rect::new(0.0, 0.0, size.width, size.height);
        if let Some(view) = self.children.first_mut() {
            view.layout(size);
            view.set_bounds(local);
        }
        size
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let (mut x, mut y) = (0.0, 0.0);
        agg_gui::widget::logical_root_transform(ctx).transform(&mut x, &mut y);
        self.origin = Point::new(x, y);
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseLeave => {
                if let Some(view) = self.view_mut() {
                    view.clear_hover();
                }
                self.row_hovered = None;
                self.base.tooltip = None;
                self.push_hover(None);
                EventResult::Ignored
            }
            Event::MouseDown {
                pos,
                button: MouseButton::Right,
                ..
            } => {
                // The TreeView ignores right clicks; they reach the wrapper.
                let Some(node) = self.node_at(*pos) else {
                    return EventResult::Ignored;
                };
                self.state.set_selected(Some(node));
                let at = Point::new(self.origin.x + pos.x, self.origin.y + pos.y);
                self.state.request_context_menu(at, node);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }
}
