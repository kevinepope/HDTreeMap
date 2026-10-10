//! `FolderTree`: the folder tree beside the map (`docs/PLAN.md` §2, view 2), an agg-gui
//! `TreeView` kept in step with `AppState`.
//!
//! Rows show a Font Awesome folder or file icon ([`icons`]), the name, the size in the
//! active size mode and the share of the parent folder; children are sorted by size and
//! added lazily, at most [`rows::CHILD_CAP`] per folder ([`rows`]). The `TreeView` does the
//! drawing, scrolling, selection and keyboard navigation; this wrapper links it to the other
//! views, once per layout:
//!
//! - **tree → state**: a row the user selects becomes `AppState::selected` (the map outlines
//!   it, the status bar describes it); the hovered row becomes `AppState::hovered` (the map
//!   outlines it); a double click or Enter on a folder row zooms the map to it.
//! - **state → tree**: a selection made elsewhere (the map) expands the folders above it,
//!   selects its row and scrolls the row into view; zooming the map expands the path down
//!   to the zoom root (the default expansion); a new tree version (size mode) rebuilds the
//!   rows with new sizes and order, keeping what was expanded.
//!
//! A `TreeView` has no select or scroll API and no change callbacks, so the wrapper reads
//! and writes its public node list and scrolls it with a wheel event. The wrapper also takes
//! the pointer and keyboard focus and forwards events to the `TreeView` (see
//! `claims_pointer_exclusively`).

pub mod icons;
pub mod rows;

use std::collections::HashSet;
use std::sync::Arc;

use agg_gui::event::current_click_count;
use agg_gui::{
    DrawCtx, Event, EventResult, HAnchor, Key, Modifiers, MouseButton, Point, Rect, Size, TreeView,
    VAnchor, Widget, WidgetBase,
};

use crate::fonts::Fonts;
use crate::model::NodeId;
use crate::state::{AppState, Hover};
use icons::TreeIcons;
use rows::Rows;

/// Widget id of the folder tree.
pub const FOLDER_TREE_ID: &str = "folder-tree";

/// Row height (logical pixels).
const ROW_HEIGHT: f64 = 22.0;
/// Text size of the rows.
const FONT_SIZE: f64 = 13.0;
/// Rows (`TreeView` nodes) past which collapsed folders' rows are dropped (or, when the open
/// folders alone need more, that many plus two folders' worth).
pub const PRUNE_AT: usize = 1500;
/// Pixels one wheel notch scrolls a `TreeView` (its `MouseWheel` handler's step).
const TREE_VIEW_WHEEL_STEP: f64 = 40.0;

/// See the module docs.
pub struct FolderTree {
    bounds: Rect,
    /// `[TreeView]`.
    children: Vec<Box<dyn Widget>>,
    base: WidgetBase,
    state: AppState,
    icons: TreeIcons,
    rows: Rows,
    /// Tree version the rows were built for.
    built_for: Option<u64>,
    /// Zoom root whose path was last expanded.
    zoom_synced: Option<NodeId>,
    /// The `AppState` selection the tree shows.
    selection_synced: Option<NodeId>,
    /// The `TreeView` row last seen (or made) selected.
    row_selected: Option<usize>,
    /// The `TreeView` row last seen hovered.
    row_hovered: Option<usize>,
    /// The hover this view put into `AppState` (cleared when the pointer leaves).
    hover_pushed: Option<Hover>,
    /// A double click landed on the tree: zoom to the selected folder after the `TreeView`
    /// selected the row under it.
    activate: bool,
    /// Row to scroll into view after the next `TreeView` layout.
    scroll_to: Option<usize>,
    /// Folders the tree opened itself to show a zoom root or selection (closed again when
    /// the next one is shown elsewhere).
    auto_expanded: HashSet<NodeId>,
    /// Rows right after the last rebuild (what the open folders need): pruning waits until
    /// collapsed folders hold a good many more than that.
    rebuilt_len: usize,
}

