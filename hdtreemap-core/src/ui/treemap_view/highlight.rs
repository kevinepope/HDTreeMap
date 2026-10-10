//! The extension highlight on the treemap: while an extension is chosen in the legend
//! (`AppState::highlight_ext`), the map is drawn from a second raster in which every
//! rectangle but that extension's files is dimmed ([`Palette::highlighting`]).
//!
//! Why a raster and not outlines in the overlay: a common extension has thousands of
//! rectangles on the archive-server demo (2,105 `.jpg` files at 1600×1000), and stroking
//! them every frame measured about 9 ms per frame on the software renderer — most of the
//! 10 ms budget, on every hover. The dimmed raster costs one raster pass when the highlight
//! (or the layout, or the shading) changes and nothing per frame after that: it is drawn
//! with `draw_image_rgba_arc` exactly like the plain map, so the GPU keeps its texture.
//! Dimming the rest also shows where the extension is better than thin outlines do.
//!
//! The highlighted file rectangles are counted along with each dimmed raster, for
//! `HDTREEMAP_STATS` and tests ([`super::TreemapView::highlighted_rects`]).

use std::rc::Rc;
use std::sync::Arc;

use web_time::Instant;

use super::{ms, LayoutKey, TreemapView};
use crate::layout::RectKind;
use crate::render::{rasterise, stats_enabled, Palette, Shading};

/// The dimmed raster for one extension, layout and shading.
struct Dimmed {
    ext: u16,
    key: LayoutKey,
    shading: Shading,
    /// The palette it dims (the colour scheme).
    palette: Rc<Palette>,
    pixels: Arc<Vec<u8>>,
    /// File rectangles of the extension in the layout.
    files: usize,
}

/// See the module docs.
#[derive(Default)]
pub(super) struct Highlight {
    dimmed: Option<Dimmed>,
    /// File rectangles highlighted by the last paint (0 with no highlight).
    painted: usize,
}

impl Highlight {
    /// The layout changed: the dimmed raster is stale.
    pub(super) fn forget(&mut self) {
        self.dimmed = None;
    }

    pub(super) fn painted(&self) -> usize {
        self.painted
    }
}

impl TreemapView {
    /// The pixels to draw: the dimmed raster while an extension is highlighted (made if
    /// stale), else the plain one. Returns `None` with no map.
    pub(super) fn shown_pixels(&mut self) -> Option<Arc<Vec<u8>>> {
        let cache = self.cache.as_ref()?;
        let Some(ext) = self.state.highlight_ext() else {
            self.highlight.painted = 0;
            return Some(Arc::clone(&cache.pixels));
        };
        let fresh = self.highlight.dimmed.as_ref().is_some_and(|d| {
            d.ext == ext
                && d.key == cache.key
                && d.shading == cache.shading
                && Rc::ptr_eq(&d.palette, &cache.palette)
        });
        if !fresh {
            let started = Instant::now();
            let (key, shading) = (cache.key, cache.shading);
            let made = self.state.with_result(|r, _| {
                let tree = &r.tree;
                let palette = cache.palette.highlighting(ext, tree.ext_count());
                let pixels = rasterise(
                    &cache.layout,
                    tree,
                    &palette,
                    shading,
                    key.width,
                    key.height,
                );
                let files = cache
                    .layout
                    .rects()
                    .iter()
                    .filter(|rect| {
                        rect.kind == RectKind::File
                            && tree.node(rect.node).is_some_and(|n| n.ext() == ext)
                    })
                    .count();
                Dimmed {
                    ext,
                    key,
                    shading,
                    palette: Rc::clone(&cache.palette),
                    pixels: Arc::new(pixels),
                    files,
                }
            });
            self.state.count_raster();
            if stats_enabled() {
                crate::render::stats_line(&format!(
                    "highlight raster {:.1} ms | {}x{} | {} files",
                    ms(started),
                    key.width,
                    key.height,
                    made.as_ref().map_or(0, |d| d.files)
                ));
            }
            self.highlight.dimmed = made;
        }
        let dimmed = self.highlight.dimmed.as_ref()?;
        self.highlight.painted = dimmed.files;
        Some(Arc::clone(&dimmed.pixels))
    }
}
