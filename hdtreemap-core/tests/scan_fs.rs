//! Tests for the native filesystem scanner (`hdtreemap_core::scan::FsScanner` and
//! `scan_blocking`) over real temporary directory trees: sizes, counts, the extension table,
//! hidden files, symlinks (never followed), hard links (counted once), unreadable folders
//! (counted as errors), a missing root, and cancelling.

#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use hdtreemap_core::model::{ExtTable, NodeFlags, NodeId, SizeMode, Tree};
use hdtreemap_core::scan::{
    scan_blocking, FsScanner, ScanPoll, ScanProgress, ScanResult, ScanSource, BATCH_RECORDS,
};

fn write(path: &Path, len: usize) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, vec![b'x'; len]).expect("write file");
}

/// `path` canonicalised the way the scanner names its root: on Windows without the `\\?\`
/// prefix `fs::canonicalize` adds to drive paths.
fn canonical(path: impl AsRef<Path>) -> std::io::Result<std::path::PathBuf> {
    let c = fs::canonicalize(path)?;
    let plain = c
        .to_str()
        .and_then(|s| s.strip_prefix(r"\\?\"))
        .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
        .map(std::path::PathBuf::from);
    Ok(plain.unwrap_or(c))
}

fn scan(root: &Path) -> (ScanResult, Arc<ScanProgress>) {
    let progress = Arc::new(ScanProgress::new());
    let result = scan_blocking(root, &progress, None);
    (result, progress)
}

/// The node at `relative` (`/`-separated) below the root.
fn find(tree: &Tree, relative: &str) -> Option<NodeId> {
    let mut node = tree.root();
    for part in relative.split('/') {
        node = tree.children(node).find(|&c| tree.name(c) == part)?;
    }
    Some(node)
}

/// docs/report.pdf 1000, docs/deep/data.bin 5000, notes.TXT 300, .hidden 20, empty/
fn sample_tree(root: &Path) {
    write(&root.join("docs/report.pdf"), 1000);
    write(&root.join("docs/deep/data.bin"), 5000);
    write(&root.join("notes.TXT"), 300);
    write(&root.join(".hidden"), 20);
    fs::create_dir_all(root.join("empty")).expect("empty dir");
}

#[test]
fn sizes_counts_and_extensions_match_the_disk() {
    let dir = tempfile::tempdir().expect("temp dir");
    sample_tree(dir.path());
    let (result, progress) = scan(dir.path());
    let tree = &result.tree;

    assert_eq!(tree.total().apparent, 6320);
    assert!(tree.total().allocated >= 6320 || cfg!(windows));
    assert_eq!(tree.file_count(), 4, "hidden files are counted");
    assert_eq!(tree.dir_count(), 3);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    // The root is canonical (on macOS the temp dir under /var is really under /private/var).
    let root = canonical(dir.path()).expect("canonical");
    assert_eq!(result.root, root);
    assert_eq!(tree.path(tree.root()), root);

    let docs = find(tree, "docs").expect("docs");
    assert_eq!(tree.sizes(docs).apparent, 6000);
    let data = find(tree, "docs/deep/data.bin").expect("data.bin");
    assert_eq!(
        tree.path(data),
        root.join("docs").join("deep").join("data.bin")
    );
    // Sorted by allocated size: docs first.
    assert_eq!(tree.children(tree.root()).next(), Some(docs));

    let table = ExtTable::from_tree(tree);
    assert_eq!(table.get("bin").map(|r| r.sizes.apparent), Some(5000));
    assert_eq!(table.get("pdf").map(|r| r.count), Some(1));
    assert_eq!(table.get("txt").map(|r| r.sizes.apparent), Some(300));
    assert_eq!(
        table.get("").map(|r| r.count),
        Some(1),
        ".hidden has no extension"
    );

    let snap = progress.snapshot();
    assert!(snap.done);
    assert_eq!((snap.files, snap.dirs, snap.errors), (4, 3, 0));
    assert_eq!(snap.bytes, tree.total().allocated);
}

#[test]
fn apparent_mode_resorts_the_scanned_tree() {
    let dir = tempfile::tempdir().expect("temp dir");
    sample_tree(dir.path());
    let (mut result, _) = scan(dir.path());
    result.tree.set_size_mode(SizeMode::Apparent);
    let first = result
        .tree
        .children(result.tree.root())
        .next()
        .expect("child");
    assert_eq!(result.tree.name(first), "docs");
    assert_eq!(result.tree.size(result.tree.root()), 6320);
}

#[test]
fn the_fs_scanner_runs_on_a_worker_and_hands_back_the_tree() {
    let dir = tempfile::tempdir().expect("temp dir");
    sample_tree(dir.path());
    let mut scanner = FsScanner::start(dir.path().to_path_buf(), None);
    let progress = scanner.progress();
    let result = loop {
        match scanner.poll(Duration::from_millis(5)) {
            ScanPoll::Finished(result) => break result,
            ScanPoll::Running => std::thread::sleep(Duration::from_millis(2)),
        }
    };
    assert_eq!(result.tree.file_count(), 4);
    assert!(progress.is_done());
    // A finished source never hands its result over twice.
    assert!(matches!(scanner.poll(Duration::ZERO), ScanPoll::Running));
}

/// Every node's path below the root with its sizes and flags, sorted.
fn listing(tree: &Tree) -> Vec<(String, u64, u64, String)> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root()];
    while let Some(id) = stack.pop() {
        let n = tree.node(id).expect("node");
        let s = n.sizes();
        out.push((
            tree.relative_path(id),
            s.allocated,
            s.apparent,
            format!("{:?}", n.flags()),
        ));
        stack.extend(tree.children(id));
    }
    out.sort();
    out
}

