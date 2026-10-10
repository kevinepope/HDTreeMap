//! The folder tree's lazy row model: which scanned nodes have a `TreeView` node, and the
//! code that adds and removes them.
//!
//! A `TreeView` lists every node it holds, so it is never handed the whole scan (millions of
//! files). It starts with the scan root, and a folder's children are added only when the
//! folder is expanded ([`Rows::populate`], on `TreeViewEvent::Expanded` or when the tree
//! opens a folder itself). Until then a folder with contents shows its expand arrow through
//! `TreeNode::may_have_children`. Each folder lists at most [`CHILD_CAP`] children, largest
//! first (sorted here, as the tree of a scan still running is not), and one
//! [`Entry::More`] row summing up the rest. A folder's rows can be dropped again
//! ([`Rows::remap`] after `TreeView::remove_children`), so it is populated anew when next
//! opened.
//!
//! Each row shows a Font Awesome glyph, the name, the size in the active mode (the row's
//! secondary text) and its share of the parent folder (the row's fraction bar). The glyph
//! colours are the look's ([`recolor`] after a theme switch).
//!
//! `entries` runs parallel to `TreeView::nodes`; `index` maps a scanned node to its row.

use std::collections::HashMap;

use agg_gui::widgets::tree_view::TreeNode;
use agg_gui::{current_visuals, NodeGlyph, NodeIcon};

use crate::fonts::fa;
use crate::format::{format_bytes, format_count};
use crate::model::{NodeId, Tree};
use crate::theme::{tree_glyphs, TreeGlyphs};

/// Most children a folder lists; the rest share one "N more" row.
pub const CHILD_CAP: usize = 200;

/// The icon colours of the look installed now (`theme::tree_glyphs`).
fn glyphs() -> TreeGlyphs {
    tree_glyphs(current_visuals().is_dark())
}

/// Give every row the icon colour of the look installed now (after a theme switch).
pub fn recolor(nodes: &mut [TreeNode]) {
    let colors = glyphs();
    for glyph in nodes.iter_mut().filter_map(|n| n.icon_glyph.as_mut()) {
        glyph.color = match glyph.glyph {
            fa::FOLDER => colors.folder,
            fa::FILE_O => colors.file,
            _ => colors.more,
        };
    }
}

/// What a `TreeView` node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A scanned file or folder; `populated` once its children have rows.
    Node { id: NodeId, populated: bool },
    /// The children of `parent` past [`CHILD_CAP`].
    More { parent: NodeId },
}

/// See the module docs.
#[derive(Default)]
pub struct Rows {
    pub entries: Vec<Entry>,
    /// Scanned node → its `TreeView` node.
    pub index: HashMap<NodeId, usize>,
    /// Folder → its "N more" row.
    more: HashMap<NodeId, usize>,
}

impl Rows {
    /// Start over with just the scan root (collapsed), labelled `root_label` (its own
    /// stored name is the whole scanned path).
    pub fn reset(&mut self, nodes: &mut Vec<TreeNode>, tree: &Tree, root_label: &str) {
        nodes.clear();
        self.entries.clear();
        self.index.clear();
        self.more.clear();
        let idx = self.add_node(nodes, tree, tree.root(), None, 0);
        if let Some(row) = nodes.get_mut(idx) {
            row.label = root_label.to_string();
            // The label is a short name: the full path is the row's tooltip.
            row.tooltip = Some(tree.path(tree.root()).to_string_lossy().into_owned());
        }
    }

    /// The scanned node a `TreeView` node shows (`None` for "more" rows).
    pub fn node_at(&self, idx: usize) -> Option<NodeId> {
        match self.entries.get(idx)? {
            Entry::Node { id, .. } => Some(*id),
            Entry::More { .. } => None,
        }
    }

