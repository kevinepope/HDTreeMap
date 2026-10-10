//! `TreemapView`: the custom widget that shows the scan as a cushion treemap
//! (`docs/PLAN.md` §3.4–3.5), the one widget the app draws itself. While a scan runs it
//! shows the tree so far, laid out again on each live tick (`state/live.rs`).
//!
//! It keeps the laid-out map ([`TreemapLayout`]) and its raster (`Arc<Vec<u8>>`, physical
//! pixels, top row first) and draws the raster with `draw_image_rgba_arc`, so agg-gui's
//! GPU backend keeps the texture while the `Arc` is unchanged. It lays out again only when
//! the physical size (window size × DPI scale), the zoom root or the tree (new scan, size
//! mode) changes, and rasterises again only then or when the shading or the palette (the
//! colour scheme) changes. Hover, selection and the folder outline are vector strokes
//! drawn over the image, so moving the mouse never re-rasterises. While a scan runs, a
//! live tick's new layout is rasterised on the following frame (the old picture stays up
//! one frame longer), so layout and raster never land in the same frame.
//!
//! Coordinates: agg-gui hands the widget Y-up logical points; the layout and raster are in
//! Y-down physical pixels, capped in total (`geometry.rs`). `to_map` / `rect_to_local`
//! convert between the two.
//!
//! The colours are the shared [`AppState::palette`], so the extension legend's swatches
//! match. The outlines follow `AppState`, not just this widget's pointer: an item hovered or
//! selected in the folder tree is outlined here too (or, when it is too small to have its
//! own rectangle, its nearest folder that has one). While an extension is highlighted the
//! map is drawn from a second, dimmed raster (`highlight.rs`).
//!
//! Input: hover updates `AppState::hovered` (the status bar shows it) and the multi-line
//! tooltip, keyed on the hovered rectangle (`tooltip_key`) so each rectangle gets its own
//! hover delay;
//! a click selects; a right click selects and opens the item's context menu (through
//! `AppState::request_context_menu`, at the click in root coordinates); a double click
//! zooms into the folder (or a file's folder); Backspace or
//! the mouse back button zooms out; Escape clears the extension highlight, else the
//! selection.

mod geometry;
mod highlight;

use std::rc::Rc;
use std::sync::Arc;

use web_time::Instant;

use agg_gui::event::current_click_count;
use agg_gui::ux_scale::effective_scale;
use agg_gui::{
    Color, DrawCtx, Event, EventResult, HAnchor, Key, Modifiers, MouseButton, Point, Rect, Size,
    VAnchor, Widget, WidgetBase,
};

use super::describe;
use crate::layout::{layout, LayoutParams, PxRect, RectKind, TreemapLayout};
use crate::model::{NodeId, Tree};
use crate::render::{rasterise, stats_enabled, Palette, Shading};
use crate::state::{AppState, Hover};
use highlight::Highlight;

/// Widget id of the treemap.
pub const TREEMAP_ID: &str = "treemap";

/// Outline of the hovered item.
const HOVER_COLOR: Color = Color::rgb(1.0, 0.93, 0.35);
/// Outline of the hovered item's folder.
const FOLDER_COLOR: Color = Color::rgba(1.0, 1.0, 1.0, 0.55);
/// Outline of the selected item.
const SELECTED_COLOR: Color = Color::rgb(0.25, 0.85, 1.0);
/// Hover and selection outline width (logical pixels).
const OUTLINE: f64 = 2.0;
/// Folder outline width (logical pixels).
const FOLDER_OUTLINE: f64 = 1.0;
/// Mouse button agg-gui's shells report for the "back" side button.
const BACK_BUTTON: MouseButton = MouseButton::Other(3);

/// What a layout depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LayoutKey {
    width: u32,
    height: u32,
    zoom_root: NodeId,
    tree_version: u64,
}

/// The current map: layout, colours and raster.
struct MapCache {
    key: LayoutKey,
    shading: Shading,
    layout: TreemapLayout,
    palette: Rc<Palette>,
    pixels: Arc<Vec<u8>>,
}

