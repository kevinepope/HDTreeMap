//! Rescan Folder's tree edit: [`Tree::replace_subtree`] puts a freshly scanned copy of one
//! folder in place of what the tree held for it, so a rescan of one folder does not mean
//! rescanning the drive (`docs/PLAN.md` §2, Rescan).
//!
//! The arena only grows: the folder's old contents are unlinked and flagged removed (like
//! Move to Trash), and the new contents are appended (nodes, names, and any extensions the
//! tree had not seen). The folder node itself stays, so its id, its row in the folder tree
//! and anything zoomed into or selected at it remain valid; its sizes become the new
//! scan's, and every folder above it gains or loses the difference.

use super::node::{NodeFlags, NodeId, Sizes};
use super::tree::Tree;

/// Why [`Tree::replace_subtree`] refused. Nothing is changed when it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpliceError {
    /// The folder is not in the tree, was removed, or the source node is not in the source.
    Missing,
    /// The node to replace, or the source node, is a file.
    NotAFolder,
    /// The tree's name slab cannot hold the new names (see `Tree` / `NAME_SLAB_LIMIT`).
    Full,
}

impl Tree {
    /// Replace the contents of folder `node` with those of folder `source_root` in
    /// `source`, adjusting the sizes and file counts of `node` and every folder above it.
    /// Returns `node`'s sizes before the splice. See the module docs.
    pub fn replace_subtree(
        &mut self,
        node: NodeId,
        source: &Tree,
        source_root: NodeId,
    ) -> Result<Sizes, SpliceError> {
        let target = self.node(node).ok_or(SpliceError::Missing)?;
        if target.flags.contains(NodeFlags::REMOVED) {
            return Err(SpliceError::Missing);
        }
        if !target.is_dir() {
            return Err(SpliceError::NotAFolder);
        }
        let new_root = source.node(source_root).ok_or(SpliceError::Missing)?;
        if !new_root.is_dir() {
            return Err(SpliceError::NotAFolder);
        }
        let (copied, name_bytes) = subtree_extent(source, source_root);
        let fits_names = self
            .names
            .len()
            .checked_add(name_bytes)
            .is_some_and(|end| end <= self.name_limit);
        let fits_ids = self
            .nodes
            .len()
            .checked_add(copied)
            .is_some_and(|n| n < NodeId::NONE.index());
        if !fits_names || !fits_ids {
            return Err(SpliceError::Full);
        }

        let old = target.sizes();
        let old_files = target.file_count;
        let new = new_root.sizes();
        let new_files = new_root.file_count;
        let new_error = new_root.flags.contains(NodeFlags::ERROR);

        // Drop the old contents.
        let old_children: Vec<NodeId> = self.children(node).collect();
        for child in old_children {
            self.flag_removed(child);
        }
        self.nodes[node.index()].first_child = NodeId::NONE;

        // Copy the new ones, keeping the source's sibling order.
        let ext_map: Vec<u16> = (0..source.ext_count())
            .map(|e| match source.ext_name(e as u16) {
                "" => 0,
                name => self.intern_ext(name),
            })
            .collect();
        let mut copied_dirs = vec![node];
        let mut stack = vec![(source_root, node)];
        while let Some((from, to)) = stack.pop() {
            let mut prev = NodeId::NONE;
            for child in source.children(from) {
                let Some(src) = source.node(child) else {
                    continue;
                };
                // Checked above: the names and ids fit.
                let Some(id) = self.push_node(to, source.name(child), src.flags) else {
                    return Err(SpliceError::Full);
                };
                let dst = &mut self.nodes[id.index()];
                dst.allocated = src.allocated;
                dst.apparent = src.apparent;
                dst.file_count = src.file_count;
                dst.ext = ext_map.get(usize::from(src.ext)).copied().unwrap_or(0);
                if prev.is_none() {
                    self.nodes[to.index()].first_child = id;
                } else {
                    self.nodes[prev.index()].next_sibling = id;
                }
                prev = id;
                if src.is_dir() {
                    copied_dirs.push(id);
                    stack.push((child, id));
                }
            }
        }

        // The folder takes the new totals; its ancestors the difference.
        let target = &mut self.nodes[node.index()];
        target.allocated = new.allocated;
        target.apparent = new.apparent;
        target.file_count = new_files;
        target.flags = target.flags.without(NodeFlags::ERROR);
        if new_error {
            target.flags.insert(NodeFlags::ERROR);
        }
        let mut up = target.parent;
        let mut ancestors = Vec::new();
        while let Some(a) = self.nodes.get_mut(up.index()) {
            a.allocated = a
                .allocated
                .saturating_sub(old.allocated)
                .saturating_add(new.allocated);
            a.apparent = a
                .apparent
                .saturating_sub(old.apparent)
                .saturating_add(new.apparent);
            a.file_count = a
                .file_count
                .saturating_sub(old_files)
                .saturating_add(new_files);
            ancestors.push(up);
            up = a.parent;
        }

        // Sizes changed above the folder; the copied lists follow the source's size mode.
        let mut scratch = Vec::new();
        if source.size_mode() != self.size_mode() {
            for dir in copied_dirs {
                self.sort_child_list(dir, &mut scratch);
            }
        }
        for a in ancestors {
            self.sort_child_list(a, &mut scratch);
        }
        Ok(old)
    }
}

