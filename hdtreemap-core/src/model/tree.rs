//! The scanned tree: an arena of [`Node`]s indexed by [`NodeId`], one `String` slab holding
//! every name, and the extension list (`docs/PLAN.md` §3.1).
//!
//! Trees are grown by `model/builder.rs` (both scan sources feed it) and then read by the
//! UI: children in size order, a node's path, the largest folders and files. The tree also
//! owns the two edits made after a scan: switching the [`SizeMode`] (a re-sort, no rescan)
//! and [`Tree::remove_subtree`] (Move to Trash), which subtracts the subtree's sizes from
//! every ancestor so totals stay exact without rescanning.

use std::collections::HashMap;
use std::path::PathBuf;

use super::node::{Node, NodeFlags, NodeId, SizeMode, Sizes, NO_EXT};

/// Why [`Tree::remove_subtree`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoveError {
    /// The scan root cannot be removed from its own tree.
    Root,
    /// No such node, or it was already removed.
    Missing,
}

/// A scanned file tree. See the module docs.
#[derive(Clone, Debug)]
pub struct Tree {
    pub(crate) nodes: Vec<Node>,
    /// Every node's name, back to back in node order (a node's name ends where the next
    /// node's begins).
    pub(crate) names: String,
    /// Extension names by id; id 0 ([`NO_EXT`]) is the empty string.
    pub(crate) ext_names: Vec<String>,
    pub(crate) ext_ids: HashMap<String, u16>,
    pub(crate) mode: SizeMode,
    /// Live (not removed) directories, the root included.
    pub(crate) dirs: u64,
    /// Most bytes the name slab may hold: node offsets into it are `u32`.
    pub(crate) name_limit: usize,
}

/// The name slab's capacity: every name must start and end within a `u32` offset.
pub const NAME_SLAB_LIMIT: usize = u32::MAX as usize;

impl Tree {
    /// A tree holding only a root directory named `root_name` (the scanned path), whose
    /// names may take up to `name_limit` bytes in all (at most [`NAME_SLAB_LIMIT`]).
    // On 32-bit targets (wasm) the cap is `usize::MAX`, so `min` changes nothing there.
    #[allow(clippy::unnecessary_min_or_max)]
    pub(crate) fn with_root(root_name: &str, name_limit: usize) -> Self {
        let mut tree = Self {
            nodes: Vec::new(),
            names: String::new(),
            ext_names: vec![String::new()],
            ext_ids: HashMap::new(),
            mode: SizeMode::default(),
            dirs: 0,
            name_limit: name_limit.min(NAME_SLAB_LIMIT),
        };
        // A root name too long for the limit is cut (on a character boundary) rather than
        // leaving a tree without a root.
        let mut cut = root_name.len().min(tree.name_limit);
        while !root_name.is_char_boundary(cut) {
            cut -= 1;
        }
        let _ = tree.push_node(NodeId::NONE, &root_name[..cut], NodeFlags::DIR);
        tree
    }

    /// Append a node (unlinked) and its name; returns its id, or `None` when the name would
    /// end past the name slab's limit or the arena is out of ids (the tree is full).
    pub(crate) fn push_node(
        &mut self,
        parent: NodeId,
        name: &str,
        flags: NodeFlags,
    ) -> Option<NodeId> {
        let end = self.names.len().checked_add(name.len())?;
        if end > self.name_limit {
            return None;
        }
        let id = u32::try_from(self.nodes.len())
            .ok()
            .filter(|&i| i != NodeId::NONE.0)?;
        let start = u32::try_from(self.names.len()).ok()?;
        self.names.push_str(name);
        self.nodes.push(Node::new(parent, start, flags));
        if flags.contains(NodeFlags::DIR) {
            self.dirs += 1;
        }
        Some(NodeId(id))
    }

