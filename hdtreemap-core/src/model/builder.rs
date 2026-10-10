//! `TreeBuilder`: grows a [`Tree`] one entry at a time while a scan runs.
//!
//! Both scan sources feed it: the filesystem walker (`scan/fs.rs`) as entries arrive from
//! jwalk, and the demo generator (`demo_fs`) as it streams a synthetic tree. Every file's
//! sizes are added to all of its ancestors as soon as it is added, so a partial tree always
//! has correct partial totals (the live map during a scan reads them). [`TreeBuilder::finish`]
//! sorts every child list by the chosen size and hands the tree over.

use super::node::{extension_of, NodeFlags, NodeId, SizeMode, Sizes, NO_EXT};
use super::tree::{Tree, NAME_SLAB_LIMIT};

/// Builds a [`Tree`] incrementally. See the module docs.
#[derive(Debug)]
pub struct TreeBuilder {
    tree: Tree,
    /// An entry did not fit (see [`TreeBuilder::is_full`]).
    full: bool,
}

impl TreeBuilder {
    /// Start a tree whose root directory is `root_name` (the scanned path).
    pub fn new(root_name: &str) -> Self {
        Self::with_name_limit(root_name, NAME_SLAB_LIMIT)
    }

    /// Start a tree whose names may take at most `limit` bytes in all (capped at
    /// [`NAME_SLAB_LIMIT`]); tests use a small limit to reach "full" quickly.
    pub fn with_name_limit(root_name: &str, limit: usize) -> Self {
        Self {
            tree: Tree::with_root(root_name, limit),
            full: false,
        }
    }

    /// Whether an entry was refused because the tree is full (its names reached the name
    /// slab's limit, about 4 GiB). Every add after that is refused too and returns
    /// [`NodeId::NONE`]; a scan should stop and say so.
    pub fn is_full(&self) -> bool {
        self.full
    }

    pub fn root(&self) -> NodeId {
        self.tree.root()
    }

    /// The tree so far: unsorted children, but correct partial totals.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Add an (empty) directory named `name` under `parent` ([`NodeId::NONE`] when full).
    pub fn add_dir(&mut self, parent: NodeId, name: &str) -> NodeId {
        self.add(parent, name, NodeFlags::DIR)
    }

    /// Add a file named `name` under `parent` with its sizes, propagating them (and one to
    /// the file count) up to the root. `flags` may add [`NodeFlags::SYMLINK`] /
    /// [`NodeFlags::ERROR`]; [`NodeFlags::DIR`] is ignored.
    pub fn add_file(
        &mut self,
        parent: NodeId,
        name: &str,
        sizes: Sizes,
        flags: NodeFlags,
    ) -> NodeId {
        let flags = flags.without(NodeFlags::DIR);
        let id = self.add(parent, name, flags);
        if id.is_none() {
            return id;
        }
        let ext = match extension_of(name) {
            Some(ext) => self.tree.intern_ext(&ext),
            None => NO_EXT,
        };
        let node = &mut self.tree.nodes[id.index()];
        node.ext = ext;
        node.allocated = sizes.allocated;
        node.apparent = sizes.apparent;
        node.file_count = 1;
        let mut up = parent;
        while let Some(a) = self.tree.nodes.get_mut(up.index()) {
            a.allocated = a.allocated.saturating_add(sizes.allocated);
            a.apparent = a.apparent.saturating_add(sizes.apparent);
            a.file_count = a.file_count.saturating_add(1);
            up = a.parent;
        }
        id
    }

    /// Flag `node` (an unreadable directory, or an entry whose metadata failed).
    pub fn mark(&mut self, node: NodeId, flags: NodeFlags) {
        if let Some(n) = self.tree.nodes.get_mut(node.index()) {
            n.flags.insert(flags.without(NodeFlags::DIR));
        }
    }

    /// The size the partial tree reads (its children stay unsorted until [`Self::finish`]).
    pub fn set_size_mode(&mut self, mode: SizeMode) {
        self.tree.mode = mode;
    }

    pub fn size_mode(&self) -> SizeMode {
        self.tree.mode
    }

    /// Sort every child list by `mode`, largest first, and return the finished tree.
    pub fn finish(mut self, mode: SizeMode) -> Tree {
        self.tree.mode = mode;
        self.tree.sort_children();
        self.tree
    }

    fn add(&mut self, parent: NodeId, name: &str, flags: NodeFlags) -> NodeId {
        if self.full {
            return NodeId::NONE;
        }
        // An unknown parent would orphan the entry; hang it under the root instead.
        let parent = if self.tree.node(parent).is_some() {
            parent
        } else {
            self.tree.root()
        };
        let Some(id) = self.tree.push_node(parent, name, flags) else {
            self.full = true;
            return NodeId::NONE;
        };
        let first = self.tree.nodes[parent.index()].first_child;
        self.tree.nodes[id.index()].next_sibling = first;
        self.tree.nodes[parent.index()].first_child = id;
        id
    }
}