impl FolderTree {
    pub fn new(state: &AppState, fonts: &Fonts) -> Self {
        let view = TreeView::new(Arc::clone(&fonts.ui))
            .with_row_height(ROW_HEIGHT)
            .with_font_size(FONT_SIZE)
            .with_h_anchor(HAnchor::STRETCH)
            .with_v_anchor(VAnchor::STRETCH);
        Self {
            bounds: Rect::default(),
            children: vec![Box::new(view)],
            base: WidgetBase::new()
                .with_h_anchor(HAnchor::STRETCH)
                .with_v_anchor(VAnchor::STRETCH),
            state: state.clone(),
            icons: TreeIcons::new(&fonts.icons),
            rows: Rows::default(),
            built_for: None,
            zoom_synced: None,
            selection_synced: None,
            row_selected: None,
            row_hovered: None,
            hover_pushed: None,
            activate: false,
            scroll_to: None,
            auto_expanded: HashSet::new(),
            rebuilt_len: 0,
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

    /// How many rows (`TreeView` nodes, stand-ins included) exist: the lazy tree's size.
    pub fn node_count(&self) -> usize {
        self.view().map_or(0, |v| v.nodes.len())
    }

    /// The scanned node of the selected row.
    pub fn selected_node(&self) -> Option<NodeId> {
        let view = self.view()?;
        let idx = view.nodes.iter().position(|n| n.is_selected)?;
        self.rows.node_at(idx)
    }

    /// Whether `node` has a row and it is expanded.
    pub fn is_expanded(&self, node: NodeId) -> bool {
        let Some(&idx) = self.rows.index.get(&node) else {
            return false;
        };
        self.view()
            .and_then(|v| v.nodes.get(idx))
            .is_some_and(|n| n.is_expanded)
    }

    /// The label of `node`'s row.
    pub fn row_label(&self, node: NodeId) -> Option<String> {
        let idx = *self.rows.index.get(&node)?;
        Some(self.view()?.nodes.get(idx)?.label.clone())
    }

    /// The labels of the visible rows below `folder` (expanded), top first.
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

    /// `node`'s row in this widget's local (Y-up) coordinates, when it is visible.
    pub fn row_rect(&self, node: NodeId) -> Option<Rect> {
        let idx = *self.rows.index.get(&node)?;
        let view = self.view()?;
        let position = Rows::visible_position(&view.nodes, idx)?;
        let y = self.row_bottom(position)?;
        let rect = Rect::new(0.0, y, self.bounds.width, ROW_HEIGHT);
        (y >= 0.0 && y + ROW_HEIGHT <= self.bounds.height).then_some(rect)
    }

    /// The bottom of the row at visible `position` (local, Y-up), from where the
    /// `TreeView` placed its first row.
    fn row_bottom(&self, position: usize) -> Option<f64> {
        let first = self.view()?.children().first()?.bounds();
        let offset = first.y + ROW_HEIGHT - self.bounds.height;
        Some(self.bounds.height - (position as f64 + 1.0) * ROW_HEIGHT + offset)
    }

    /// Rebuild the rows when the tree changed (a new scan or size mode).
    fn sync_tree(&mut self) {
        let version = self.state.tree_version();
        if self.built_for == Some(version) {
            return;
        }
        let expanded = self.expanded_folders();
        self.rebuild(&expanded);
        self.built_for = Some(version);
        self.zoom_synced = None;
        self.auto_expanded.clear();
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
    /// (top down: a folder's children exist once its parent is populated). Rows of
    /// collapsed folders' contents are dropped. Row indices change, so the selection is
    /// looked up again on the next sync.
    fn rebuild(&mut self, expanded: &HashSet<NodeId>) {
        self.with_tree(|nodes, rows, tree, icons| {
            rows.reset(nodes, tree, icons);
            let mut stack = vec![tree.root()];
            while let Some(folder) = stack.pop() {
                if !expanded.contains(&folder) {
                    continue;
                }
                if let Some(&idx) = rows.index.get(&folder) {
                    rows.populate(nodes, tree, icons, idx);
                    nodes[idx].is_expanded = true;
                    stack.extend(tree.children(folder));
                }
            }
        });
        self.selection_synced = None;
        self.row_selected = None;
        self.row_hovered = None;
        self.rebuilt_len = self.node_count();
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
    /// root or the selection. Returns the folders on `paths` that are closed now, so the
    /// caller can record which ones the reveal opens.
    fn close_auto_expanded(&mut self, paths: &[NodeId]) -> Vec<NodeId> {
        let keep: HashSet<NodeId> = paths.iter().copied().collect();
        let close: Vec<usize> = self
            .auto_expanded
            .iter()
            .filter(|f| !keep.contains(f))
            .filter_map(|f| self.rows.index.get(f).copied())
            .collect();
        self.auto_expanded.retain(|f| keep.contains(f));
        if let Some(view) = self.view_mut() {
            for idx in close {
                if let Some(n) = view.nodes.get_mut(idx) {
                    n.is_expanded = false;
                }
            }
        }
        paths
            .iter()
            .copied()
            .filter(|&f| !self.is_expanded(f))
            .collect()
    }

    /// Remember which of `were_closed` are open now (the tree opened them).
    fn note_auto_expanded(&mut self, were_closed: Vec<NodeId>) {
        for f in were_closed {
            if self.is_expanded(f) {
                self.auto_expanded.insert(f);
            }
        }
    }

    /// Run `f` with the `TreeView`'s nodes, the rows and the finished tree.
    fn with_tree<R>(
        &mut self,
        f: impl FnOnce(
            &mut Vec<agg_gui::widgets::tree_view::TreeNode>,
            &mut Rows,
            &crate::model::Tree,
            &TreeIcons,
        ) -> R,
    ) -> Option<R> {
        let state = self.state.clone();
        let icons = self.icons.clone();
        let mut rows = std::mem::take(&mut self.rows);
        let out = match self.view_mut() {
            Some(view) => state.with_result(|r, _| f(&mut view.nodes, &mut rows, &r.tree, &icons)),
            None => None,
        };
        self.rows = rows;
        out
    }

    /// Carry what the user did in the `TreeView` (select, hover, activate) into the state.
    fn pull_from_view(&mut self) {
        let Some(view) = self.view() else {
            return;
        };
        // The selected row: a newly selected one, else the one we know if still selected.
        let selected: Vec<usize> = view
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.is_selected)
            .map(|(i, _)| i)
            .collect();
        let row = selected
            .iter()
            .copied()
            .find(|&i| Some(i) != self.row_selected)
            .or_else(|| self.row_selected.filter(|i| selected.contains(i)));
        let hovered = view.hovered_node_idx();

        if row != self.row_selected {
            self.row_selected = row;
            if let Some(node) = row.and_then(|i| self.rows.node_at(i)) {
                self.selection_synced = Some(node);
                self.state.set_selected(Some(node));
            } else if row.is_none() {
                self.selection_synced = None;
                self.state.set_selected(None);
            }
        }
        if hovered != self.row_hovered {
            self.row_hovered = hovered;
            let hover = hovered
                .and_then(|i| self.rows.node_at(i))
                .map(|node| Hover {
                    node,
                    aggregate: None,
                });
            self.push_hover(hover);
        }
        if std::mem::take(&mut self.activate) {
            self.activate_selected();
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

    /// Zoom the map to the selected row's folder. Returns whether it was a folder.
    fn activate_selected(&mut self) -> bool {
        let Some(node) = self.row_selected.and_then(|i| self.rows.node_at(i)) else {
            return false;
        };
        let is_dir = self
            .state
            .with_result(|r, _| r.tree.node(node).is_some_and(|n| n.is_dir()))
            .unwrap_or(false);
        if is_dir {
            self.state.set_zoom_root(node);
        }
        is_dir
    }

    /// Carry the state into the tree: the zoom path and a selection made elsewhere.
    fn push_from_state(&mut self) {
        let zoom = self.state.zoom_root();
        let selected = self.state.selected();
        if self.zoom_synced != Some(zoom) {
            self.zoom_synced = Some(zoom);
            let mut paths = self.path_to(Some(zoom));
            paths.extend(self.path_to(selected));
            let closed = self.close_auto_expanded(&paths);
            self.with_tree(|nodes, rows, tree, icons| rows.expand_to(nodes, tree, icons, zoom));
            self.note_auto_expanded(closed);
        }
        let budget = PRUNE_AT.max(self.rebuilt_len + 2 * rows::CHILD_CAP);
        if self.node_count() > budget {
            // Collapsed folders keep their rows until now; drop them so the TreeView,
            // whose layout cost grows with every row it holds, stays small.
            let expanded = self.expanded_folders();
            self.rebuild(&expanded);
        }
        if self.selection_synced == selected {
            return;
        }
        self.selection_synced = selected;
        let row = match selected {
            Some(node) => {
                let from_tree = self.row_selected.and_then(|i| self.rows.node_at(i)) == Some(node);
                let closed = if from_tree {
                    Vec::new()
                } else {
                    let mut paths = self.path_to(Some(zoom));
                    paths.extend(self.path_to(Some(node)).into_iter().skip(1));
                    self.close_auto_expanded(&paths)
                };
                let row = self
                    .with_tree(|nodes, rows, tree, icons| rows.reveal(nodes, tree, icons, node))
                    .flatten();
                self.note_auto_expanded(closed);
                row
            }
            None => None,
        };
        if let Some(view) = self.view_mut() {
            for (i, n) in view.nodes.iter_mut().enumerate() {
                n.is_selected = Some(i) == row;
            }
        }
        self.row_selected = row;
        self.scroll_to = row;
    }

    /// Scroll the `TreeView` so row `idx` is in view (centred when it was out of view).
    fn scroll_into_view(&mut self, idx: usize) {
        let height = self.bounds.height;
        let Some(view) = self.view() else {
            return;
        };
        let Some(position) = Rows::visible_position(&view.nodes, idx) else {
            return;
        };
        let Some(bottom) = self.row_bottom(position) else {
            return;
        };
        if bottom >= 0.0 && bottom + ROW_HEIGHT <= height {
            return;
        }
        // Moving the row up by `shift` pixels needs the scroll offset to grow by `shift`;
        // the TreeView's wheel handler subtracts `delta_y * step` from its offset.
        let shift = (height - ROW_HEIGHT) / 2.0 - bottom;
        let wheel = Event::MouseWheel {
            pos: Point::new(1.0, 1.0),
            delta_y: -shift / TREE_VIEW_WHEEL_STEP,
            delta_x: 0.0,
            modifiers: Modifiers::default(),
        };
        if let Some(view) = self.view_mut() {
            view.on_event(&wheel);
        }
    }
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

    fn layout(&mut self, available: Size) -> Size {
        let size = Size::new(available.width.max(0.0), available.height.max(0.0));
        self.bounds = Rect::new(0.0, 0.0, size.width, size.height);
        self.sync_tree();
        self.with_tree(|nodes, rows, tree, icons| rows.populate_expanded(nodes, tree, icons));
        self.pull_from_view();
        self.push_from_state();
        let local = Rect::new(0.0, 0.0, size.width, size.height);
        if let Some(view) = self.children.first_mut() {
            view.layout(size);
            view.set_bounds(local);
        }
        if let Some(idx) = self.scroll_to.take() {
            self.scroll_into_view(idx);
            if let Some(view) = self.children.first_mut() {
                view.layout(size);
                view.set_bounds(local);
            }
        }
        size
    }

    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {}

    fn is_focusable(&self) -> bool {
        true
    }

    /// The wrapper takes every pointer event and focus itself and hands them to its
    /// `TreeView`: a click on a row would otherwise land on the row's label, which cannot
    /// take focus, and the `TreeView` would never get the keyboard.
    fn claims_pointer_exclusively(&self, _local_pos: Point) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseLeave => {
                if let Some(view) = self.view_mut() {
                    view.clear_hover();
                }
                self.row_hovered = None;
                self.push_hover(None);
                return EventResult::Ignored;
            }
            Event::KeyDown {
                key: Key::Enter,
                modifiers,
            } if !(modifiers.ctrl || modifiers.meta || modifiers.alt) => {
                // Enter on a folder zooms (instead of the TreeView's expand toggle).
                self.pull_from_view();
                if self.activate_selected() {
                    return EventResult::Consumed;
                }
            }
            _ => {}
        }
        let double_click = matches!(
            event,
            Event::MouseDown {
                button: MouseButton::Left,
                ..
            }
        ) && current_click_count() >= 2;
        let result = match self.view_mut() {
            Some(view) => view.on_event(event),
            None => EventResult::Ignored,
        };
        if double_click {
            // The TreeView selected the row under the press; zoom on the next layout.
            self.activate = true;
            agg_gui::animation::request_draw();
        }
        result
    }
}
