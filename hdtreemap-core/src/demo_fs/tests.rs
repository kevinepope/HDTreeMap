//! Tests for the demo scanner: determinism, totals against the advertised used space,
//! time-slicing equivalence, pacing, entry-count estimates and believable trees.

use std::collections::HashSet;

use super::*;
use crate::model::{ExtTable, NodeId, Tree};

/// Every node as (depth, name, sizes, flags) in depth-first child order: equal iff the
/// trees are identical.
fn fingerprint(tree: &Tree) -> Vec<(usize, String, u64, u64, bool)> {
    let mut out = Vec::new();
    let mut stack = vec![(tree.root(), 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let node = tree.node(id).expect("node");
        let s = node.sizes();
        out.push((
            depth,
            tree.name(id).to_string(),
            s.allocated,
            s.apparent,
            node.is_dir(),
        ));
        let kids: Vec<NodeId> = tree.children(id).collect();
        for k in kids.into_iter().rev() {
            stack.push((k, depth + 1));
        }
    }
    out
}

fn generate(profile: DemoProfile) -> ScanResult {
    DemoScanner::new(profile, DemoPace::Unpaced)
        .run_to_end()
        .expect("result")
}

#[test]
fn every_drive_id_has_a_profile() {
    for volume in DemoPlatform::default().list_volumes() {
        let profile = DemoProfile::from_id(&volume.id).expect("profile for every drive");
        assert_eq!(profile.used_bytes(), volume.used_bytes());
        assert_eq!(profile.mount_path(), volume.mount_path);
    }
    assert_eq!(DemoProfile::from_id("nope"), None);
}

#[test]
fn same_seed_same_tree() {
    for profile in [
        DemoProfile::UsbStick,
        DemoProfile::MessyDownloads,
        DemoProfile::LaptopSsd,
    ] {
        let a = generate(profile);
        let b = generate(profile);
        assert_eq!(fingerprint(&a.tree), fingerprint(&b.tree), "{profile:?}");
        assert_eq!(a.errors, b.errors);
    }
}

#[test]
fn time_slicing_produces_the_same_tree_as_one_shot() {
    for profile in [
        DemoProfile::UsbStick,
        DemoProfile::MessyDownloads,
        DemoProfile::MediaLibrary,
    ] {
        let one_shot = generate(profile);
        let mut sliced = DemoScanner::new(profile, DemoPace::Unpaced);
        let mut slices = 0;
        while !sliced.step(7 + slices % 13) {
            slices += 1;
            // Partial totals are readable mid-scan.
            assert!(sliced.partial_tree().is_some());
        }
        let sliced = sliced.take_result().expect("result");
        assert!(slices > 10);
        assert_eq!(
            fingerprint(&sliced.tree),
            fingerprint(&one_shot.tree),
            "{profile:?}"
        );
    }
}

#[test]
fn totals_match_the_advertised_used_space_and_estimates_hold() {
    for profile in DemoProfile::ALL {
        let result = generate(profile);
        let tree = &result.tree;
        let used = profile.used_bytes() as f64;
        let allocated = tree.total().allocated as f64;
        assert!(
            (allocated - used).abs() / used < 0.02,
            "{profile:?}: {allocated} vs {used}"
        );
        assert!(tree.total().apparent <= tree.total().allocated);
        let entries = (tree.file_count() + tree.dir_count()) as f64;
        let estimate = profile.approx_entries() as f64;
        assert!(
            (entries - estimate).abs() / estimate < 0.25,
            "{profile:?}: {entries} entries, estimate {estimate}"
        );
    }
}

#[test]
fn names_are_unique_within_each_folder() {
    for profile in [
        DemoProfile::UsbStick,
        DemoProfile::MessyDownloads,
        DemoProfile::DevWorkstation,
    ] {
        let result = generate(profile);
        let tree = &result.tree;
        let mut stack = vec![tree.root()];
        while let Some(id) = stack.pop() {
            let mut seen = HashSet::new();
            for child in tree.children(id) {
                assert!(
                    seen.insert(tree.name(child)),
                    "{profile:?}: {:?} twice",
                    tree.path(child)
                );
                assert!(!tree.name(child).is_empty());
                stack.push(child);
            }
        }
    }
}

#[test]
fn drives_look_like_what_they_claim() {
    let laptop = generate(DemoProfile::LaptopSsd);
    let biggest = laptop.tree.largest_files(1)[0];
    assert!(
        laptop.tree.size(biggest) > 30 * recipes::GB,
        "a giant VM image"
    );
    assert!(
        laptop.errors.total() >= 2,
        "Mail and friends are unreadable"
    );

    let dev = generate(DemoProfile::DevWorkstation);
    let exts = ExtTable::from_tree(&dev.tree);
    for ext in ["js", "json", "map", "ts", "rlib", "rmeta", "o", "pack"] {
        assert!(
            exts.get(ext).is_some_and(|r| r.count > 0),
            "dev drive has .{ext}"
        );
    }
    assert!(exts.get("js").is_some_and(|r| r.count > 10_000));

    let media = generate(DemoProfile::MediaLibrary);
    let exts = ExtTable::from_tree(&media.tree);
    for ext in ["jpg", "heic", "mov", "mp4"] {
        assert!(exts.get(ext).is_some(), "media drive has .{ext}");
    }

    let usb = generate(DemoProfile::UsbStick);
    assert!(usb.tree.file_count() < 1000);

    let downloads = generate(DemoProfile::MessyDownloads);
    let names: Vec<String> = downloads
        .tree
        .children(downloads.tree.root())
        .map(|c| downloads.tree.name(c).to_string())
        .collect();
    assert!(
        names.iter().any(|n| n.contains(" (1).")),
        "browser-numbered copies"
    );
    let exts = ExtTable::from_tree(&downloads.tree);
    for ext in [
        "dmg", "exe", "msi", "pkg", "zip", "7z", "gz", "pdf", "torrent",
    ] {
        assert!(exts.get(ext).is_some(), "downloads have .{ext}");
    }
}

#[test]
fn the_archive_server_is_huge_but_quick() {
    let started = Instant::now();
    let result = generate(DemoProfile::ArchiveServer);
    let elapsed = started.elapsed();
    let files = result.tree.file_count();
    assert!((1_000_000..=2_000_000).contains(&files), "{files} files");
    // Generous for debug builds on slow CI machines; release is far quicker.
    assert!(elapsed < Duration::from_secs(60), "{elapsed:?}");
}

#[test]
fn a_paced_scan_spreads_out_and_an_unpaced_one_finishes_in_one_poll() {
    let mut paced = DemoScanner::new(DemoProfile::UsbStick, DemoPace::Realistic);
    // Immediately after starting, almost nothing is due.
    assert!(matches!(
        paced.poll(Duration::from_secs(1)),
        ScanPoll::Running
    ));
    assert!(paced.progress().snapshot().files < 100);

    let mut unpaced = DemoScanner::new(DemoProfile::UsbStick, DemoPace::Unpaced);
    let ScanPoll::Finished(result) = unpaced.poll(Duration::from_secs(60)) else {
        panic!("an unpaced USB stick finishes within a minute-long budget");
    };
    assert!(result.tree.file_count() > 100);
    assert!(unpaced.progress().is_done());
    assert!(matches!(
        unpaced.poll(Duration::from_secs(1)),
        ScanPoll::Running
    ));
}

#[test]
fn a_cancelled_demo_scan_stops() {
    let mut scanner = DemoScanner::new(DemoProfile::UsbStick, DemoPace::Unpaced);
    scanner.cancel();
    assert!(matches!(
        scanner.poll(Duration::from_secs(1)),
        ScanPoll::Running
    ));
    assert_eq!(scanner.progress().snapshot().files, 0);
}
