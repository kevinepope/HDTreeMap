//! The cushion rasteriser: paints a [`TreemapLayout`] into an RGBA buffer the size of the
//! treemap view in physical pixels (`docs/PLAN.md` §3.4). `TreemapView` draws the buffer
//! with `draw_image_rgba_arc` and only calls this again when the layout or colours change.
//!
//! Each painted rectangle fills the pixels whose centres it contains (the layout tiles the
//! raster exactly). With [`Shading::Cushion`] a pixel's intensity is
//! `I = Ia + max(0, Is · n·L)`: `n` is the normal of the rectangle's cushion surface at the
//! pixel centre, `L` a light from the upper left ([`LIGHT`]), as in WinDirStat's and
//! QDirStat's cushion renderers; the rectangle's base colour is scaled by
//! `I · BRIGHTNESS`. With
//! [`Shading::Flat`] rectangles are filled evenly and their right and bottom pixel lines are
//! darkened so neighbours stay apart.
//!
//! The rows are split into bands of [`BAND_ROWS`]; each band paints only the rectangles that
//! reach into it ([`crate::layout::bins::Bins`]). Natively the bands run in parallel on
//! rayon; on wasm32 (single-threaded) they run in turn.
//!
//! The buffer is top row first with alpha 255 everywhere, so it reads the same as straight
//! or premultiplied RGBA.

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

use super::palette::{Palette, Rgb};
use crate::layout::bins::Bins;
use crate::layout::{LayoutRect, TreemapLayout};
use crate::model::Tree;

/// How rectangles are shaded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shading {
    /// Per-pixel cushion lighting (the default).
    #[default]
    Cushion,
    /// Even fills with a darker line on each rectangle's right and bottom edge.
    Flat,
}

/// Ambient light `Ia`; the diffuse share is `Is = 1 - Ia`.
pub const AMBIENT: f32 = 0.15;

/// Cushion intensities are scaled by this before colouring (clamped at full colour), so a
/// cushion's top shows its colour at full strength and only the rims fall dark — the role
/// of WinDirStat's `brightness / PALETTE_BRIGHTNESS`.
pub const BRIGHTNESS: f32 = 1.2;

/// Direction towards the light, normalised: up and to the left of the screen (`-x`, `-y`
/// in the raster's Y-down space), mostly in front (`+z`).
pub const LIGHT: [f32; 3] = [-1.0 / LIGHT_LEN, -1.0 / LIGHT_LEN, 10.0 / LIGHT_LEN];

/// `|(-1, -1, 10)| = sqrt(102)`.
const LIGHT_LEN: f32 = 10.099_505;

/// Intensity of an evenly lit (flat) rectangle.
const FLAT_INTENSITY: f32 = 0.92;
/// Edge lines of flat rectangles are this much darker.
const FLAT_EDGE: f32 = 0.6;
/// Flat rectangles narrower than this get no edge line (it would swallow them).
const FLAT_EDGE_MIN: u32 = 3;

/// The default colour of pixels no rectangle covers (an empty zoom root); a palette made
/// for a theme carries its own ([`Palette::background`]).
pub const BACKGROUND: [u8; 4] = [32, 32, 32, 255];

/// Rows per band of parallel work.
pub const BAND_ROWS: u32 = 16;

/// Paint `layout` (laid out into `0..width × 0..height`) into a new `width × height` RGBA
/// buffer, top row first.
pub fn rasterise(
    layout: &TreemapLayout,
    tree: &Tree,
    palette: &Palette,
    shading: Shading,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let row_bytes = width as usize * 4;
    // Zeroed memory comes straight from the allocator; each band paints its background.
    let mut pixels = vec![0u8; row_bytes * height as usize];
    if width == 0 || height == 0 {
        return pixels;
    }

    let colors = palette.colors(tree, layout);
    let leaves: Vec<(&LayoutRect, Rgb)> = layout
        .rects()
        .iter()
        .zip(colors)
        .filter(|(r, _)| r.kind.is_painted())
        .collect();
    let bands = height.div_ceil(BAND_ROWS) as usize;
    let bins = Bins::build(leaves.len(), bands, |i| {
        let (y0, y1) = leaves[i].0.rect.pixel_rows(height);
        let (x0, x1) = leaves[i].0.rect.pixel_cols(width);
        if y0 >= y1 || x0 >= x1 {
            0..0
        } else {
            (y0 / BAND_ROWS) as usize..((y1 - 1) / BAND_ROWS) as usize + 1
        }
    });

    let background = palette.background();
    let paint_band = |(band, chunk): (usize, &mut [u8])| {
        let first_row = band as u32 * BAND_ROWS;
        let rows = (chunk.len() / row_bytes) as u32;
        for px in chunk.chunks_exact_mut(4) {
            px.copy_from_slice(&background);
        }
        for &leaf in bins.cell(band) {
            let (rect, color) = &leaves[leaf as usize];
            let target = Band {
                pixels: chunk,
                first_row,
                rows,
                width,
                height,
            };
            match shading {
                Shading::Cushion => paint_cushion(target, rect, *color),
                Shading::Flat => paint_flat(target, rect, *color),
            }
        }
    };
    let band_bytes = row_bytes * BAND_ROWS as usize;
    #[cfg(not(target_arch = "wasm32"))]
    pixels
        .par_chunks_mut(band_bytes)
        .enumerate()
        .for_each(paint_band);
    #[cfg(target_arch = "wasm32")]
    pixels
        .chunks_mut(band_bytes)
        .enumerate()
        .for_each(paint_band);
    pixels
}