    /// Add the children of the folder at `idx` (once).
    pub fn populate(&mut self, nodes: &mut Vec<TreeNode>, tree: &Tree, idx: usize) {
        let Some(Entry::Node {
            id,
            populated: false,
        }) = self.entries.get(idx).copied()
        else {
            return;
        };
        self.entries[idx] = Entry::Node {
            id,
            populated: true,
        };
        let parent_size = tree.size(id);
        let mut rest_count = 0u64;
        let mut rest_bytes = 0u64;
        let mut rest_dirs = false;
        let mut rest_files = false;
        for (order, child) in tree.children_by_size(id).into_iter().enumerate() {
            if order < CHILD_CAP {
                self.add_node(nodes, tree, child, Some(idx), order as u32);
            } else {
                rest_count += 1;
                rest_bytes = rest_bytes.saturating_add(tree.size(child));
                let is_dir = tree.node(child).is_some_and(|n| n.is_dir());
                rest_dirs |= is_dir;
                rest_files |= !is_dir;
            }
        }
        if rest_count > 0 {
            let noun = match (rest_dirs, rest_files, rest_count == 1) {
                (true, false, true) => "folder",
                (true, false, false) => "folders",
                (false, true, true) => "file",
                (false, true, false) => "files",
                (_, _, true) => "item",
                _ => "items",
            };
            let label = format!("{} more {noun}", format_count(rest_count));
            let mut row = TreeNode::new(label, NodeIcon::File, Some(idx), CHILD_CAP as u32);
            row.icon_glyph = Some(NodeGlyph::new(fa::ELLIPSIS_H, glyphs().more));
            row.secondary_text = Some(format_bytes(rest_bytes));
            row.fraction = Some(share(rest_bytes, parent_size));
            let more = push(nodes, row);
            self.set_entry(more, Entry::More { parent: id });
            self.more.insert(id, more);
        }
        if let Some(folder) = nodes.get_mut(idx) {
            // Everything in it may have been removed since its row was made.
            folder.may_have_children = false;
        }
    }

    /// Make `node` reachable: add and expand every folder above it. Returns the row that
    /// shows it, or, when it (or a folder above it) is past a folder's cap, that folder's
    /// "N more" row; `None` for a node that is not live.
    pub fn reveal(
        &mut self,
        nodes: &mut Vec<TreeNode>,
        tree: &Tree,
        node: NodeId,
    ) -> Option<usize> {
        if !tree.is_live(node) {
            return None;
        }
        let mut path: Vec<NodeId> = tree.ancestors(node).collect();
        path.reverse();
        for folder in path {
            let Some(&idx) = self.index.get(&folder) else {
                // A folder on the way is itself past its parent's cap.
                let parent = tree.node(folder)?.parent();
                return self.more.get(&parent).copied();
            };
            self.populate(nodes, tree, idx);
            nodes[idx].is_expanded = true;
        }
        match self.index.get(&node) {
            Some(&idx) => Some(idx),
            None => {
                let parent = tree.node(node)?.parent();
                self.more.get(&parent).copied()
            }
        }
    }

    /// Expand `folder` and every folder above it.
    pub fn expand_to(&mut self, nodes: &mut Vec<TreeNode>, tree: &Tree, folder: NodeId) {
        if let Some(idx) = self.reveal(nodes, tree, folder) {
            if self.node_at(idx) == Some(folder) {
                self.populate(nodes, tree, idx);
                nodes[idx].is_expanded = true;
            }
        }
    }

    /// Follow `TreeView::remove_children` (old row → new row, `None` for removed rows):
    /// drop the removed rows and renumber the rest. The folder whose rows were removed is
    /// `folder` (old index); it is marked unpopulated, so opening it adds them again.
    pub fn remap(
        &mut self,
        nodes: &mut [TreeNode],
        tree: &Tree,
        folder: usize,
        map: &[Option<usize>],
    ) {
        let mut entries = vec![
            Entry::More {
                parent: NodeId::NONE
            };
            nodes.len()
        ];
        for (old, entry) in self.entries.iter().enumerate() {
            if let Some(Some(new)) = map.get(old) {
                if let Some(slot) = entries.get_mut(*new) {
                    *slot = *entry;
                }
            }
        }
        self.entries = entries;
        let renumber = |m: &mut HashMap<NodeId, usize>| {
            m.retain(|_, idx| match map.get(*idx).copied().flatten() {
                Some(new) => {
                    *idx = new;
                    true
                }
                None => false,
            });
        };
        renumber(&mut self.index);
        renumber(&mut self.more);
        if let Some(new) = map.get(folder).copied().flatten() {
            if let Some(Entry::Node { id, .. }) = self.entries.get(new).copied() {
                self.entries[new] = Entry::Node {
                    id,
                    populated: false,
                };
                nodes[new].may_have_children = has_live_children(tree, id);
            }
        }
    }

