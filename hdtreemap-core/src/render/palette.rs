//! The treemap's colours (`docs/PLAN.md` §3.4): one of [`HUES`] for each of the extensions
//! using the most space, neutral grey for every other file, and a darker grey for blocks
//! that stand for several items (aggregates and folders too small to open), the WinDirStat
//! scheme. Built from the scan's [`ExtTable`]; the extension legend (M3) reads the same
//! assignments through [`Palette::assigned`].

use crate::layout::{LayoutRect, RectKind};
use crate::model::{ExtTable, SizeMode, Tree};

/// Linear RGB in `0.0..=1.0`.
pub type Rgb = [f32; 3];

/// Hand-picked, clearly distinct hues for the top extensions, biggest first.
pub const HUES: [Rgb; 16] = [
    rgb(66, 133, 244),  // blue
    rgb(234, 67, 53),   // red
    rgb(52, 168, 83),   // green
    rgb(251, 188, 5),   // yellow
    rgb(171, 71, 188),  // purple
    rgb(0, 172, 193),   // cyan
    rgb(255, 112, 67),  // orange
    rgb(124, 179, 66),  // lime
    rgb(236, 64, 122),  // pink
    rgb(92, 107, 192),  // indigo
    rgb(38, 166, 154),  // teal
    rgb(161, 120, 100), // brown
    rgb(100, 181, 246), // light blue
    rgb(255, 138, 128), // salmon
    rgb(212, 225, 87),  // pale lime
    rgb(206, 147, 216), // lavender
];

/// Files whose extension has no hue of its own.
pub const OTHER_FILES: Rgb = rgb(150, 150, 150);
/// Aggregates and folders drawn as one block.
pub const FOLDER_BLOCK: Rgb = rgb(95, 95, 95);

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

/// Colour per extension id. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct Palette {
    /// Indexed by extension id; `None` = [`OTHER_FILES`].
    by_ext: Vec<Option<Rgb>>,
    /// `(extension id, colour)` in hue order.
    assigned: Vec<(u16, Rgb)>,
}

impl Palette {
    /// Give the [`HUES`] to the biggest extensions of `table` by `mode` (files without an
    /// extension stay grey). `ext_count` is the tree's number of extension ids.
    pub fn from_extensions(table: &ExtTable, mode: SizeMode, ext_count: usize) -> Self {
        let mut rows: Vec<_> = table
            .rows()
            .iter()
            .filter(|r| !r.name.is_empty() && r.sizes.get(mode) > 0)
            .collect();
        rows.sort_by(|a, b| {
            b.sizes
                .get(mode)
                .cmp(&a.sizes.get(mode))
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut by_ext = vec![None; ext_count];
        let mut assigned = Vec::with_capacity(HUES.len());
        for (row, &hue) in rows.iter().zip(HUES.iter()) {
            if let Some(slot) = by_ext.get_mut(usize::from(row.ext)) {
                *slot = Some(hue);
                assigned.push((row.ext, hue));
            }
        }
        Self { by_ext, assigned }
    }

    /// The colour of files with extension id `ext`.
    pub fn ext_color(&self, ext: u16) -> Rgb {
        self.by_ext
            .get(usize::from(ext))
            .copied()
            .flatten()
            .unwrap_or(OTHER_FILES)
    }

    /// The extensions that have a hue, biggest first.
    pub fn assigned(&self) -> &[(u16, Rgb)] {
        &self.assigned
    }

    /// The base colour of a laid-out rectangle.
    pub fn rect_color(&self, tree: &Tree, rect: &LayoutRect) -> Rgb {
        match rect.kind {
            RectKind::File => tree
                .node(rect.node)
                .map_or(OTHER_FILES, |n| self.ext_color(n.ext())),
            RectKind::Dir | RectKind::FolderBlock | RectKind::Aggregate => FOLDER_BLOCK,
        }
    }
}
