//! The folder tree's lazy row model: which scanned nodes have a `TreeView` node, and the
//! code that adds them.
//!
//! A `TreeView` lists every node it holds, so it is never handed the whole scan (millions of
//! files). It starts with the scan root, and a folder's children are added only when the
//! folder is expanded ([`Rows::populate`]). Until then an unexpanded folder carries one
//! hidden stand-in child (an [`Entry::Placeholder`]) so the view draws its expand arrow; on
//! expansion the stand-in's slot becomes the first real child (`TreeView` nodes are
//! addressed by index, so nothing is ever removed). Each folder lists at most [`CHILD_CAP`]
//! children, largest first (the tree keeps children sorted by size), and one
//! [`Entry::More`] row summing up the rest.
//!
//! `entries` runs parallel to `TreeView::nodes`; `index` maps a scanned node to its row.

use std::collections::HashMap;

use agg_gui::widgets::tree_view::TreeNode;
use agg_gui::NodeIcon;

use super::icons::TreeIcons;
use crate::format::{format_bytes, format_count, format_percent};
use crate::model::{NodeId, Tree};

/// Most children a folder lists; the rest share one "N more" row.
pub const CHILD_CAP: usize = 200;

/// What a `TreeView` node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A scanned file or folder. `placeholder` is the stand-in child of a folder whose
    /// children have not been added yet.
    Node {
        id: NodeId,
        populated: bool,
        placeholder: Option<usize>,
    },
    /// The children of `parent` past [`CHILD_CAP`].
    More { parent: NodeId },
    /// The stand-in child of an unexpanded folder (never shown).
    Placeholder,
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
    /// Start over with just the scan root (collapsed).
    pub fn reset(&mut self, nodes: &mut Vec<TreeNode>, tree: &Tree, icons: &TreeIcons) {
        nodes.clear();
        self.entries.clear();
        self.index.clear();
        self.more.clear();
        self.add_node(nodes, tree, icons, tree.root(), None, 0, None);
    }

    /// The scanned node a `TreeView` node shows (`None` for "more" rows and stand-ins).
    pub fn node_at(&self, idx: usize) -> Option<NodeId> {
        match self.entries.get(idx)? {
            Entry::Node { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// Add the children of every expanded folder that does not have them yet. Returns
    /// whether anything was added.
    pub fn populate_expanded(
        &mut self,
        nodes: &mut Vec<TreeNode>,
        tree: &Tree,
        icons: &TreeIcons,
    ) -> bool {
        let pending: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                nodes.get(i).is_some_and(|n| n.is_expanded)
                    && matches!(
                        self.entries[i],
                        Entry::Node {
                            populated: false,
                            ..
                        }
                    )
            })
            .collect();
        for &idx in &pending {
            self.populate(nodes, tree, icons, idx);
        }
        !pending.is_empty()
    }

    /// Add the children of the folder at `idx` (once).
    pub fn populate(
        &mut self,
        nodes: &mut Vec<TreeNode>,
        tree: &Tree,
        icons: &TreeIcons,
        idx: usize,
    ) {
        let Some(Entry::Node {
            id,
            populated: false,
            placeholder,
        }) = self.entries.get(idx).copied()
        else {
            return;
        };
        self.entries[idx] = Entry::Node {
            id,
            populated: true,
            placeholder: None,
        };
        let parent_size = tree.size(id);
        let mut slot = placeholder;
        let mut rest_count = 0u64;
        let mut rest_bytes = 0u64;
        let mut rest_dirs = false;
        let mut rest_files = false;
        for (order, child) in tree.children(id).filter(|&c| tree.is_live(c)).enumerate() {
            if order < CHILD_CAP {
                self.add_node(
                    nodes,
                    tree,
                    icons,
                    child,
                    Some(idx),
                    order as u32,
                    slot.take(),
                );
            } else {
                rest_count += 1;
                rest_bytes += tree.size(child);
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
            let label = format!(
                "{} more {noun}  \u{b7}  {}  \u{b7}  {}",
                format_count(rest_count),
                format_bytes(rest_bytes),
                format_percent(rest_bytes, parent_size)
            );
            let more = place(
                nodes,
                slot.take(),
                tree_node(
                    label,
                    NodeIcon::File,
                    icons.more.clone(),
                    idx,
                    CHILD_CAP as u32,
                ),
            );
            self.set_entry(more, Entry::More { parent: id });
            self.more.insert(id, more);
        }
        if let Some(unused) = slot {
            // Everything in the folder was removed since its row was added.
            nodes[unused].label = "(empty)".to_string();
            nodes[unused].icon_image = icons.more.clone();
            self.set_entry(unused, Entry::Placeholder);
        }
    }

    /// Make `node` reachable: add and expand every folder above it. Returns the row that
    /// shows it, or, when it (or a folder above it) is past a folder's cap, that folder's
    /// "N more" row; `None` for a node that is not live.
    pub fn reveal(
        &mut self,
        nodes: &mut Vec<TreeNode>,
        tree: &Tree,
        icons: &TreeIcons,
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
            self.populate(nodes, tree, icons, idx);
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
    pub fn expand_to(
        &mut self,
        nodes: &mut Vec<TreeNode>,
        tree: &Tree,
        icons: &TreeIcons,
        folder: NodeId,
    ) {
        if let Some(idx) = self.reveal(nodes, tree, icons, folder) {
            if self.node_at(idx) == Some(folder) {
                self.populate(nodes, tree, icons, idx);
                nodes[idx].is_expanded = true;
            }
        }
    }

    /// Where row `idx` is among the visible rows (top first), or `None` when a folder above
    /// it is collapsed. Same order as the `TreeView`'s own: depth first, siblings by
    /// `order`.
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

    #[allow(clippy::too_many_arguments)]
    fn add_node(
        &mut self,
        nodes: &mut Vec<TreeNode>,
        tree: &Tree,
        icons: &TreeIcons,
        id: NodeId,
        parent: Option<usize>,
        order: u32,
        slot: Option<usize>,
    ) -> usize {
        let is_dir = tree.node(id).is_some_and(|n| n.is_dir());
        let (icon, image) = if is_dir {
            (NodeIcon::Folder, icons.folder.clone())
        } else {
            (NodeIcon::File, icons.file.clone())
        };
        let mut node = TreeNode::new(row_label(tree, id), icon, parent, order);
        node.icon_image = image;
        let idx = place(nodes, slot, node);
        let has_children = is_dir && tree.children(id).any(|c| tree.is_live(c));
        let placeholder = has_children.then(|| {
            let stand_in = TreeNode::new(String::new(), NodeIcon::File, Some(idx), 0);
            let p = place(nodes, None, stand_in);
            self.set_entry(p, Entry::Placeholder);
            p
        });
        self.set_entry(
            idx,
            Entry::Node {
                id,
                populated: !has_children,
                placeholder,
            },
        );
        self.index.insert(id, idx);
        idx
    }

    fn set_entry(&mut self, idx: usize, entry: Entry) {
        if idx >= self.entries.len() {
            self.entries.resize(idx + 1, Entry::Placeholder);
        }
        self.entries[idx] = entry;
    }
}

/// `"Documents  ·  12.3 GB  ·  45.2%"`: name, size in the active mode, share of the parent
/// folder. The scan root shows its path and size.
pub fn row_label(tree: &Tree, id: NodeId) -> String {
    let size = tree.size(id);
    let parent = tree.node(id).map_or(NodeId::NONE, |n| n.parent());
    if parent.is_none() {
        return format!("{}  \u{b7}  {}", tree.name(id), format_bytes(size));
    }
    format!(
        "{}  \u{b7}  {}  \u{b7}  {}",
        tree.name(id),
        format_bytes(size),
        format_percent(size, tree.size(parent))
    )
}

fn tree_node(
    label: String,
    icon: NodeIcon,
    image: Option<agg_gui::IconImage>,
    parent: usize,
    order: u32,
) -> TreeNode {
    let mut node = TreeNode::new(label, icon, Some(parent), order);
    node.icon_image = image;
    node
}

/// Put `node` into `slot` (reusing a stand-in's index) or append it. Returns its index.
fn place(nodes: &mut Vec<TreeNode>, slot: Option<usize>, node: TreeNode) -> usize {
    match slot.filter(|&s| s < nodes.len()) {
        Some(s) => {
            nodes[s] = node;
            s
        }
        None => {
            nodes.push(node);
            nodes.len() - 1
        }
    }
}
