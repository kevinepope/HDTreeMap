//! `DemoPlatform`: the browser demo's simulated machine (see `platform.rs`).
//!
//! It lists a fixed set of descriptive drives, each showing off a different side of the app.
//! A drive's `id` is a stable profile name: the demo file-tree generator (`demo_fs`) uses it
//! to decide what kind of tree to synthesise for that drive, and scanning a drive streams
//! that tree through a `DemoScanner`. Nothing here touches the real machine; actions report
//! that they were simulated.

use std::path::{Path, PathBuf};

use super::{ActionOutcome, ActionResult, PlatformServices, VolumeInfo};
use crate::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use crate::scan::ScanSource;
use crate::state::ScanTarget;

const GB: u64 = 1024 * 1024 * 1024;
const TB: u64 = 1024 * GB;

/// Profile ids of the simulated drives, in display order.
pub mod profile {
    pub const LAPTOP_SSD: &str = "laptop-ssd";
    pub const DEV_WORKSTATION: &str = "dev-workstation";
    pub const MEDIA_LIBRARY: &str = "media-library";
    pub const USB_STICK: &str = "usb-stick";
    pub const MESSY_DOWNLOADS: &str = "messy-downloads";
    pub const ARCHIVE_SERVER: &str = "archive-server";
}

/// `(id, name, description, total, free)` for each simulated drive.
const DRIVES: &[(&str, &str, &str, u64, u64)] = &[
    (
        profile::LAPTOP_SSD,
        "Nearly Full Laptop SSD",
        "Almost out of space. Find what's eating it.",
        512 * GB,
        9 * GB,
    ),
    (
        profile::DEV_WORKSTATION,
        "Developer Workstation",
        "Thousands of node_modules, target/ and .git folders.",
        TB,
        310 * GB,
    ),
    (
        profile::MEDIA_LIBRARY,
        "Photo & Video Library",
        "A few huge video files among many photos.",
        2 * TB,
        640 * GB,
    ),
    (
        profile::USB_STICK,
        "Small USB Stick",
        "A small, simple drive. A quick scan.",
        32 * GB,
        11 * GB,
    ),
    (
        profile::MESSY_DOWNLOADS,
        "Messy Downloads Folder",
        "Duplicates, installers, archives and odd extensions.",
        256 * GB,
        70 * GB,
    ),
    (
        profile::ARCHIVE_SERVER,
        "Huge Archive Server",
        "Millions of files. Stress-tests the treemap.",
        8 * TB,
        TB + TB / 5,
    ),
];

/// What the demo tells the user instead of touching a real file.
const SIMULATED_MESSAGE: &str = "This is a browser demo: nothing on your computer is touched.";

/// The simulated machine behind the web demo.
#[derive(Clone, Copy, Debug)]
pub struct DemoPlatform {
    pace: DemoPace,
}

impl Default for DemoPlatform {
    /// Scans at a believable pace, as the web demo shows them.
    fn default() -> Self {
        Self {
            pace: DemoPace::Realistic,
        }
    }
}

impl DemoPlatform {
    /// A demo machine whose scans run as fast as each frame's time budget allows (tests).
    pub fn unpaced() -> Self {
        Self {
            pace: DemoPace::Unpaced,
        }
    }

    /// The pretend mount path of the drive with profile `id`.
    pub fn mount_path(id: &str) -> PathBuf {
        PathBuf::from(format!("/demo/{id}"))
    }
}

impl PlatformServices for DemoPlatform {
    fn list_volumes(&self) -> Vec<VolumeInfo> {
        DRIVES
            .iter()
            .map(|&(id, name, description, total, free)| VolumeInfo {
                id: id.to_string(),
                name: name.to_string(),
                mount_path: Self::mount_path(id),
                total_bytes: total,
                free_bytes: free,
                description: Some(description.to_string()),
            })
            .collect()
    }

    /// The demo has no real folders to pick; the start screen offers the drives instead.
    fn pick_folder(&self) -> Option<PathBuf> {
        None
    }

    fn reveal(&self, _path: &Path) -> ActionResult {
        Ok(ActionOutcome::Simulated(SIMULATED_MESSAGE.to_string()))
    }

    fn open(&self, _path: &Path) -> ActionResult {
        Ok(ActionOutcome::Simulated(SIMULATED_MESSAGE.to_string()))
    }

    fn trash(&self, _path: &Path) -> ActionResult {
        Ok(ActionOutcome::Simulated(SIMULATED_MESSAGE.to_string()))
    }

    fn is_demo(&self) -> bool {
        true
    }

    /// Streams the chosen drive's simulated tree. A target that is not one of the demo's
    /// drives (none can be picked on the web) gets the small USB stick.
    fn scan_source(&self, target: &ScanTarget) -> Box<dyn ScanSource> {
        let profile = target
            .volume_id
            .as_deref()
            .and_then(DemoProfile::from_id)
            .unwrap_or(DemoProfile::UsbStick);
        Box::new(DemoScanner::new(profile, self.pace))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::format_bytes;

    #[test]
    fn demo_lists_the_six_profiles_in_order() {
        let volumes = DemoPlatform::default().list_volumes();
        let ids: Vec<&str> = volumes.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                profile::LAPTOP_SSD,
                profile::DEV_WORKSTATION,
                profile::MEDIA_LIBRARY,
                profile::USB_STICK,
                profile::MESSY_DOWNLOADS,
                profile::ARCHIVE_SERVER,
            ]
        );
        for v in &volumes {
            assert!(v.free_bytes < v.total_bytes, "{} has free < total", v.name);
            assert!(v.description.as_deref().is_some_and(|d| !d.is_empty()));
            assert_eq!(v.mount_path, DemoPlatform::mount_path(&v.id));
        }
    }

    #[test]
    fn demo_sizes_read_as_specified() {
        let sizes: Vec<(String, String)> = DemoPlatform::default()
            .list_volumes()
            .iter()
            .map(|v| (format_bytes(v.total_bytes), format_bytes(v.free_bytes)))
            .collect();
        let expect = [
            ("512.0 GB", "9.0 GB"),
            ("1.0 TB", "310.0 GB"),
            ("2.0 TB", "640.0 GB"),
            ("32.0 GB", "11.0 GB"),
            ("256.0 GB", "70.0 GB"),
            ("8.0 TB", "1.2 TB"),
        ];
        for ((total, free), (want_total, want_free)) in sizes.iter().zip(expect) {
            assert_eq!(total, want_total);
            assert_eq!(free, want_free);
        }
    }

    #[test]
    fn demo_actions_are_simulated() {
        let p = DemoPlatform::default();
        assert!(p.is_demo());
        assert_eq!(p.pick_folder(), None);
        for result in [
            p.reveal(Path::new("/demo/x")),
            p.open(Path::new("/demo/x")),
            p.trash(Path::new("/demo/x")),
        ] {
            assert!(matches!(result, Ok(ActionOutcome::Simulated(_))));
        }
    }

    #[test]
    fn scanning_a_drive_streams_its_profile() {
        let p = DemoPlatform::unpaced();
        let usb = p
            .list_volumes()
            .into_iter()
            .find(|v| v.id == profile::USB_STICK)
            .expect("usb stick");
        let mut source = p.scan_source(&ScanTarget::volume(&usb));
        let crate::scan::ScanPoll::Finished(result) =
            source.poll(std::time::Duration::from_secs(60))
        else {
            panic!("an unpaced USB stick scan finishes in one long poll");
        };
        assert_eq!(result.root, usb.mount_path);
        let used = usb.used_bytes() as f64;
        assert!((result.tree.total().allocated as f64 - used).abs() / used < 0.02);
    }
}
