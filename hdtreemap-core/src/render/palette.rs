//! The treemap's colours (`docs/PLAN.md` §3.4), by one of two schemes ([`ColorBy`]):
//!
//! - **by file type** (the default): one of [`HUES`] for each of the extensions using the
//!   most space, neutral grey for every other file, and a darker grey for blocks that stand
//!   for several items (aggregates and folders too small to open), the WinDirStat scheme.
//!   Built from the scan's [`ExtTable`] ([`Palette::from_extensions`]).
//! - **by top-level folder**: one of [`HUES`] for each of the biggest children of the zoom
//!   root, shared by everything inside it (blocks a little darker), grey for the rest
//!   ([`Palette::for_top_folders`]). The cushions still show the structure inside.
//!
//! `AppState::palette` builds it once per tree (and zoom root, by folder); the legend's
//! swatches read the same colours ([`Palette::ext_color`], [`Palette::folder_color`]).
//! [`Palette::highlighting`] derives the palette of the extension highlight: every colour
//! dimmed but the chosen extension's files.

use std::collections::HashMap;

use crate::layout::{LayoutRect, RectKind, TreemapLayout, NO_PARENT};
use crate::model::{ExtTable, NodeId, SizeMode, Tree};

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

/// Share of its brightness a colour keeps when another extension is highlighted.
pub const DIMMED: f32 = 0.22;
/// Share of its folder's brightness a block (folder block, merged leftovers) keeps when
/// colouring by top-level folder.
pub const FOLDER_BLOCK_SHADE: f32 = 0.7;

/// What decides a rectangle's colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorBy {
    /// The file's extension.
    #[default]
    Extension,
    /// The top-level folder (child of the zoom root) it is in.
    TopFolder,
}

impl ColorBy {
    pub fn label(self) -> &'static str {
        match self {
            ColorBy::Extension => "File Type",
            ColorBy::TopFolder => "Top-Level Folder",
        }
    }
}

/// Colour per extension id. See the module docs.
#[derive(Clone, Debug)]
pub struct Palette {
    /// Indexed by extension id; `None` = `other`.
    by_ext: Vec<Option<Rgb>>,
    /// `(extension id, colour)` in hue order.
    assigned: Vec<(u16, Rgb)>,
    /// Files whose extension has no colour of its own ([`OTHER_FILES`]).
    other: Rgb,
    /// Aggregates and folder blocks ([`FOLDER_BLOCK`], or the theme's grey).
    block: Rgb,
    /// Pixels no rectangle covers ([`BACKGROUND`](super::BACKGROUND), or the theme's).
    background: [u8; 4],
    color_by: ColorBy,
    /// By top-level folder: the zoom root and the colour of each of its children.
    zoom_root: NodeId,
    folders: HashMap<NodeId, Rgb>,
    /// `(folder, colour)` in hue order, biggest first.
    folder_order: Vec<(NodeId, Rgb)>,
    /// Every rectangle but this extension's files is dimmed.
    highlight: Option<u16>,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            by_ext: Vec::new(),
            assigned: Vec::new(),
            other: OTHER_FILES,
            block: FOLDER_BLOCK,
            background: super::raster::BACKGROUND,
            color_by: ColorBy::Extension,
            zoom_root: NodeId::NONE,
            folders: HashMap::new(),
            folder_order: Vec::new(),
            highlight: None,
        }
    }
}