/// A node looked up in the current layout: `(node, its rectangle or nearest folder's)`.
type Located = Option<(NodeId, Option<usize>)>;

/// The treemap widget. See the module docs.
pub struct TreemapView {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    base: WidgetBase,
    state: AppState,
    params: LayoutParams,
    cache: Option<MapCache>,
    /// Painted rectangle under the pointer.
    hover_index: Option<usize>,
    /// Where the pointer last was (local, Y-up), to re-hit-test after a re-layout.
    pointer: Option<Point>,
    /// The selection in the current layout.
    selected_index: Located,
    /// A hover set by another view (the folder tree) in the current layout.
    linked_hover_index: Located,
    /// The highlighted extension's files in the current layout.
    highlight: Highlight,
    /// A live tick's layout, rasterised (and shown) on the next frame.
    pending: Option<(LayoutKey, TreemapLayout)>,
    /// This widget's (0, 0) in root coordinates (logical, Y up), from the last paint: where
    /// a right click's context menu opens.
    origin: Point,
}

impl TreemapView {
    pub fn new(state: &AppState) -> Self {
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            base: WidgetBase::new()
                .with_h_anchor(HAnchor::STRETCH)
                .with_v_anchor(VAnchor::STRETCH),
            state: state.clone(),
            params: LayoutParams::default(),
            cache: None,
            hover_index: None,
            pointer: None,
            selected_index: None,
            linked_hover_index: None,
            highlight: Highlight::default(),
            pending: None,
            origin: Point::ORIGIN,
        }
    }

    /// How many file rectangles the extension highlight showed in the last paint (a hook
    /// for tests and `HDTREEMAP_STATS`).
    pub fn highlighted_rects(&self) -> usize {
        self.highlight.painted()
    }

    /// Bring the layout and raster up to date for the current size and state.
    fn refresh(&mut self, scale: f64) {
        let (width, height) = geometry::raster_size(self.bounds.width, self.bounds.height, scale);
        let key = LayoutKey {
            width,
            height,
            zoom_root: self.state.zoom_root(),
            tree_version: self.state.tree_version(),
        };
        let shading = self.state.shading();
        let Some(shared_palette) = self.state.palette() else {
            self.drop_map();
            return;
        };
        let fresh_layout = self.cache.as_ref().is_some_and(|c| c.key == key);
        let fresh_look = self
            .cache
            .as_ref()
            .is_some_and(|c| c.shading == shading && Rc::ptr_eq(&c.palette, &shared_palette));
        if fresh_layout && fresh_look {
            return;
        }
        if width == 0 || height == 0 {
            self.drop_map();
            return;
        }
        let refresh_started = Instant::now();
        let previous = self.cache.take();
        let pending = self
            .pending
            .take()
            .filter(|(k, _)| *k == key)
            .map(|(_, m)| m);
        let state = self.state.clone();
        let params = self.params;
        if pending.is_none() && !fresh_layout && previous.is_some() && state.is_live() {
            // A live tick: lay out now, rasterise on the next frame, so the two never add
            // up in one frame (the old picture stays meanwhile; `state/live.rs`).
            self.pending = state.with_result(|r, _| (key, lay_out(&state, r.tree, key, &params)));
            self.cache = previous;
            self.state.add_live_work(refresh_started.elapsed());
            agg_gui::animation::request_draw();
            return;
        }
        let next = state.with_result(|result, _| {
            let tree = result.tree;
            let map = match (previous, pending) {
                (Some(c), _) if fresh_layout => c.layout,
                (_, Some(map)) => map,
                _ => lay_out(&state, tree, key, &params),
            };
            let palette = shared_palette;
            let started = Instant::now();
            let pixels = rasterise(&map, tree, &palette, shading, width, height);
            state.count_raster();
            if stats_enabled() {
                crate::render::stats_line(&format!(
                    "raster {:.1} ms | {width}x{height} | {shading:?}",
                    ms(started)
                ));
            }
            MapCache {
                key,
                shading,
                layout: map,
                palette,
                pixels: Arc::new(pixels),
            }
        });
        let relaid = !fresh_layout;
        self.cache = next;
        // While a scan runs, this is part of the live tick's refresh (`state/live.rs`).
        self.state.add_live_work(refresh_started.elapsed());
        if relaid {
            self.forget_lookups();
            self.rehover();
        }
    }

    fn drop_map(&mut self) {
        self.cache = None;
        self.hover_index = None;
        self.forget_lookups();
    }

    /// The layout changed: every rectangle index looked up in the old one is stale.
    fn forget_lookups(&mut self) {
        self.selected_index = None;
        self.linked_hover_index = None;
        self.highlight.forget();
    }

    /// Hit-test the last pointer position again (the map under it changed).
    fn rehover(&mut self) {
        self.hover_index = None;
        match self.pointer {
            Some(pos) => self.hover_at(pos),
            None => self.state.set_hovered(None),
        }
    }

    /// A local (Y-up, logical) point in map pixels (Y-down, physical).
    fn to_map(&self, pos: Point) -> Option<(f64, f64)> {
        let cache = self.cache.as_ref()?;
        let raster = (cache.key.width, cache.key.height);
        geometry::to_raster(pos, raster, self.local_bounds())
    }

    /// A map rectangle as a local (Y-up, logical) rectangle.
    fn rect_to_local(&self, r: PxRect) -> Option<Rect> {
        let cache = self.cache.as_ref()?;
        let raster = (cache.key.width, cache.key.height);
        geometry::to_local(r, raster, self.local_bounds())
    }

    /// The widget's own rectangle in its local coordinates.
    fn local_bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.bounds.width, self.bounds.height)
    }

    /// Whether the cached layout shows the current zoom root and tree. Between a zoom (or a
    /// tree edit) and the next paint it does not, and its rectangles name the wrong nodes.
    fn layout_is_current(&self) -> bool {
        self.cache.as_ref().is_some_and(|c| {
            c.key.zoom_root == self.state.zoom_root()
                && c.key.tree_version == self.state.tree_version()
        })
    }

    /// Whether `node` is still in the tree (not removed by Move to Trash or a rescan).
    fn is_live(&self, node: NodeId) -> bool {
        self.state
            .with_result(|r, _| r.tree.is_live(node))
            .unwrap_or(false)
    }

    /// Update the hover from a pointer at `pos` (local, Y-up). With a stale layout nothing
    /// is hovered until the next paint lays out again (which re-hit-tests the pointer).
    fn hover_at(&mut self, pos: Point) {
        self.pointer = Some(pos);
        if !self.layout_is_current() {
            if self.hover_index.take().is_some() {
                self.base.tooltip = None;
            }
            agg_gui::animation::request_draw();
            return;
        }
        let index = self
            .to_map(pos)
            .and_then(|(x, y)| self.cache.as_ref()?.layout.hit(x, y));
        if index == self.hover_index {
            return;
        }
        self.hover_index = index;
        let hover = index.and_then(|i| self.hover_for(i));
        let tooltip = hover.and_then(|h| {
            self.state
                .with_result(|r, _| describe::tooltip(r.tree, self.state.zoom_root(), h))
        });
        self.base.tooltip = tooltip;
        self.state.set_hovered(hover);
        agg_gui::animation::request_draw();
    }

    fn clear_hover(&mut self) {
        self.pointer = None;
        if self.hover_index.take().is_some() || self.state.hovered().is_some() {
            self.base.tooltip = None;
            self.state.set_hovered(None);
            agg_gui::animation::request_draw();
        }
    }

    /// What painted rectangle `index` stands for.
    fn hover_for(&self, index: usize) -> Option<Hover> {
        let map = &self.cache.as_ref()?.layout;
        let r = map.rects().get(index)?;
        Some(Hover {
            node: r.node,
            aggregate: map.aggregate(index),
        })
    }

    /// The folder a double click on painted rectangle `index` zooms into.
    fn zoom_target(&self, index: usize) -> Option<NodeId> {
        let map = &self.cache.as_ref()?.layout;
        let r = map.rects().get(index)?;
        match r.kind {
            RectKind::File => map.ancestors(index).next().map(|p| map.rects()[p].node),
            RectKind::Dir | RectKind::FolderBlock | RectKind::Aggregate => Some(r.node),
        }
    }

    fn press(&mut self, pos: Point) {
        self.hover_at(pos);
        if !self.layout_is_current() {
            // The map under the pointer is about to change; a press on the old one is
            // dropped rather than acting on a node the user no longer sees.
            return;
        }
        let Some(index) = self.hover_index else {
            self.state.set_selected(None);
            return;
        };
        if current_click_count() >= 2 {
            if let Some(target) = self.zoom_target(index) {
                self.state.set_zoom_root(target);
            }
            return;
        }
        let node = self.hover_for(index).map(|h| h.node);
        self.state.set_selected(node);
        agg_gui::animation::request_draw();
    }

    /// A right click at `pos`: select what is under it and ask for its context menu.
    fn context_menu_at(&mut self, pos: Point) {
        self.hover_at(pos);
        if !self.layout_is_current() {
            return;
        }
        let Some(node) = self
            .hover_index
            .and_then(|i| self.hover_for(i))
            .map(|h| h.node)
        else {
            return;
        };
        self.state.set_selected(Some(node));
        let at = Point::new(self.origin.x + pos.x, self.origin.y + pos.y);
        self.state.request_context_menu(at, node);
    }

    /// The rectangle showing `node` (or its nearest laid-out folder), looked up once per
    /// change through `slot`.
    fn locate(&self, slot: &mut Located, node: NodeId) -> Option<usize> {
        if let Some((cached, index)) = *slot {
            if cached == node {
                return index;
            }
        }
        let map = &self.cache.as_ref()?.layout;
        let index = self
            .state
            .with_result(|r, _| map.index_of_nearest(r.tree, node))
            .flatten();
        *slot = Some((node, index));
        index
    }

    /// The selection's rectangle in the current layout.
    fn selected_rect(&mut self) -> Option<PxRect> {
        let selected = self.state.selected().filter(|&n| self.is_live(n))?;
        let mut slot = self.selected_index;
        let index = self.locate(&mut slot, selected);
        self.selected_index = slot;
        let map = &self.cache.as_ref()?.layout;
        index.and_then(|i| map.rects().get(i)).map(|r| r.rect)
    }

    /// The rectangle to outline as hovered: the one under this widget's pointer, or the one
    /// showing what another view (the folder tree) hovers.
    fn shown_hover_index(&mut self) -> Option<usize> {
        let hover = self.state.hovered().filter(|h| self.is_live(h.node))?;
        if let Some(own) = self.hover_index {
            if self.hover_for(own) == Some(hover) {
                return Some(own);
            }
        }
        let mut slot = self.linked_hover_index;
        let index = self.locate(&mut slot, hover.node);
        self.linked_hover_index = slot;
        index
    }

    fn stroke(&self, ctx: &mut dyn DrawCtx, r: PxRect, color: Color, width: f64) {
        let Some(local) = self.rect_to_local(r) else {
            return;
        };
        // Keep the line inside the rectangle when it fits.
        let inset = if local.width > width * 2.0 && local.height > width * 2.0 {
            width / 2.0
        } else {
            0.0
        };
        ctx.begin_path();
        ctx.rect(
            local.x + inset,
            local.y + inset,
            (local.width - 2.0 * inset).max(0.0),
            (local.height - 2.0 * inset).max(0.0),
        );
        ctx.set_stroke_color(color);
        ctx.set_line_width(width);
        ctx.stroke();
    }
}

