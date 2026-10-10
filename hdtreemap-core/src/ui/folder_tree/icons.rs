//! The folder tree's row icons: Font Awesome glyphs (folder, file, "more") drawn once into
//! small RGBA images.
//!
//! agg-gui's `TreeView` rows take either its procedural `NodeIcon` shapes or an
//! `IconImage`; they have no glyph slot. To keep the app's icons Font Awesome, each glyph is
//! rendered with the bundled face into an `IconImage` at the display's scale when the tree
//! is built, and shared (by `Arc`) between every row that shows it.

use std::sync::Arc;

use agg_gui::framebuffer::unpremultiply_rgba_inplace;
use agg_gui::text::Font;
use agg_gui::ux_scale::effective_scale;
use agg_gui::{Color, Framebuffer, GfxCtx, IconImage, Size};

use crate::fonts::fa;

/// Logical size of a row icon (`TreeView`'s own icon cell is 14 px).
pub const ICON_SIZE: f64 = 14.0;
/// Glyph size within the icon.
const GLYPH_SIZE: f64 = 12.5;

/// Folder icons: the tree view's folder yellow.
const FOLDER_COLOR: Color = Color::rgb(0.90, 0.72, 0.20);
/// File icons.
const FILE_COLOR: Color = Color::rgb(0.62, 0.74, 0.86);
/// The "N more items" row.
const MORE_COLOR: Color = Color::rgb(0.6, 0.6, 0.6);

/// The three row icons. Cheap to clone.
#[derive(Clone)]
pub struct TreeIcons {
    pub folder: Option<IconImage>,
    pub file: Option<IconImage>,
    pub more: Option<IconImage>,
}

impl TreeIcons {
    /// Render the icons with `icons` (the Font Awesome face) at the current display scale.
    pub fn new(icons: &Arc<Font>) -> Self {
        Self {
            folder: glyph_icon(icons, fa::FOLDER, FOLDER_COLOR),
            file: glyph_icon(icons, fa::FILE_O, FILE_COLOR),
            more: glyph_icon(icons, fa::ELLIPSIS_H, MORE_COLOR),
        }
    }
}

/// `glyph` in `color`, centred in an [`ICON_SIZE`] square, as straight-alpha RGBA at the
/// display's physical resolution.
fn glyph_icon(font: &Arc<Font>, glyph: char, color: Color) -> Option<IconImage> {
    let scale = effective_scale().clamp(0.5, 8.0);
    let px = (ICON_SIZE * scale).ceil().max(1.0) as u32;
    let mut fb = Framebuffer::new(px, px);
    {
        let mut ctx = GfxCtx::new(&mut fb);
        let text = glyph.to_string();
        ctx.set_font(Arc::clone(font));
        ctx.set_font_size(GLYPH_SIZE * scale);
        ctx.set_fill_color(color);
        let side = f64::from(px);
        let metrics = ctx.measure_text(&text)?;
        let x = ((side - metrics.width) / 2.0).max(0.0);
        // Centre the glyph's ascent-to-descent box vertically (Y up, baseline at `y`).
        let y = (side - (metrics.ascent + metrics.descent)) / 2.0 + metrics.descent;
        ctx.fill_text(&text, x, y);
    }
    let mut rgba = fb.pixels_flipped();
    unpremultiply_rgba_inplace(&mut rgba);
    IconImage::from_rgba(Arc::new(rgba), px, px, Size::new(ICON_SIZE, ICON_SIZE))
}