#[test]
fn the_live_scan_builds_the_same_tree_as_a_blocking_scan() {
    let dir = tempfile::tempdir().expect("temp dir");
    // Enough entries for several batches.
    for d in 0..40 {
        for f in 0..(BATCH_RECORDS / 20) {
            write(
                &dir.path().join(format!("d{d:02}/sub/f{f}.dat")),
                (d * 7 + f) % 3000,
            );
        }
    }
    sample_tree(dir.path());
    let mut scanner = FsScanner::start(dir.path().to_path_buf(), None);
    let mut partial_counts = Vec::new();
    let result = loop {
        match scanner.poll(Duration::from_micros(200)) {
            ScanPoll::Finished(result) => break result,
            ScanPoll::Running => {
                if let Some(p) = scanner.partial() {
                    partial_counts.push(p.tree.file_count());
                }
            }
        }
    };
    assert!(scanner.partial().is_none(), "the tree was handed over");
    assert!(
        partial_counts.windows(2).all(|w| w[0] <= w[1]),
        "the partial tree only grows"
    );
    let (blocking, _) = scan(dir.path());
    assert_eq!(result.root, blocking.root);
    assert_eq!(result.tree.total(), blocking.tree.total());
    assert_eq!(result.tree.file_count(), blocking.tree.file_count());
    assert_eq!(result.tree.dir_count(), blocking.tree.dir_count());
    assert_eq!(listing(&result.tree), listing(&blocking.tree));
    assert_eq!(result.errors, blocking.errors);
    // Children are sorted, largest first, as after a blocking scan.
    let root = result.tree.root();
    let kids: Vec<NodeId> = result.tree.children(root).collect();
    assert_eq!(kids, result.tree.children_by_size(root));
}

#[test]
fn a_missing_root_is_an_error_not_a_crash() {
    let dir = tempfile::tempdir().expect("temp dir");
    let missing = dir.path().join("does-not-exist");
    let (result, progress) = scan(&missing);
    assert_eq!(result.errors.total(), 1);
    assert_eq!(result.tree.file_count(), 0);
    assert_eq!(progress.snapshot().errors, 1);
    let root = result.tree.node(result.tree.root()).expect("root");
    assert!(
        root.flags().contains(NodeFlags::ERROR),
        "the root is flagged"
    );
}

