//! `ExtLegend`: the extension legend under the folder tree (`docs/PLAN.md` §2, view 3), an
//! agg-gui `Table` listing the extensions of the files below the map's zoom root.
//!
//! Columns: the colour swatch (the treemap's colour for the extension, from the shared
//! `AppState::palette`, drawn by the cell painter), the extension, total size, share of
//! the zoom root, and file count. Rows are sorted by size; a header click sorts by that
//! column instead, and a second click reverses it. Clicking a row highlights the
//! extension's files on the map (`AppState::highlight_ext`); clicking it again (or Escape
//! on the map) clears it.
//!
//! The rows come from `AppState::zoom_extensions` (cached per zoom root and tree version),
//! refreshed on the table's own layout through its rows provider, so zooming the map
//! updates the legend without rebuilding the widget.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widgets::table::distribute_widths;
use agg_gui::{
    CellInfo, Color, DrawCtx, EventResult, HAnchor, Rect, Size, Table, TableColumn, TableRows,
    VAnchor, Widget, WidgetBase,
};

use crate::fonts::fa;
use crate::format::{format_bytes, format_count, format_percent};
use crate::model::NodeId;
use crate::render::palette::Rgb;
use crate::state::AppState;

/// Widget id of the extension legend.
pub const EXT_LEGEND_ID: &str = "ext-legend";
/// What the extension column says for files without one.
pub const NO_EXTENSION: &str = "(no extension)";

/// Row height (logical pixels).
pub const ROW_HEIGHT: f64 = 20.0;
/// Header height (logical pixels).
pub const HEADER_HEIGHT: f64 = 22.0;
/// Text size.
const FONT_SIZE: f64 = 12.5;
/// Space between a cell's edge and its text.
const PAD: f64 = 6.0;

/// The legend's columns, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Swatch,
    Extension,
    Size,
    Share,
    Files,
}

impl Column {
    pub const ALL: [Column; 5] = [
        Column::Swatch,
        Column::Extension,
        Column::Size,
        Column::Share,
        Column::Files,
    ];

    fn title(self) -> &'static str {
        match self {
            Column::Swatch => "",
            Column::Extension => "Extension",
            Column::Size => "Size",
            Column::Share => "%",
            Column::Files => "Files",
        }
    }

    fn spec(self) -> TableColumn {
        match self {
            Column::Swatch => TableColumn::exact(22.0),
            Column::Extension => TableColumn::remainder().at_least(64.0).clip(true),
            Column::Size => TableColumn::exact(72.0),
            Column::Share => TableColumn::exact(54.0),
            Column::Files => TableColumn::exact(70.0),
        }
    }

    /// Numbers read right-aligned.
    fn right_aligned(self) -> bool {
        matches!(self, Column::Size | Column::Share | Column::Files)
    }
}

/// How the rows are ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortOrder {
    pub column: Column,
    pub descending: bool,
}

impl Default for SortOrder {
    fn default() -> Self {
        Self {
            column: Column::Size,
            descending: true,
        }
    }
}

/// One extension's row.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendRow {
    pub ext: u16,
    /// Lower-case, no dot; empty for files without an extension.
    pub name: String,
    pub bytes: u64,
    pub count: u64,
    pub color: Rgb,
}

/// What the table shows. Shared by the table's callbacks and the widget.
#[derive(Default)]
struct Model {
    /// `(tree_version, zoom root)` the rows were made for.
    key: Option<(u64, NodeId)>,
    rows: Vec<LegendRow>,
    /// Size of the zoom root (the % column's whole).
    total: u64,
    sort: SortOrder,
}

impl Model {
    /// Re-read the zoom root's extensions if the tree or the zoom changed.
    fn refresh(&mut self, state: &AppState) {
        let key = (state.tree_version(), state.zoom_root());
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        let (Some(table), Some(palette)) = (state.zoom_extensions(), state.palette()) else {
            self.rows.clear();
            self.total = 0;
            return;
        };
        let mode = state.size_mode();
        self.total = state
            .with_result(|r, _| r.tree.size(key.1))
            .unwrap_or_default();
        self.rows = table
            .rows()
            .iter()
            .map(|r| LegendRow {
                ext: r.ext,
                name: r.name.clone(),
                bytes: r.sizes.get(mode),
                count: r.count,
                color: palette.ext_color(r.ext),
            })
            .collect();
        self.apply_sort();
    }