/// Lay out `tree` for `key` (counted, and timed under `HDTREEMAP_STATS`).
fn lay_out(state: &AppState, tree: &Tree, key: LayoutKey, params: &LayoutParams) -> TreemapLayout {
    let started = Instant::now();
    let (width, height) = (key.width, key.height);
    let bounds = PxRect::new(0.0, 0.0, f64::from(width), f64::from(height));
    let map = layout(tree, key.zoom_root, bounds, params);
    state.count_layout();
    if stats_enabled() {
        crate::render::stats_line(&format!(
            "layout {:.1} ms | {width}x{height} | {} rects, {} painted",
            ms(started),
            map.rects().len(),
            map.painted_count()
        ));
    }
    map
}

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

impl Widget for TreemapView {
    fn type_name(&self) -> &'static str {
        "TreemapView"
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
        size
    }

    /// The hovered rectangle: moving to another one hides the tip and re-arms the hover
    /// delay, as entering a new widget would.
    fn tooltip_key(&self) -> Option<u64> {
        self.hover_index.map(|i| i as u64)
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let (mut ox, mut oy) = (0.0, 0.0);
        agg_gui::widget::logical_root_transform(ctx).transform(&mut ox, &mut oy);
        self.origin = Point::new(ox, oy);
        let scale = effective_scale();
        self.refresh(scale);
        let Some(pixels) = self.shown_pixels() else {
            return;
        };
        let Some(cache) = &self.cache else {
            return;
        };
        let (w, h) = (cache.key.width, cache.key.height);
        // Stretched over the widget: the raster may be smaller than its physical size
        // (`geometry::MAX_RASTER_PIXELS`).
        ctx.draw_image_rgba_arc(
            &pixels,
            w,
            h,
            0.0,
            0.0,
            self.bounds.width,
            self.bounds.height,
        );
    }

    fn paint_overlay(&mut self, ctx: &mut dyn DrawCtx) {
        if let Some(selected) = self.selected_rect() {
            self.stroke(ctx, selected, SELECTED_COLOR, OUTLINE);
        }
        let hover_index = self.shown_hover_index();
        let Some(cache) = &self.cache else {
            return;
        };
        let Some(index) = hover_index else {
            return;
        };
        let map = &cache.layout;
        let folder = map
            .ancestors(index)
            .next()
            .filter(|&p| p != 0)
            .map(|p| map.rects()[p].rect);
        let hovered = map.rects().get(index).map(|r| r.rect);
        if let Some(folder) = folder {
            self.stroke(ctx, folder, FOLDER_COLOR, FOLDER_OUTLINE);
        }
        if let Some(hovered) = hovered {
            self.stroke(ctx, hovered, HOVER_COLOR, OUTLINE);
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { pos } => {
                if self.hit_test(*pos) {
                    self.hover_at(*pos);
                } else {
                    self.clear_hover();
                }
                EventResult::Ignored
            }
            Event::MouseLeave => {
                self.clear_hover();
                EventResult::Ignored
            }
            Event::MouseDown {
                pos,
                button: MouseButton::Left,
                ..
            } => {
                self.press(*pos);
                EventResult::Consumed
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => EventResult::Consumed,
            Event::MouseDown {
                pos,
                button: MouseButton::Right,
                ..
            } => {
                self.context_menu_at(*pos);
                EventResult::Consumed
            }
            Event::MouseUp {
                button: MouseButton::Right,
                ..
            } => EventResult::Consumed,
            Event::MouseDown {
                button: BACK_BUTTON,
                ..
            } => {
                self.state.zoom_out();
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn on_unconsumed_key(&mut self, key: &Key, modifiers: Modifiers) -> EventResult {
        let plain = !(modifiers.ctrl || modifiers.meta || modifiers.alt);
        match key {
            Key::Backspace if plain && self.state.zoom_out() => EventResult::Consumed,
            Key::Escape if self.state.highlight_ext().is_some() => {
                self.state.set_highlight_ext(None);
                EventResult::Consumed
            }
            Key::Escape if self.state.selected().is_some() => {
                self.state.set_selected(None);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }
}