#[test]
fn a_file_root_has_correct_paths() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("movie.mkv");
    write(&file, 4321);
    let (result, _) = scan(&file);
    let tree = &result.tree;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(tree.file_count(), 1);
    assert_eq!(tree.total().apparent, 4321);
    let child = tree.children(tree.root()).next().expect("the file");
    assert_eq!(tree.name(child), "movie.mkv");
    // The root is the file's folder and the child the file: no doubled file name.
    let path = tree.path(child);
    assert_eq!(path, tree.path(tree.root()).join("movie.mkv"));
    assert_eq!(
        canonical(&path).expect("the path exists"),
        canonical(&file).expect("canonical")
    );
}

#[test]
fn the_root_is_made_absolute_and_canonical() {
    let dir = tempfile::tempdir().expect("temp dir");
    sample_tree(dir.path());
    // A path with `..` in it names the scan by where it really is.
    let roundabout = dir.path().join("docs").join("..");
    let (result, _) = scan(&roundabout);
    let canonical = canonical(dir.path()).expect("canonical");
    assert_eq!(result.root, canonical);
    assert_eq!(result.tree.path(result.tree.root()), canonical);
    assert_eq!(result.tree.file_count(), 4);
    let data = find(&result.tree, "docs/deep/data.bin").expect("data.bin");
    assert!(result.tree.path(data).is_absolute());
}

