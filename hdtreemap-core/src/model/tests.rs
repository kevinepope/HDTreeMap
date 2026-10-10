//! Unit tests for the model: builder size propagation, sorting by either size mode, paths,
//! the extension table, `remove_subtree` and the largest-folders/files queries.

use std::path::PathBuf;

use super::*;

/// /scan
/// ├── docs/            a.pdf (100/90), b.PDF (50/40), notes.txt (5/1)
/// │   └── old/         c.pdf (200/150)
/// ├── video.mp4        (1000/990)
/// └── empty/
fn sample() -> (Tree, [NodeId; 8]) {
    let mut b = TreeBuilder::new("/scan");
    let root = b.root();
    let docs = b.add_dir(root, "docs");
    let a = b.add_file(docs, "a.pdf", Sizes::new(100, 90), NodeFlags::empty());
    let bb = b.add_file(docs, "b.PDF", Sizes::new(50, 40), NodeFlags::empty());
    let notes = b.add_file(docs, "notes.txt", Sizes::new(5, 1), NodeFlags::empty());
    let old = b.add_dir(docs, "old");
    let c = b.add_file(old, "c.pdf", Sizes::new(200, 150), NodeFlags::empty());
    let video = b.add_file(root, "video.mp4", Sizes::new(1000, 990), NodeFlags::empty());
    let empty = b.add_dir(root, "empty");
    let _ = notes;
    (
        b.finish(SizeMode::Allocated),
        [docs, a, bb, old, c, video, empty, root],
    )
}

fn names(tree: &Tree, id: NodeId) -> Vec<&str> {
    tree.children(id).map(|c| tree.name(c)).collect()
}

#[test]
fn sizes_propagate_to_every_ancestor_while_building() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let d1 = b.add_dir(root, "d1");
    let d2 = b.add_dir(d1, "d2");
    b.add_file(d2, "f", Sizes::new(4096, 10), NodeFlags::empty());
    // Partial totals are visible before finish().
    assert_eq!(b.tree().sizes(d2), Sizes::new(4096, 10));
    assert_eq!(b.tree().sizes(d1), Sizes::new(4096, 10));
    assert_eq!(b.tree().total(), Sizes::new(4096, 10));
    b.add_file(d1, "g", Sizes::new(8192, 8000), NodeFlags::empty());
    assert_eq!(b.tree().sizes(d1), Sizes::new(12288, 8010));
    assert_eq!(b.tree().sizes(d2), Sizes::new(4096, 10));
    assert_eq!(b.tree().file_count(), 2);
    assert_eq!(b.tree().dir_count(), 2);
}

#[test]
fn finish_sorts_children_largest_first() {
    let (tree, [docs, ..]) = sample();
    assert_eq!(
        names(&tree, tree.root()),
        vec!["video.mp4", "docs", "empty"]
    );
    assert_eq!(
        names(&tree, docs),
        vec!["old", "a.pdf", "b.PDF", "notes.txt"]
    );
    assert_eq!(tree.total(), Sizes::new(1355, 1271));
    assert_eq!(tree.file_count(), 5);
    assert_eq!(tree.dir_count(), 3);
}

#[test]
fn switching_size_mode_resorts() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    // Sparse file: big apparent, small allocated; and the opposite.
    b.add_file(
        root,
        "sparse.img",
        Sizes::new(4096, 1_000_000),
        NodeFlags::empty(),
    );
    b.add_file(
        root,
        "dense.bin",
        Sizes::new(50_000, 49_000),
        NodeFlags::empty(),
    );
    let mut tree = b.finish(SizeMode::Allocated);
    assert_eq!(names(&tree, tree.root()), vec!["dense.bin", "sparse.img"]);
    tree.set_size_mode(SizeMode::Apparent);
    assert_eq!(tree.size_mode(), SizeMode::Apparent);
    assert_eq!(names(&tree, tree.root()), vec!["sparse.img", "dense.bin"]);
    assert_eq!(tree.size(tree.root()), 1_049_000);
}

#[test]
fn paths_are_rebuilt_from_the_root() {
    let (tree, [docs, _, _, old, c, ..]) = sample();
    assert_eq!(tree.path(tree.root()), PathBuf::from("/scan"));
    assert_eq!(tree.path(c), PathBuf::from("/scan/docs/old/c.pdf"));
    assert_eq!(tree.relative_path(c), "docs/old/c.pdf");
    assert_eq!(tree.relative_path(tree.root()), "");
    assert_eq!(tree.depth(old), 2);
    assert_eq!(
        tree.ancestors(c).collect::<Vec<_>>(),
        vec![old, docs, tree.root()]
    );
}

#[test]
fn ext_table_totals_by_lower_cased_extension() {
    let (tree, _) = sample();
    let table = ExtTable::from_tree(&tree);
    let names: Vec<&str> = table.rows().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["mp4", "pdf", "txt"]);
    let pdf = table.get("pdf").expect("pdf row");
    assert_eq!(pdf.count, 3);
    assert_eq!(pdf.sizes, Sizes::new(350, 280));
    assert_eq!(table.top(2).len(), 2);
    assert_eq!(table.top(20).len(), 3);
}