    fn apply_sort(&mut self) {
        let SortOrder { column, descending } = self.sort;
        self.rows.sort_by(|a, b| {
            let order = match column {
                Column::Extension => a.name.cmp(&b.name),
                Column::Files => a.count.cmp(&b.count),
                Column::Swatch | Column::Size | Column::Share => a.bytes.cmp(&b.bytes),
            };
            let order = if descending { order.reverse() } else { order };
            order.then_with(|| a.name.cmp(&b.name))
        });
    }

    /// A header click: sort by `column`, or reverse it if it already sorts.
    fn sort_by(&mut self, column: Column) {
        let column = if column == Column::Swatch {
            Column::Size
        } else {
            column
        };
        self.sort = if self.sort.column == column {
            SortOrder {
                column,
                descending: !self.sort.descending,
            }
        } else {
            SortOrder {
                column,
                // Names read A to Z; amounts biggest first.
                descending: column != Column::Extension,
            }
        };
        self.apply_sort();
    }

    fn cell_text(&self, row: &LegendRow, column: Column) -> String {
        match column {
            Column::Swatch => String::new(),
            Column::Extension if row.name.is_empty() => NO_EXTENSION.to_string(),
            Column::Extension => format!(".{}", row.name),
            Column::Size => format_bytes(row.bytes),
            Column::Share => format_percent(row.bytes, self.total),
            Column::Files => format_count(row.count),
        }
    }
}

/// See the module docs.
pub struct ExtLegend {
    bounds: Rect,
    /// `[Table]`.
    children: Vec<Box<dyn Widget>>,
    base: WidgetBase,
    state: AppState,
    model: Rc<RefCell<Model>>,
    scroll: Rc<Cell<f64>>,
}

impl ExtLegend {
    pub fn new(state: &AppState, font: &Arc<agg_gui::text::Font>) -> Self {
        let model = Rc::new(RefCell::new(Model::default()));
        let scroll = Rc::new(Cell::new(0.0));

        let rows_model = Rc::clone(&model);
        let rows_state = state.clone();
        let paint_model = Rc::clone(&model);
        let header_model = Rc::clone(&model);
        let click_model = Rc::clone(&model);
        let click_state = state.clone();
        let sort_model = Rc::clone(&model);
        let selected_model = Rc::clone(&model);
        let selected_state = state.clone();

        let table = Table::builder()
            .columns(Column::ALL.iter().map(|c| c.spec()).collect())
            .header_height(HEADER_HEIGHT)
            .resizable(false)
            .striped(true)
            .sense_click(true)
            .scroll_offset_cell(Rc::clone(&scroll))
            .rows_provider(Box::new(move || {
                let mut model = rows_model.borrow_mut();
                model.refresh(&rows_state);
                TableRows::Homogeneous {
                    count: model.rows.len(),
                    height: ROW_HEIGHT,
                }
            }))
            .selection_pred(Box::new(move |row| {
                let highlight = selected_state.highlight_ext();
                highlight.is_some()
                    && selected_model.borrow().rows.get(row).map(|r| r.ext) == highlight
            }))
            .cell_painter(Box::new(move |info, ctx| {
                paint_cell(&paint_model.borrow(), info, ctx);
            }))
            .header_painter(Box::new(move |info, ctx| {
                let sort = header_model.borrow().sort;
                paint_header(sort, info.col, info.rect, info.visuals, info.font, ctx);
            }))
            .header_click(Box::new(move |col, _, _| {
                if let Some(&column) = Column::ALL.get(col) {
                    sort_model.borrow_mut().sort_by(column);
                    agg_gui::animation::request_draw();
                }
                EventResult::Consumed
            }))
            .on_row_click(Box::new(move |row, _| {
                let ext = click_model.borrow().rows.get(row).map(|r| r.ext);
                if let Some(ext) = ext {
                    click_state.toggle_highlight_ext(ext);
                }
            }))
            .build(Arc::clone(font));

        Self {
            bounds: Rect::default(),
            children: vec![Box::new(table)],
            base: WidgetBase::new()
                .with_h_anchor(HAnchor::STRETCH)
                .with_v_anchor(VAnchor::STRETCH),
            state: state.clone(),
            model,
            scroll,
        }
    }

    /// The rows as shown, top first.
    pub fn rows(&self) -> Vec<LegendRow> {
        self.model.borrow().rows.clone()
    }

    /// The current order.
    pub fn sort_order(&self) -> SortOrder {
        self.model.borrow().sort
    }