/// The rows of the raster one band owns.
struct Band<'a> {
    pixels: &'a mut [u8],
    first_row: u32,
    rows: u32,
    width: u32,
    height: u32,
}

impl Band<'_> {
    /// The band's part of `rect`: pixel columns and raster rows, or `None` if they miss.
    fn span(&self, rect: &LayoutRect) -> Option<(u32, u32, u32, u32)> {
        let (x0, x1) = rect.rect.pixel_cols(self.width);
        let (y0, y1) = rect.rect.pixel_rows(self.height);
        let y0b = y0.max(self.first_row);
        let y1b = y1.min(self.first_row + self.rows);
        (x0 < x1 && y0b < y1b).then_some((x0, x1, y0b, y1b))
    }

    fn row(&mut self, y: u32, x0: u32, x1: u32) -> &mut [u8] {
        let start = ((y - self.first_row) * self.width + x0) as usize * 4;
        let end = ((y - self.first_row) * self.width + x1) as usize * 4;
        &mut self.pixels[start..end]
    }
}

fn to_byte(v: f32) -> u8 {
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// The cushion intensity at a surface gradient `(dz/dx, dz/dy)`.
pub fn cushion_intensity(dzdx: f32, dzdy: f32) -> f32 {
    // Normal (-dz/dx, -dz/dy, 1), not yet normalised.
    let (nx, ny) = (-dzdx, -dzdy);
    let cos = (nx * LIGHT[0] + ny * LIGHT[1] + LIGHT[2]) / (nx * nx + ny * ny + 1.0).sqrt();
    AMBIENT + ((1.0 - AMBIENT) * cos.min(1.0)).max(0.0)
}

fn paint_cushion(mut band: Band<'_>, rect: &LayoutRect, color: Rgb) {
    let Some((x0, x1, y0, y1)) = band.span(rect) else {
        return;
    };
    let [a, b, c, d] = rect.cushion;
    for y in y0..y1 {
        let dzdy = 2.0 * b * (y as f32 + 0.5) + d;
        let row = band.row(y, x0, x1);
        for (i, px) in row.chunks_exact_mut(4).enumerate() {
            let x = (x0 as usize + i) as f32 + 0.5;
            let light = cushion_intensity(2.0 * a * x + c, dzdy) * BRIGHTNESS;
            px[0] = to_byte(color[0] * light);
            px[1] = to_byte(color[1] * light);
            px[2] = to_byte(color[2] * light);
            px[3] = 255;
        }
    }
}

fn paint_flat(mut band: Band<'_>, rect: &LayoutRect, color: Rgb) {
    let Some((x0, x1, y0, y1)) = band.span(rect) else {
        return;
    };
    let (_, full_x1) = rect.rect.pixel_cols(band.width);
    let (full_y0, full_y1) = rect.rect.pixel_rows(band.height);
    let (full_x0, _) = rect.rect.pixel_cols(band.width);
    let edge_x = (full_x1 - full_x0 >= FLAT_EDGE_MIN).then_some(full_x1 - 1);
    let edge_y = (full_y1 - full_y0 >= FLAT_EDGE_MIN).then_some(full_y1 - 1);
    let fill = color.map(|c| to_byte(c * FLAT_INTENSITY));
    let edge = color.map(|c| to_byte(c * FLAT_INTENSITY * FLAT_EDGE));
    for y in y0..y1 {
        let row = band.row(y, x0, x1);
        for (i, px) in row.chunks_exact_mut(4).enumerate() {
            let x = x0 + i as u32;
            let on_edge = Some(y) == edge_y || Some(x) == edge_x;
            let rgb = if on_edge { edge } else { fill };
            px[..3].copy_from_slice(&rgb);
            px[3] = 255;
        }
    }
}