/// How many nodes lie below `root` in `tree`, and their names' total length.
fn subtree_extent(tree: &Tree, root: NodeId) -> (usize, usize) {
    let mut count = 0;
    let mut bytes = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        for child in tree.children(id) {
            count += 1;
            bytes += tree.name(child).len();
            stack.push(child);
        }
    }
    (count, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ExtTable, SizeMode, TreeBuilder};

    fn file(b: &mut TreeBuilder, parent: NodeId, name: &str, size: u64) -> NodeId {
        b.add_file(parent, name, Sizes::new(size, size), NodeFlags::empty())
    }

    /// /r: big.iso 1000, docs/ (a.txt 100, old/ b.txt 50), x.txt 10
    fn base() -> (Tree, NodeId, NodeId) {
        let mut b = TreeBuilder::new("/r");
        let root = b.root();
        file(&mut b, root, "big.iso", 1000);
        let docs = b.add_dir(root, "docs");
        file(&mut b, docs, "a.txt", 100);
        let old = b.add_dir(docs, "old");
        file(&mut b, old, "b.txt", 50);
        file(&mut b, root, "x.txt", 10);
        (b.finish(SizeMode::Allocated), docs, old)
    }

    /// A rescan of /r/docs: a.txt grew, old/ is gone, new.pdf and pics/c.jpg appeared.
    fn rescan() -> Tree {
        let mut b = TreeBuilder::new("/r/docs");
        let root = b.root();
        file(&mut b, root, "a.txt", 300);
        file(&mut b, root, "new.pdf", 2000);
        let pics = b.add_dir(root, "pics");
        file(&mut b, pics, "c.jpg", 70);
        b.finish(SizeMode::Allocated)
    }

    fn names(tree: &Tree, id: NodeId) -> Vec<&str> {
        tree.children(id).map(|c| tree.name(c)).collect()
    }

    #[test]
    fn a_splice_replaces_the_contents_and_fixes_every_total() {
        let (mut tree, docs, old) = base();
        let fresh = rescan();
        let before = tree
            .replace_subtree(docs, &fresh, fresh.root())
            .expect("splice");
        assert_eq!(before, Sizes::new(150, 150));
        assert_eq!(tree.sizes(docs), Sizes::new(2370, 2370));
        assert_eq!(tree.total(), Sizes::new(1000 + 2370 + 10, 1000 + 2370 + 10));
        assert_eq!(tree.file_count(), 5);
        assert_eq!(tree.dir_count(), 2, "docs and pics; old is gone");
        assert!(!tree.is_live(old));
        assert_eq!(names(&tree, docs), vec!["new.pdf", "a.txt", "pics"]);
        // docs is now the biggest child of the root: the root's list was re-sorted.
        assert_eq!(names(&tree, tree.root()), vec!["docs", "big.iso", "x.txt"]);
        let pics = tree
            .children(docs)
            .find(|&c| tree.name(c) == "pics")
            .expect("pics");
        let jpg = tree.children(pics).next().expect("c.jpg");
        assert_eq!(
            tree.path(jpg),
            std::path::PathBuf::from("/r/docs/pics/c.jpg")
        );
        assert_eq!(tree.ext_name(tree.node(jpg).expect("node").ext()), "jpg");
        let table = ExtTable::from_tree(&tree);
        assert_eq!(
            table.get("txt").map(|r| (r.count, r.sizes.allocated)),
            Some((2, 310))
        );
        assert_eq!(table.get("pdf").map(|r| r.count), Some(1));
    }

    #[test]
    fn a_splice_can_take_a_folder_from_inside_a_bigger_scan() {
        // The web demo rescans a whole drive and takes the folder from it.
        let (mut tree, docs, _) = base();
        let mut b = TreeBuilder::new("/r");
        let root = b.root();
        let d = b.add_dir(root, "docs");
        file(&mut b, d, "only.md", 5);
        file(&mut b, root, "elsewhere.bin", 99);
        let whole = b.finish(SizeMode::Allocated);
        tree.replace_subtree(docs, &whole, d).expect("splice");
        assert_eq!(names(&tree, docs), vec!["only.md"]);
        assert_eq!(tree.total().allocated, 1000 + 5 + 10);
    }

    #[test]
    fn splicing_the_root_rescans_everything_in_place() {
        let (mut tree, _, _) = base();
        let fresh = rescan();
        let root = tree.root();
        tree.replace_subtree(root, &fresh, fresh.root())
            .expect("splice");
        assert_eq!(tree.total().allocated, 2370);
        assert_eq!(tree.name(root), "/r", "the root keeps its name");
    }

    #[test]
    fn a_source_in_the_other_size_mode_is_sorted_for_this_tree() {
        let (mut tree, docs, _) = base();
        let mut b = TreeBuilder::new("/r/docs");
        let root = b.root();
        // Allocated order: a (400) before b (300); apparent order: b (90) before a (10).
        b.add_file(root, "a", Sizes::new(400, 10), NodeFlags::empty());
        b.add_file(root, "b", Sizes::new(300, 90), NodeFlags::empty());
        let fresh = b.finish(SizeMode::Apparent);
        tree.replace_subtree(docs, &fresh, fresh.root())
            .expect("splice");
        assert_eq!(names(&tree, docs), vec!["a", "b"]);
    }

    #[test]
    fn refusals_change_nothing() {
        let (mut tree, docs, old) = base();
        let fresh = rescan();
        let big = tree.children(tree.root()).next().expect("big.iso");
        assert_eq!(
            tree.replace_subtree(big, &fresh, fresh.root()),
            Err(SpliceError::NotAFolder)
        );
        tree.remove_subtree(old).expect("remove");
        assert_eq!(
            tree.replace_subtree(old, &fresh, fresh.root()),
            Err(SpliceError::Missing)
        );
        let a_file = fresh
            .children(fresh.root())
            .find(|&c| fresh.node(c).is_some_and(|n| !n.is_dir()))
            .expect("a file");
        assert_eq!(
            tree.replace_subtree(docs, &fresh, a_file),
            Err(SpliceError::NotAFolder)
        );
        let total = tree.total();
        let mut tight = TreeBuilder::with_name_limit("/r", 12);
        let root = tight.root();
        tight.add_dir(root, "docs");
        let mut small = tight.finish(SizeMode::Allocated);
        let small_docs = small.children(small.root()).next().expect("docs");
        assert_eq!(
            small.replace_subtree(small_docs, &fresh, fresh.root()),
            Err(SpliceError::Full)
        );
        assert_eq!(tree.total(), total);
    }
}