    /// The scan root (always node 0).
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Nodes in the arena, removed ones included.
    pub fn arena_len(&self) -> usize {
        self.nodes.len()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// The node's own name (the full scanned path for the root). Empty for an unknown id.
    pub fn name(&self, id: NodeId) -> &str {
        let Some(node) = self.nodes.get(id.index()) else {
            return "";
        };
        let start = node.name_start as usize;
        let end = self
            .nodes
            .get(id.index() + 1)
            .map_or(self.names.len(), |next| next.name_start as usize);
        self.names.get(start..end).unwrap_or("")
    }

    /// The size the active mode reads (0 for an unknown id).
    pub fn size(&self, id: NodeId) -> u64 {
        self.node(id).map_or(0, |n| n.size(self.mode))
    }

    pub fn sizes(&self, id: NodeId) -> Sizes {
        self.node(id).map_or_else(Sizes::default, Node::sizes)
    }

    pub fn size_mode(&self) -> SizeMode {
        self.mode
    }

    /// Switch the active size and re-sort every child list by it.
    pub fn set_size_mode(&mut self, mode: SizeMode) {
        if self.mode != mode {
            self.mode = mode;
            self.sort_children();
        }
    }

    /// Total of the whole tree in both modes.
    pub fn total(&self) -> Sizes {
        self.sizes(self.root())
    }

    /// Live files in the tree.
    pub fn file_count(&self) -> u64 {
        self.node(self.root())
            .map_or(0, |n| u64::from(n.file_count))
    }

    /// Live directories below the root (the root itself is not counted).
    pub fn dir_count(&self) -> u64 {
        self.dirs.saturating_sub(1)
    }

    /// The extension name for an extension id (`""` for none).
    pub fn ext_name(&self, ext: u16) -> &str {
        self.ext_names
            .get(usize::from(ext))
            .map_or("", String::as_str)
    }

    /// Number of extension ids (including [`NO_EXT`]).
    pub fn ext_count(&self) -> usize {
        self.ext_names.len()
    }

    /// The id for `ext`, interning it. Past `u16::MAX` distinct extensions the rest share
    /// [`NO_EXT`].
    pub(crate) fn intern_ext(&mut self, ext: &str) -> u16 {
        if let Some(&id) = self.ext_ids.get(ext) {
            return id;
        }
        let Ok(id) = u16::try_from(self.ext_names.len()) else {
            return NO_EXT;
        };
        self.ext_names.push(ext.to_string());
        self.ext_ids.insert(ext.to_string(), id);
        id
    }

    /// The node's children, in the current order (size-descending once finished).
    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children {
            tree: self,
            next: self.node(id).map_or(NodeId::NONE, |n| n.first_child),
        }
    }

    /// The node's live children by the active size, largest first (ties by name): the
    /// finished tree's order, also for a tree still being built, whose children are not
    /// sorted yet (the live map's folder tree).
    pub fn children_by_size(&self, id: NodeId) -> Vec<NodeId> {
        let mut kids: Vec<NodeId> = self.children(id).filter(|&c| self.is_live(c)).collect();
        let mode = self.mode;
        kids.sort_by(|&a, &b| {
            self.size_of(b, mode)
                .cmp(&self.size_of(a, mode))
                .then_with(|| self.name(a).cmp(self.name(b)))
        });
        kids
    }

    fn size_of(&self, id: NodeId, mode: SizeMode) -> u64 {
        self.node(id).map_or(0, |n| n.size(mode))
    }

    /// Number of steps from the root (0 for the root).
    pub fn depth(&self, id: NodeId) -> usize {
        self.ancestors(id).count()
    }