#[test]
fn ext_table_of_a_subtree_counts_only_its_files() {
    let (mut tree, [docs, _, _, old, _, video, empty, root]) = sample();
    let table = ExtTable::from_subtree(&tree, docs);
    let names: Vec<&str> = table.rows().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["pdf", "txt"]);
    assert_eq!(table.get("pdf").map(|r| r.count), Some(3));
    let old_table = ExtTable::from_subtree(&tree, old);
    assert_eq!(old_table.rows().len(), 1);
    assert_eq!(old_table.rows()[0].sizes, Sizes::new(200, 150));
    let pdf = old_table.rows()[0].ext;
    assert_eq!(old_table.by_ext(pdf).map(|r| r.count), Some(1));
    // The root's table is the whole tree's; an empty folder or a file root has nothing
    // below it except the file itself, and an unknown id has nothing.
    assert_eq!(
        ExtTable::from_subtree(&tree, root).rows(),
        ExtTable::from_tree(&tree).rows()
    );
    assert!(ExtTable::from_subtree(&tree, empty).rows().is_empty());
    assert_eq!(ExtTable::from_subtree(&tree, video).rows().len(), 1);
    assert!(ExtTable::from_subtree(&tree, NodeId(9999))
        .rows()
        .is_empty());
    // A removed folder has no table.
    tree.remove_subtree(old).expect("removable");
    assert!(ExtTable::from_subtree(&tree, old).rows().is_empty());
    assert_eq!(
        ExtTable::from_subtree(&tree, docs)
            .get("pdf")
            .map(|r| r.count),
        Some(2)
    );
}

#[test]
fn files_without_extension_share_row_zero() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    b.add_file(root, "Makefile", Sizes::new(10, 10), NodeFlags::empty());
    b.add_file(root, ".bashrc", Sizes::new(20, 20), NodeFlags::empty());
    let tree = b.finish(SizeMode::Allocated);
    let table = ExtTable::from_tree(&tree);
    assert_eq!(table.rows().len(), 1);
    assert_eq!(table.rows()[0].ext, NO_EXT);
    assert_eq!(table.rows()[0].count, 2);
}

#[test]
fn remove_subtree_subtracts_from_ancestors() {
    let (mut tree, [docs, _, _, old, c, _, _, root]) = sample();
    let removed = tree.remove_subtree(old).expect("old is removable");
    assert_eq!(removed, Sizes::new(200, 150));
    assert_eq!(tree.sizes(docs), Sizes::new(155, 131));
    assert_eq!(tree.total(), Sizes::new(1155, 1121));
    assert_eq!(tree.file_count(), 4);
    assert_eq!(tree.dir_count(), 2);
    assert!(!names(&tree, docs).contains(&"old"));
    assert!(!tree.is_live(old) && !tree.is_live(c));
    // The extension table no longer counts the removed file.
    let pdf = ExtTable::from_tree(&tree).get("pdf").cloned().expect("pdf");
    assert_eq!(pdf.count, 2);
    // Removing it again, or the root, is refused.
    assert_eq!(tree.remove_subtree(old), Err(RemoveError::Missing));
    assert_eq!(tree.remove_subtree(c), Err(RemoveError::Missing));
    assert_eq!(tree.remove_subtree(root), Err(RemoveError::Root));
    assert_eq!(tree.remove_subtree(NodeId(9999)), Err(RemoveError::Missing));
}

#[test]
fn remove_first_middle_and_last_children() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let ids: Vec<NodeId> = (1..=4u64)
        .map(|i| b.add_file(root, &format!("f{i}"), Sizes::new(i, i), NodeFlags::empty()))
        .collect();
    let mut tree = b.finish(SizeMode::Allocated);
    assert_eq!(names(&tree, root), vec!["f4", "f3", "f2", "f1"]);
    tree.remove_subtree(ids[3]).expect("first child");
    tree.remove_subtree(ids[1]).expect("middle child");
    tree.remove_subtree(ids[0]).expect("last child");
    assert_eq!(names(&tree, root), vec!["f3"]);
    assert_eq!(tree.total(), Sizes::new(3, 3));
}

#[test]
fn largest_files_and_folders() {
    let (tree, [docs, _, _, old, c, video, ..]) = sample();
    assert_eq!(tree.largest_files(2), vec![video, c]);
    assert_eq!(tree.largest_files(0), Vec::<NodeId>::new());
    // docs (355) holds old (200): 56%, not a wrapper.
    assert_eq!(tree.largest_folders(10), vec![docs, old]);
}

