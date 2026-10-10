//! Values derived from the finished tree and cached in `AppState`, so every view reads the
//! same one and nothing recomputes them per frame:
//!
//! - [`AppState::palette`] — the map's colours (`render/palette.rs`): by extension, rebuilt
//!   when the tree changes; by top-level folder, also when the zoom root changes. The
//!   treemap paints with it and the legend draws its swatches from it, so the two can never
//!   disagree.
//! - [`AppState::zoom_extensions`] — the [`ExtTable`] of the map's zoom root, rebuilt when the
//!   tree or the zoom root changes. The extension legend lists it and the status bar's
//!   highlight note reads its totals.

use std::cell::RefCell;
use std::rc::Rc;

use web_time::Instant;

use super::AppState;
use crate::model::{ExtTable, NodeId};
use crate::render::{stats_enabled, ColorBy, Palette};

/// What a zoom root's extension table depends on: `(tree_version, zoom root)`.
type ZoomKey = (u64, NodeId);

/// What the palette depends on: the tree version, the scheme, and (by folder) the zoom root.
type PaletteKey = (u64, ColorBy, NodeId);

/// See the module docs.
#[derive(Default)]
pub(super) struct DerivedCache {
    palette: RefCell<Option<(PaletteKey, Rc<Palette>)>>,
    zoom_extensions: RefCell<Option<(ZoomKey, Rc<ExtTable>)>>,
}

impl AppState {
    fn derived(&self) -> &DerivedCache {
        &self.inner.view.derived
    }

    /// The map's colours for the shown scan (`None` with no result).
    pub fn palette(&self) -> Option<Rc<Palette>> {
        let color_by = self.color_by();
        let root = match color_by {
            ColorBy::Extension => NodeId::NONE,
            ColorBy::TopFolder => self.zoom_root(),
        };
        let key: PaletteKey = (self.tree_version(), color_by, root);
        if let Some((k, palette)) = self.derived().palette.borrow().as_ref() {
            if *k == key {
                return Some(Rc::clone(palette));
            }
        }
        let palette = self.with_result(|r, exts| {
            Rc::new(match color_by {
                ColorBy::Extension => {
                    Palette::from_extensions(exts, r.tree.size_mode(), r.tree.ext_count())
                }
                ColorBy::TopFolder => Palette::for_top_folders(r.tree, root),
            })
        })?;
        *self.derived().palette.borrow_mut() = Some((key, Rc::clone(&palette)));
        Some(palette)
    }

    /// Totals per extension of the files below the zoom root, sorted by the active size
    /// (`None` with no result).
    pub fn zoom_extensions(&self) -> Option<Rc<ExtTable>> {
        let key: ZoomKey = (self.tree_version(), self.zoom_root());
        if let Some((k, table)) = self.derived().zoom_extensions.borrow().as_ref() {
            if *k == key {
                return Some(Rc::clone(table));
            }
        }
        let started = Instant::now();
        let table = self.with_result(|r, exts| {
            if key.1 == r.tree.root() {
                // The scan's own table, already totalled and sorted.
                Rc::new(exts.clone())
            } else {
                Rc::new(ExtTable::from_subtree(r.tree, key.1))
            }
        })?;
        if stats_enabled() {
            crate::render::stats_line(&format!(
                "extension table {:.1} ms | {} extensions",
                started.elapsed().as_secs_f64() * 1000.0,
                table.rows().len()
            ));
        }
        *self.derived().zoom_extensions.borrow_mut() = Some((key, Rc::clone(&table)));
        Some(table)
    }
}