    /// The node's ancestors, nearest first, ending at the root.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let mut current = self.node(id).map_or(NodeId::NONE, |n| n.parent);
        std::iter::from_fn(move || {
            if current.is_none() {
                return None;
            }
            let here = current;
            current = self.node(here).map_or(NodeId::NONE, |n| n.parent);
            Some(here)
        })
    }

    /// The full filesystem path of a node (the root's name is the scanned path).
    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut parts: Vec<&str> = vec![self.name(id)];
        parts.extend(self.ancestors(id).map(|a| self.name(a)));
        let mut path = PathBuf::new();
        for part in parts.iter().rev() {
            path.push(part);
        }
        path
    }

    /// The node's real path, for handing to the OS (reveal, open, trash, rescan): `None`
    /// when the node or a folder above it has a [`NodeFlags::LOSSY_NAME`], because the
    /// stored name is not the name on disk and acting on it could hit the wrong file.
    pub fn exact_path(&self, id: NodeId) -> Option<PathBuf> {
        (self.node(id).is_some() && !self.has_lossy_path(id)).then(|| self.path(id))
    }

    /// Whether the node or any folder above it has a lossily converted name.
    pub fn has_lossy_path(&self, id: NodeId) -> bool {
        std::iter::once(id).chain(self.ancestors(id)).any(|n| {
            self.node(n)
                .is_some_and(|n| n.flags.contains(NodeFlags::LOSSY_NAME))
        })
    }

    /// The path below the root with `/` separators (`""` for the root), for display.
    pub fn relative_path(&self, id: NodeId) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut current = id;
        while let Some(node) = self.node(current) {
            if node.parent.is_none() {
                break;
            }
            parts.push(self.name(current));
            current = node.parent;
        }
        parts.reverse();
        parts.join("/")
    }

    /// Sort every live directory's children by the active size, largest first (ties by
    /// name, so the order is deterministic).
    pub fn sort_children(&mut self) {
        let mut kids: Vec<NodeId> = Vec::new();
        for index in 0..self.nodes.len() {
            self.sort_child_list(NodeId(index as u32), &mut kids);
        }
    }

    /// Sort one live directory's children like [`Tree::sort_children`]. `kids` is scratch
    /// space, reused across calls.
    pub(crate) fn sort_child_list(&mut self, id: NodeId, kids: &mut Vec<NodeId>) {
        let Some(node) = self.nodes.get(id.index()) else {
            return;
        };
        if !node.is_dir() || node.flags.contains(NodeFlags::REMOVED) {
            return;
        }
        kids.clear();
        let mut child = node.first_child;
        while let Some(c) = self.nodes.get(child.index()) {
            kids.push(child);
            child = c.next_sibling;
        }
        if kids.len() < 2 {
            return;
        }
        let mode = self.mode;
        kids.sort_by(|&a, &b| {
            let (na, nb) = (&self.nodes[a.index()], &self.nodes[b.index()]);
            nb.size(mode)
                .cmp(&na.size(mode))
                .then_with(|| self.name(a).cmp(self.name(b)))
        });
        self.nodes[id.index()].first_child = kids[0];
        for pair in kids.windows(2) {
            self.nodes[pair[0].index()].next_sibling = pair[1];
        }
        if let Some(&last) = kids.last() {
            self.nodes[last.index()].next_sibling = NodeId::NONE;
        }
    }

    /// Remove `id` and everything below it (after Move to Trash): unlink it from its parent
    /// and subtract its sizes and file count from every ancestor. Returns the sizes removed.
    pub fn remove_subtree(&mut self, id: NodeId) -> Result<Sizes, RemoveError> {
        let node = self.node(id).ok_or(RemoveError::Missing)?;
        if node.flags.contains(NodeFlags::REMOVED) {
            return Err(RemoveError::Missing);
        }
        let parent = node.parent;
        if parent.is_none() {
            return Err(RemoveError::Root);
        }
        let removed = node.sizes();
        let files = node.file_count;

        // Unlink from the parent's sibling list.
        let mut prev = NodeId::NONE;
        let mut cur = self.nodes[parent.index()].first_child;
        while !cur.is_none() && cur != id {
            prev = cur;
            cur = self.nodes[cur.index()].next_sibling;
        }
        let next = self.nodes[id.index()].next_sibling;
        if prev.is_none() {
            self.nodes[parent.index()].first_child = next;
        } else {
            self.nodes[prev.index()].next_sibling = next;
        }
        self.nodes[id.index()].next_sibling = NodeId::NONE;

        // Flag the whole subtree, so arena-wide passes (the extension table) skip it.
        self.flag_removed(id);

        let mut up = parent;
        while let Some(a) = self.nodes.get_mut(up.index()) {
            a.allocated = a.allocated.saturating_sub(removed.allocated);
            a.apparent = a.apparent.saturating_sub(removed.apparent);
            a.file_count = a.file_count.saturating_sub(files);
            up = a.parent;
        }
        Ok(removed)
    }

    /// Flag `id` and everything below it [`NodeFlags::REMOVED`] (they stay linked to each
    /// other; the caller unlinks `id`) and stop counting their directories.
    pub(crate) fn flag_removed(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let Some(node) = self.nodes.get_mut(n.index()) else {
                continue;
            };
            node.flags.insert(NodeFlags::REMOVED);
            if node.is_dir() {
                self.dirs = self.dirs.saturating_sub(1);
            }
            stack.extend(self.children(n));
        }
    }

    /// Whether `id` is a live (not removed) node of this tree.
    pub fn is_live(&self, id: NodeId) -> bool {
        self.node(id)
            .is_some_and(|n| !n.flags.contains(NodeFlags::REMOVED))
    }

    /// The `n` largest live files by the active size, largest first.
    pub fn largest_files(&self, n: usize) -> Vec<NodeId> {
        self.top_by_size(n, |node| !node.is_dir())
    }

    /// The `n` largest live folders below the root, largest first, leaving out "wrapper"
    /// folders whose single biggest subfolder holds at least 90% of them (so a chain like
    /// `Users` → `Users/me` → `Users/me/Library` lists the folder that matters, not every
    /// step down to it).
    pub fn largest_folders(&self, n: usize) -> Vec<NodeId> {
        let mode = self.mode;
        let root = self.root();
        let mut candidates: Vec<NodeId> = (0..self.nodes.len() as u32)
            .map(NodeId)
            .filter(|&id| {
                let node = &self.nodes[id.index()];
                id != root
                    && node.is_dir()
                    && !node.flags.contains(NodeFlags::REMOVED)
                    && node.size(mode) > 0
                    && !self.is_wrapper(id)
            })
            .collect();
        self.keep_largest(&mut candidates, n);
        candidates
    }

    /// A folder dominated (≥ 90%) by one subfolder.
    fn is_wrapper(&self, id: NodeId) -> bool {
        let total = self.size(id);
        let biggest_dir = self
            .children(id)
            .filter(|&c| self.nodes[c.index()].is_dir())
            .map(|c| self.size(c))
            .max()
            .unwrap_or(0);
        total > 0 && biggest_dir as u128 * 10 >= total as u128 * 9
    }

    fn top_by_size(&self, n: usize, keep: impl Fn(&Node) -> bool) -> Vec<NodeId> {
        let mut ids: Vec<NodeId> = (0..self.nodes.len() as u32)
            .map(NodeId)
            .filter(|&id| {
                let node = &self.nodes[id.index()];
                !node.flags.contains(NodeFlags::REMOVED) && keep(node)
            })
            .collect();
        self.keep_largest(&mut ids, n);
        ids
    }

    /// Keep the `n` largest of `ids`, sorted largest first (ties by arena order).
    fn keep_largest(&self, ids: &mut Vec<NodeId>, n: usize) {
        let order =
            |a: &NodeId, b: &NodeId| self.size(*b).cmp(&self.size(*a)).then_with(|| a.cmp(b));
        if n == 0 {
            ids.clear();
            return;
        }
        if ids.len() > n {
            ids.select_nth_unstable_by(n - 1, order);
            ids.truncate(n);
        }
        ids.sort_by(order);
    }
}

/// Iterator over a node's children (see [`Tree::children`]).
pub struct Children<'a> {
    tree: &'a Tree,
    next: NodeId,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let node = self.tree.node(self.next)?;
        let here = self.next;
        self.next = node.next_sibling;
        Some(here)
    }
}