    /// The text of a cell as drawn.
    pub fn cell_text(&self, row: usize, column: Column) -> Option<String> {
        let model = self.model.borrow();
        let r = model.rows.get(row)?;
        Some(model.cell_text(r, column))
    }

    /// Row `row` in local (Y-up) coordinates, when it is in view.
    pub fn row_rect(&self, row: usize) -> Option<Rect> {
        let body = (self.bounds.height - HEADER_HEIGHT).max(0.0);
        let top = row as f64 * ROW_HEIGHT - self.scroll.get();
        let y = body - top - ROW_HEIGHT;
        let fits = row < self.model.borrow().rows.len() && y >= 0.0 && y + ROW_HEIGHT <= body;
        fits.then(|| Rect::new(0.0, y, self.bounds.width, ROW_HEIGHT))
    }

    /// Header cell `column` in local (Y-up) coordinates.
    pub fn header_rect(&self, column: Column) -> Option<Rect> {
        let specs: Vec<TableColumn> = Column::ALL.iter().map(|c| c.spec()).collect();
        let widths = distribute_widths(&specs, self.bounds.width.max(40.0), &[]);
        let at = Column::ALL.iter().position(|&c| c == column)?;
        let x: f64 = widths[..at].iter().sum();
        Some(Rect::new(
            x,
            self.bounds.height - HEADER_HEIGHT,
            widths[at],
            HEADER_HEIGHT,
        ))
    }
}

fn to_color(rgb: Rgb) -> Color {
    Color::rgb(rgb[0], rgb[1], rgb[2])
}

fn paint_cell(model: &Model, info: &CellInfo, ctx: &mut dyn DrawCtx) {
    let Some(row) = model.rows.get(info.row) else {
        return;
    };
    let Some(&column) = Column::ALL.get(info.col) else {
        return;
    };
    let r = info.rect;
    if column == Column::Swatch {
        let side = (r.height - 6.0).clamp(4.0, 12.0);
        ctx.set_fill_color(to_color(row.color));
        ctx.begin_path();
        ctx.rounded_rect(
            r.x + (r.width - side) / 2.0,
            r.y + (r.height - side) / 2.0,
            side,
            side,
            2.0,
        );
        ctx.fill();
        return;
    }
    let text = model.cell_text(row, column);
    let color = if row.name.is_empty() && column == Column::Extension {
        info.visuals.text_dim
    } else {
        info.visuals.text_color
    };
    draw_text(ctx, info.font, &text, r, column.right_aligned(), color);
}

fn paint_header(
    sort: SortOrder,
    col: usize,
    r: Rect,
    visuals: &agg_gui::theme::Visuals,
    font: &Arc<agg_gui::text::Font>,
    ctx: &mut dyn DrawCtx,
) {
    let Some(&column) = Column::ALL.get(col) else {
        return;
    };
    let mut title = column.title().to_string();
    if sort.column == column {
        let arrow = if sort.descending {
            fa::CARET_DOWN
        } else {
            fa::CARET_UP
        };
        if column.right_aligned() {
            title = format!("{arrow} {title}");
        } else {
            title = format!("{title} {arrow}");
        }
    }
    draw_text(
        ctx,
        font,
        &title,
        r,
        column.right_aligned(),
        visuals.text_dim,
    );
}

/// `text` in `rect`, vertically centred, left or right aligned with [`PAD`].
fn draw_text(
    ctx: &mut dyn DrawCtx,
    font: &Arc<agg_gui::text::Font>,
    text: &str,
    rect: Rect,
    right: bool,
    color: Color,
) {
    if text.is_empty() {
        return;
    }
    ctx.set_font(Arc::clone(font));
    ctx.set_font_size(FONT_SIZE);
    ctx.set_fill_color(color);
    let Some(m) = ctx.measure_text(text) else {
        return;
    };
    let x = if right {
        rect.x + rect.width - PAD - m.width
    } else {
        rect.x + PAD
    };
    let y = rect.y + (rect.height - (m.ascent + m.descent)) / 2.0 + m.descent;
    ctx.fill_text(text, x, y);
}

impl Widget for ExtLegend {
    fn type_name(&self) -> &'static str {
        "ExtLegend"
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
        self.model.borrow_mut().refresh(&self.state);
        if let Some(table) = self.children.first_mut() {
            table.layout(size);
            table.set_bounds(self.bounds);
        }
        size
    }

    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {}

    fn on_event(&mut self, _event: &agg_gui::Event) -> EventResult {
        EventResult::Ignored
    }
}