#[test]
fn a_cancelled_scan_stops_early() {
    let dir = tempfile::tempdir().expect("temp dir");
    for i in 0..50 {
        write(&dir.path().join(format!("d{i}/f.txt")), 10);
    }
    let progress = Arc::new(ScanProgress::new());
    progress.cancel();
    let result = scan_blocking(dir.path(), &progress, None);
    assert_eq!(
        result.tree.file_count(),
        0,
        "nothing below the root is read"
    );
    assert!(progress.is_done());

    // Dropping a running scanner cancels it.
    let scanner = FsScanner::start(dir.path().to_path_buf(), None);
    let progress = scanner.progress();
    drop(scanner);
    assert!(progress.is_cancelled());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn symlinks_are_listed_but_never_followed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let outside = tempfile::tempdir().expect("outside dir");
        write(&outside.path().join("big.bin"), 100_000);
        write(&dir.path().join("real.txt"), 10);
        symlink(outside.path(), dir.path().join("to-outside")).expect("dir symlink");
        symlink(dir.path().join("real.txt"), dir.path().join("to-real")).expect("file symlink");
        symlink(dir.path(), dir.path().join("loop")).expect("loop symlink");

        let (result, _) = scan(dir.path());
        let tree = &result.tree;
        assert!(
            find(tree, "to-outside/big.bin").is_none(),
            "followed a symlink"
        );
        let link = find(tree, "to-outside").expect("link listed");
        let node = tree.node(link).expect("node");
        assert!(node.flags().contains(NodeFlags::SYMLINK));
        assert!(!node.is_dir());
        assert!(
            tree.sizes(link).apparent < 4096,
            "a link counts its own size"
        );
        assert!(tree.total().apparent < 10_000);
        assert_eq!(tree.file_count(), 4, "real.txt and three links");
    }

    #[test]
    fn a_symlinked_root_is_scanned_where_it_points() {
        let dir = tempfile::tempdir().expect("temp dir");
        sample_tree(&dir.path().join("real"));
        symlink(dir.path().join("real"), dir.path().join("alias")).expect("dir symlink");
        let (result, _) = scan(&dir.path().join("alias"));
        let canonical = canonical(dir.path().join("real")).expect("canonical");
        assert_eq!(result.tree.path(result.tree.root()), canonical);
        assert_eq!(result.tree.file_count(), 4);
    }

    #[test]
    fn a_root_linking_to_a_file_scans_that_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(&dir.path().join("data/real.bin"), 7000);
        symlink(
            dir.path().join("data/real.bin"),
            dir.path().join("link.bin"),
        )
        .expect("link");
        let (result, _) = scan(&dir.path().join("link.bin"));
        let tree = &result.tree;
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(tree.file_count(), 1);
        assert_eq!(
            tree.total().apparent,
            7000,
            "the file's size, not the link's"
        );
        let file = tree.children(tree.root()).next().expect("the file");
        let real = canonical(dir.path().join("data/real.bin")).expect("canonical");
        assert_eq!(tree.path(file), real);
    }

    #[test]
    fn hard_links_are_counted_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(&dir.path().join("a/original.bin"), 50_000);
        fs::create_dir_all(dir.path().join("b")).expect("b");
        fs::hard_link(
            dir.path().join("a/original.bin"),
            dir.path().join("b/link.bin"),
        )
        .expect("hard link");
        let (result, _) = scan(dir.path());
        assert_eq!(result.tree.total().apparent, 50_000);
        assert_eq!(result.tree.file_count(), 1);
    }

    #[test]
    fn unreadable_folders_are_counted_as_errors() {
        if is_root_user() {
            eprintln!("skipped: root can read any folder");
            return;
        }
        let dir = tempfile::tempdir().expect("temp dir");
        write(&dir.path().join("open/ok.txt"), 100);
        write(&dir.path().join("locked/secret.txt"), 100);
        let locked = dir.path().join("locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
        let (result, progress) = scan(dir.path());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod back");

        assert_eq!(result.errors.total(), 1, "{:?}", result.errors);
        let canonical = canonical(dir.path()).expect("canonical").join("locked");
        assert_eq!(result.errors.entries()[0].path, canonical);
        assert_eq!(progress.snapshot().errors, 1);
        let node = find(&result.tree, "locked").expect("locked listed");
        assert!(result
            .tree
            .node(node)
            .expect("node")
            .flags()
            .contains(NodeFlags::ERROR));
        assert_eq!(result.tree.file_count(), 1, "only ok.txt was readable");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_names_are_flagged_lossy() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let odd = dir.path().join(OsStr::from_bytes(b"caf\xe9"));
        fs::create_dir_all(&odd).expect("a non-UTF-8 folder name");
        write(&odd.join("inside.txt"), 10);
        write(&dir.path().join("plain.txt"), 10);
        let (result, _) = scan(dir.path());
        let tree = &result.tree;
        let folder = tree
            .children(tree.root())
            .find(|&c| tree.name(c).starts_with("caf"))
            .expect("listed under a lossy name");
        assert!(tree
            .node(folder)
            .expect("node")
            .flags()
            .contains(NodeFlags::LOSSY_NAME));
        let inside = tree.children(folder).next().expect("inside.txt");
        assert_eq!(tree.exact_path(inside), None);
        let plain = find(tree, "plain.txt").expect("plain");
        assert!(tree.exact_path(plain).is_some());
    }

    fn is_root_user() -> bool {
        // `id -u` without a libc dependency: a root-owned temp file reveals the euid.
        let probe = tempfile::NamedTempFile::new().expect("probe");
        use std::os::unix::fs::MetadataExt;
        probe
            .as_file()
            .metadata()
            .map(|m| m.uid() == 0)
            .unwrap_or(false)
    }
}