    /// Where row `idx` is among the visible rows (top first), or `None` when a folder above
    /// it is collapsed. Same order as the `TreeView`'s own: depth first, siblings by
    /// `order`. O(rows); for tests and diagnostics (the `TreeView` caches its own).
    pub fn visible_position(nodes: &[TreeNode], idx: usize) -> Option<usize> {
        let mut kids: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
        let mut roots = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            match n.parent {
                Some(p) if p < nodes.len() => kids[p].push(i),
                Some(_) => {}
                None => roots.push(i),
            }
        }
        for list in kids.iter_mut().chain(std::iter::once(&mut roots)) {
            list.sort_by_key(|&i| nodes[i].order);
        }
        let mut stack: Vec<usize> = roots.into_iter().rev().collect();
        let mut position = 0;
        while let Some(i) = stack.pop() {
            if i == idx {
                return Some(position);
            }
            position += 1;
            if nodes[i].is_expanded {
                stack.extend(kids[i].iter().rev());
            }
        }
        None
    }

    fn add_node(
        &mut self,
        nodes: &mut Vec<TreeNode>,
        tree: &Tree,
        id: NodeId,
        parent: Option<usize>,
        order: u32,
    ) -> usize {
        let is_dir = tree.node(id).is_some_and(|n| n.is_dir());
        let colors = glyphs();
        let (icon, glyph) = if is_dir {
            (NodeIcon::Folder, NodeGlyph::new(fa::FOLDER, colors.folder))
        } else {
            (NodeIcon::File, NodeGlyph::new(fa::FILE_O, colors.file))
        };
        let mut row = TreeNode::new(tree.name(id), icon, parent, order);
        row.icon_glyph = Some(glyph);
        let size = tree.size(id);
        row.secondary_text = Some(format_bytes(size));
        let parent_id = tree.node(id).map_or(NodeId::NONE, |n| n.parent());
        if !parent_id.is_none() {
            row.fraction = Some(share(size, tree.size(parent_id)));
        }
        let has_children = is_dir && has_live_children(tree, id);
        row.may_have_children = has_children;
        let idx = push(nodes, row);
        self.set_entry(
            idx,
            Entry::Node {
                id,
                populated: !has_children,
            },
        );
        self.index.insert(id, idx);
        idx
    }

    fn set_entry(&mut self, idx: usize, entry: Entry) {
        if idx >= self.entries.len() {
            self.entries.resize(
                idx + 1,
                Entry::More {
                    parent: NodeId::NONE,
                },
            );
        }
        self.entries[idx] = entry;
    }
}

fn has_live_children(tree: &Tree, id: NodeId) -> bool {
    tree.children(id).any(|c| tree.is_live(c))
}

/// `part` as a fraction of `whole` (0 for an empty whole).
fn share(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64).clamp(0.0, 1.0) as f32
    }
}

/// Append `row`, returning its index.
fn push(nodes: &mut Vec<TreeNode>, row: TreeNode) -> usize {
    nodes.push(row);
    nodes.len() - 1
}

/// The root row's label: the drive's name when a drive was scanned, else the scanned
/// folder's own name (the whole path only for a root such as `/` that has none).
pub fn root_label(tree: &Tree, volume_name: Option<&str>) -> String {
    if let Some(name) = volume_name {
        return name.to_string();
    }
    let path = tree.path(tree.root());
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => path.to_string_lossy().into_owned(),
    }
}