#[test]
fn wrapper_folders_are_left_out_of_largest_folders() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let users = b.add_dir(root, "Users");
    let me = b.add_dir(users, "me");
    let lib = b.add_dir(me, "Library");
    let caches = b.add_dir(lib, "Caches");
    b.add_file(caches, "big.db", Sizes::new(600, 600), NodeFlags::empty());
    let mail = b.add_dir(lib, "Mail");
    b.add_file(mail, "box.mbox", Sizes::new(400, 400), NodeFlags::empty());
    b.add_file(me, "small.txt", Sizes::new(10, 10), NodeFlags::empty());
    let tree = b.finish(SizeMode::Allocated);
    // Users (1010) and me (1010) are wrappers around Library (1000).
    assert_eq!(tree.largest_folders(10), vec![lib, caches, mail]);
}

#[test]
fn errors_and_links_are_flags() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let locked = b.add_dir(root, "locked");
    b.mark(locked, NodeFlags::ERROR.union(NodeFlags::DIR));
    let link = b.add_file(
        root,
        "link",
        Sizes::new(0, 12),
        NodeFlags::SYMLINK.union(NodeFlags::DIR),
    );
    let tree = b.finish(SizeMode::Allocated);
    let locked_node = tree.node(locked).expect("locked");
    assert!(locked_node.is_dir() && locked_node.flags().contains(NodeFlags::ERROR));
    let link_node = tree.node(link).expect("link");
    assert!(!link_node.is_dir(), "a file never becomes a directory");
    assert!(link_node.flags().contains(NodeFlags::SYMLINK));
}

#[test]
fn unknown_ids_read_as_empty() {
    let (tree, _) = sample();
    let bogus = NodeId(12345);
    assert_eq!(tree.name(bogus), "");
    assert_eq!(tree.size(bogus), 0);
    assert_eq!(tree.children(bogus).count(), 0);
    assert_eq!(tree.children(NodeId::NONE).count(), 0);
}

#[test]
fn many_extensions_saturate_to_none() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    for i in 0..(u16::MAX as u32 + 10) {
        b.add_file(
            root,
            &format!("f.e{i}"),
            Sizes::new(1, 1),
            NodeFlags::empty(),
        );
    }
    let tree = b.finish(SizeMode::Allocated);
    assert_eq!(tree.ext_count(), u16::MAX as usize + 1);
    assert_eq!(tree.file_count(), u16::MAX as u64 + 10);
}

#[test]
fn a_lossy_name_makes_its_path_and_everything_below_inexact() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let fine = b.add_dir(root, "fine");
    let ok = b.add_file(fine, "ok.txt", Sizes::new(1, 1), NodeFlags::empty());
    let bad = b.add_dir(root, "bad\u{fffd}");
    b.mark(bad, NodeFlags::LOSSY_NAME);
    let below = b.add_file(bad, "inside.txt", Sizes::new(1, 1), NodeFlags::empty());
    let lossy_file = b.add_file(fine, "x\u{fffd}", Sizes::new(1, 1), NodeFlags::LOSSY_NAME);
    let tree = b.finish(SizeMode::Allocated);
    assert_eq!(tree.exact_path(ok), Some(PathBuf::from("/r/fine/ok.txt")));
    assert_eq!(tree.exact_path(root), Some(PathBuf::from("/r")));
    assert_eq!(tree.exact_path(bad), None);
    assert_eq!(
        tree.exact_path(below),
        None,
        "a lossy folder taints its contents"
    );
    assert_eq!(tree.exact_path(lossy_file), None);
    assert!(tree.has_lossy_path(below));
    assert!(!tree.has_lossy_path(fine));
}

#[test]
fn extension_totals_saturate_instead_of_overflowing() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let huge = Sizes::new(u64::MAX / 2 + 10, u64::MAX / 2 + 10);
    b.add_file(root, "a.iso", huge, NodeFlags::empty());
    b.add_file(root, "b.iso", huge, NodeFlags::empty());
    let tree = b.finish(SizeMode::Allocated);
    for table in [
        ExtTable::from_tree(&tree),
        ExtTable::from_subtree(&tree, root),
    ] {
        let row = table.get("iso").expect("iso");
        assert_eq!(row.sizes, Sizes::new(u64::MAX, u64::MAX));
        assert_eq!(row.count, 2);
    }
}

#[test]
fn a_full_name_slab_refuses_entries_instead_of_wrapping() {
    // 4 GiB of names would wrap a u32 offset; a small limit stands in for it.
    let mut b = TreeBuilder::with_name_limit("/r", 12);
    let root = b.root();
    let a = b.add_file(root, "abcd", Sizes::new(5, 5), NodeFlags::empty());
    assert!(!a.is_none() && !b.is_full());
    let d = b.add_dir(root, "efghijk");
    assert!(d.is_none(), "the name would end past the limit");
    assert!(b.is_full());
    let after = b.add_file(root, "x", Sizes::new(7, 7), NodeFlags::empty());
    assert!(after.is_none(), "nothing is added once full");
    let tree = b.finish(SizeMode::Allocated);
    assert_eq!(tree.arena_len(), 2);
    assert_eq!(tree.total(), Sizes::new(5, 5));
    assert_eq!(tree.name(a), "abcd");
}
