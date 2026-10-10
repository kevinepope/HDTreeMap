//! `ExtTable`: totals per file extension (bytes in both size modes, file count), computed
//! from a finished [`Tree`] or one folder of it. The treemap's colours come from the whole
//! scan's table; the extension legend lists the table of the map's zoom root
//! ([`ExtTable::from_subtree`]). Both are keyed by the same extension ids.

use super::node::{NodeFlags, NodeId, SizeMode, Sizes};
use super::tree::Tree;

/// Totals for one extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtRow {
    /// Extension id in the tree (`0` for files without an extension).
    pub ext: u16,
    /// Lower-case extension without the dot; empty for "no extension".
    pub name: String,
    pub sizes: Sizes,
    pub count: u64,
}

/// Every extension in a tree with its totals, sorted by the size mode it was built for.
#[derive(Clone, Debug, Default)]
pub struct ExtTable {
    rows: Vec<ExtRow>,
}

impl ExtTable {
    /// Total the live files of `tree` by extension, largest (in the tree's active size
    /// mode) first; ties by name.
    pub fn from_tree(tree: &Tree) -> Self {
        let mut totals: Vec<(Sizes, u64)> = vec![(Sizes::default(), 0); tree.ext_count()];
        for node in &tree.nodes {
            if node.is_dir() || node.flags.contains(NodeFlags::REMOVED) {
                continue;
            }
            add_file(&mut totals, node);
        }
        Self::from_totals(tree, totals)
    }

    /// Total the live files below folder `root` (the whole tree for the scan root; an
    /// empty table for an unknown or removed node), sorted like [`ExtTable::from_tree`].
    pub fn from_subtree(tree: &Tree, root: NodeId) -> Self {
        if root == tree.root() {
            return Self::from_tree(tree);
        }
        let mut totals: Vec<(Sizes, u64)> = vec![(Sizes::default(), 0); tree.ext_count()];
        if tree.is_live(root) {
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                let Some(node) = tree.node(id) else {
                    continue;
                };
                if node.is_dir() {
                    stack.extend(tree.children(id));
                } else {
                    add_file(&mut totals, node);
                }
            }
        }
        Self::from_totals(tree, totals)
    }

    fn from_totals(tree: &Tree, totals: Vec<(Sizes, u64)>) -> Self {
        let rows = totals
            .into_iter()
            .enumerate()
            .filter(|(_, (_, count))| *count > 0)
            .map(|(ext, (sizes, count))| {
                let ext = ext as u16;
                ExtRow {
                    ext,
                    name: tree.ext_name(ext).to_string(),
                    sizes,
                    count,
                }
            })
            .collect();
        let mut table = Self { rows };
        table.sort(tree.size_mode());
        table
    }

    /// Re-sort for `mode`.
    pub fn sort(&mut self, mode: SizeMode) {
        self.rows.sort_by(|a, b| {
            b.sizes
                .get(mode)
                .cmp(&a.sizes.get(mode))
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    pub fn rows(&self) -> &[ExtRow] {
        &self.rows
    }

    /// The first `n` rows.
    pub fn top(&self, n: usize) -> &[ExtRow] {
        &self.rows[..n.min(self.rows.len())]
    }

    /// The row for an extension name (`""` for none).
    pub fn get(&self, name: &str) -> Option<&ExtRow> {
        self.rows.iter().find(|r| r.name == name)
    }

    /// The row for an extension id.
    pub fn by_ext(&self, ext: u16) -> Option<&ExtRow> {
        self.rows.iter().find(|r| r.ext == ext)
    }
}

fn add_file(totals: &mut [(Sizes, u64)], node: &super::node::Node) {
    if let Some((sizes, count)) = totals.get_mut(usize::from(node.ext)) {
        sizes.allocated = sizes.allocated.saturating_add(node.allocated);
        sizes.apparent = sizes.apparent.saturating_add(node.apparent);
        *count = count.saturating_add(1);
    }
}