fn dim(c: Rgb) -> Rgb {
    c.map(|v| v * DIMMED)
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
        Self {
            by_ext,
            assigned,
            ..Self::default()
        }
    }

    /// Colour by the top-level folders of `zoom_root`: the [`HUES`] go to its biggest
    /// children (by the tree's active size; files count too), grey to the rest.
    pub fn for_top_folders(tree: &Tree, zoom_root: NodeId) -> Self {
        let folder_order: Vec<(NodeId, Rgb)> = tree
            .children_by_size(zoom_root)
            .into_iter()
            .filter(|&c| tree.size(c) > 0)
            .zip(HUES.iter().copied())
            .collect();
        Self {
            color_by: ColorBy::TopFolder,
            zoom_root,
            folders: folder_order.iter().copied().collect(),
            folder_order,
            ..Self::default()
        }
    }

    /// This palette with the neutral colours of a theme: the grey of folder blocks and
    /// merged leftovers, and the raster's background (`theme::map_tones`). The hues stay.
    pub fn with_neutrals(mut self, block: Rgb, background: [u8; 4]) -> Self {
        self.block = block;
        self.background = background;
        self
    }

    /// The grey of folder blocks and merged leftovers.
    pub fn block(&self) -> Rgb {
        self.block
    }

    /// The colour of raster pixels no rectangle covers (an empty zoom root).
    pub fn background(&self) -> [u8; 4] {
        self.background
    }

    /// Which scheme this palette colours by.
    pub fn color_by(&self) -> ColorBy {
        self.color_by
    }

    /// This palette with every rectangle dimmed except the files of extension id `ext`
    /// (the extension legend's highlight).
    pub fn highlighting(&self, ext: u16, _ext_count: usize) -> Self {
        Self {
            highlight: Some(ext),
            ..self.clone()
        }
    }

    /// The colour of top-level folder (or file) `node` when colouring by folder: its hue,
    /// or grey when it has none.
    pub fn folder_color(&self, node: NodeId) -> Rgb {
        self.folders.get(&node).copied().unwrap_or(self.other)
    }

    /// The top-level folders that have a hue, biggest first (by folder only).
    pub fn folders(&self) -> &[(NodeId, Rgb)] {
        &self.folder_order
    }

    /// The colour of files with extension id `ext` (dimmed in a highlighting palette
    /// unless `ext` is the highlighted one).
    pub fn ext_color(&self, ext: u16) -> Rgb {
        let color = self
            .by_ext
            .get(usize::from(ext))
            .copied()
            .flatten()
            .unwrap_or(self.other);
        match self.highlight {
            Some(lit) if lit != ext => dim(color),
            _ => color,
        }
    }

    /// The extensions that have a hue, biggest first.
    pub fn assigned(&self) -> &[(u16, Rgb)] {
        &self.assigned
    }

    /// The base colour of a laid-out rectangle by file type (the folder scheme needs the
    /// layout: [`Palette::colors`]).
    pub fn rect_color(&self, tree: &Tree, rect: &LayoutRect) -> Rgb {
        match rect.kind {
            RectKind::File => match tree.node(rect.node) {
                Some(n) => self.ext_color(n.ext()),
                None => self.dim_unless_highlighted(tree, rect, self.other),
            },
            RectKind::Dir | RectKind::FolderBlock | RectKind::Aggregate => {
                self.dim_unless_highlighted(tree, rect, self.block)
            }
        }
    }

    /// The colour of every rectangle of `layout`, by index (unpainted directories get the
    /// block colour).
    pub fn colors(&self, tree: &Tree, layout: &TreemapLayout) -> Vec<Rgb> {
        let rects = layout.rects();
        if self.color_by == ColorBy::Extension {
            return rects.iter().map(|r| self.rect_color(tree, r)).collect();
        }
        // The rectangle of each one's top-level folder: rectangles are in pre-order, so a
        // parent's entry is ready before its children's.
        let mut tops: Vec<u32> = Vec::with_capacity(rects.len());
        let mut colors = Vec::with_capacity(rects.len());
        for (i, r) in rects.iter().enumerate() {
            let top = match r.parent {
                NO_PARENT => i as u32,
                p if rects
                    .get(p as usize)
                    .is_some_and(|pr| pr.parent == NO_PARENT) =>
                {
                    i as u32
                }
                p => tops.get(p as usize).copied().unwrap_or(i as u32),
            };
            tops.push(top);
            let top_node = rects[top as usize].node;
            let at_root = r.parent == NO_PARENT
                || (r.kind == RectKind::Aggregate && r.node == self.zoom_root);
            let color = if at_root || top_node == self.zoom_root {
                self.block
            } else {
                let hue = self.folder_color(top_node);
                match r.kind {
                    RectKind::File => hue,
                    RectKind::Dir | RectKind::FolderBlock | RectKind::Aggregate => {
                        hue.map(|v| v * FOLDER_BLOCK_SHADE)
                    }
                }
            };
            colors.push(self.dim_unless_highlighted(tree, r, color));
        }
        colors
    }

    /// `color`, dimmed while an extension is highlighted and `rect` is not one of its files.
    fn dim_unless_highlighted(&self, tree: &Tree, rect: &LayoutRect, color: Rgb) -> Rgb {
        let Some(ext) = self.highlight else {
            return color;
        };
        let lit =
            rect.kind == RectKind::File && tree.node(rect.node).is_some_and(|n| n.ext() == ext);
        if lit {
            color
        } else {
            dim(color)
        }
    }
}