/// macOS APFS: `cp -c` clones share their blocks, so a family is counted once (the first
/// member in full, the rest at their private size); apparent sizes count every copy.
/// Skipped where the temp folder cannot hold clones (not APFS).
#[cfg(target_os = "macos")]
#[test]
fn apfs_clones_are_counted_once() {
    use std::process::Command;
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    // 2 MB that does not compress, so the blocks are really there.
    let data: Vec<u8> = (0..2_000_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    fs::write(root.join("original.bin"), &data).expect("write");
    for copy in ["clone1.bin", "sub/clone2.bin"] {
        if let Some(parent) = root.join(copy).parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        let cloned = Command::new("cp")
            .arg("-c")
            .arg(root.join("original.bin"))
            .arg(root.join(copy))
            .status()
            .is_ok_and(|s| s.success());
        if !cloned {
            eprintln!("skipped: {} cannot hold clones (not APFS)", root.display());
            return;
        }
    }
    // A plain copy shares nothing and counts in full.
    fs::write(root.join("copy.bin"), &data).expect("write");
    let (result, _) = scan(root);
    let tree = &result.tree;
    let one = tree.sizes(find(tree, "copy.bin").expect("copy")).allocated;
    assert!(one >= 2_000_000);
    let total = tree.total();
    assert_eq!(
        total.apparent,
        4 * 2_000_000,
        "every copy's apparent size counts"
    );
    assert!(
        total.allocated < 2 * one + 64 * 1024,
        "the clones' blocks count once: {} allocated for 2 families of {one}",
        total.allocated
    );
    assert!(total.allocated >= 2 * one);
    // Exactly one member of the family carries its blocks.
    let members = ["original.bin", "clone1.bin", "sub/clone2.bin"]
        .map(|p| tree.sizes(find(tree, p).expect(p)).allocated);
    assert_eq!(
        members.iter().filter(|&&a| a >= one).count(),
        1,
        "{members:?}"
    );
}

/// A folder of thousands of entries (several bulk reads on macOS) is counted exactly:
/// files, a symlink, a hard link once, a subfolder, and (on APFS) a clone once.
#[test]
fn a_huge_folder_is_counted_like_a_small_one() {
    let dir = tempfile::tempdir().expect("temp dir");
    let big = dir.path().join("big");
    fs::create_dir(&big).expect("mkdir");
    let n = 5000;
    for i in 0..n {
        fs::write(big.join(format!("f{i:05}.dat")), vec![b'x'; i % 700]).expect("write");
    }
    fs::create_dir(big.join("sub")).expect("mkdir");
    write(&big.join("sub/inner.txt"), 10);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("f00001.dat", big.join("link")).expect("symlink");
        fs::hard_link(big.join("f00002.dat"), big.join("hard")).expect("hard link");
    }
    let (result, progress) = scan(dir.path());
    let tree = &result.tree;
    let want_bytes: u64 = (0..n as u64).map(|i| i % 700).sum::<u64>() + 10;
    let links = if cfg!(unix) { 1 } else { 0 };
    assert_eq!(
        tree.file_count(),
        n as u64 + 1 + links,
        "every file, the symlink, no hard link"
    );
    assert_eq!(
        tree.dir_count(),
        2,
        "big and big/sub (the root is not counted)"
    );
    let apparent = tree.total().apparent;
    let link_len = if cfg!(unix) {
        "f00001.dat".len() as u64
    } else {
        0
    };
    assert_eq!(apparent, want_bytes + link_len);
    assert!(
        find(tree, "big/sub/inner.txt").is_some(),
        "its subfolder was entered"
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(progress.snapshot().files, tree.file_count());

    // macOS APFS: clones in a big folder count once too.
    #[cfg(target_os = "macos")]
    {
        let data: Vec<u8> = (0..1_000_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8)
            .collect();
        fs::write(big.join("blob.bin"), &data).expect("write");
        let cloned = std::process::Command::new("cp")
            .arg("-c")
            .arg(big.join("blob.bin"))
            .arg(big.join("blob-clone.bin"))
            .status()
            .is_ok_and(|s| s.success());
        if cloned {
            let (result, _) = scan(dir.path());
            let tree = &result.tree;
            let a = tree
                .sizes(find(tree, "big/blob.bin").expect("blob"))
                .allocated;
            let b = tree
                .sizes(find(tree, "big/blob-clone.bin").expect("clone"))
                .allocated;
            assert!(a.max(b) >= 1_000_000 && a.min(b) < 64 * 1024, "{a} and {b}");
        }
    }
}
