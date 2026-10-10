//! The contract between the app and the operating system.
//!
//! Everything the UI needs from the platform (mounted volumes, a folder picker, reveal /
//! open / trash) goes through [`PlatformServices`], so the core stays free of platform code
//! and the same widgets run natively and in the browser:
//!
//! - `hdtreemap-native` implements it with the real OS (sysinfo, rfd, opener, trash). The
//!   pure volume-list cleanup it applies ([`is_listable_volume`], [`dedupe_volumes`],
//!   [`volume_display_name`]) and the Linux mount reasoning ([`mountinfo`]) live here so
//!   they are unit-tested on every OS.
//! - [`DemoPlatform`] (all targets) is the browser demo's simulated machine: a fixed set of
//!   descriptive drives, and actions that only report they are simulated.
//!
//! Scanning itself is a separate abstraction (`scan::ScanSource`); the platform only picks
//! the source for a target ([`PlatformServices::scan_source`]): the real disk natively, the
//! simulated drive's generator on the demo.

mod demo;
pub mod mountinfo;

use std::fmt;
use std::path::{Path, PathBuf};

use crate::scan::ScanSource;
use crate::state::ScanTarget;

pub use demo::{profile, DemoPlatform};

/// One mounted volume as listed on the start screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeInfo {
    /// Stable identifier. Native: the mount path. Demo: the simulated drive's profile id
    /// (which the demo file-tree generator uses to pick what to generate).
    pub id: String,
    /// Name shown on the card (volume label, or a readable stand-in).
    pub name: String,
    /// Where the volume is mounted; scanning a volume scans this path.
    pub mount_path: PathBuf,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// One-line description shown under the name. `None` for real volumes.
    pub description: Option<String>,
}

impl VolumeInfo {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.free_bytes)
    }
}

/// What a platform action did when it did not fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionOutcome {
    /// The OS carried the action out.
    Done,
    /// Nothing happened on a real machine: the demo platform simulates the action. The text
    /// is what to tell the user.
    Simulated(String),
}

/// A platform action that failed, with a message for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformError {
    pub message: String,
}

impl PlatformError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PlatformError {}

pub type ActionResult = Result<ActionOutcome, PlatformError>;

/// OS services the app uses. Implemented by each shell; see the module docs.
pub trait PlatformServices {
    /// Mounted volumes worth offering on the start screen, in display order.
    fn list_volumes(&self) -> Vec<VolumeInfo>;
    /// Ask the user for a folder to scan. `None` when cancelled or unsupported.
    fn pick_folder(&self) -> Option<PathBuf>;
    /// Show `path` selected in the system file manager.
    fn reveal(&self, path: &Path) -> ActionResult;
    /// Open `path` with its default application.
    fn open(&self, path: &Path) -> ActionResult;
    /// Move `path` to the system trash (never a permanent delete).
    fn trash(&self, path: &Path) -> ActionResult;
    /// True for the browser demo's simulated machine.
    fn is_demo(&self) -> bool;
    /// Start scanning `target`, returning the running source the UI polls each frame.
    fn scan_source(&self, target: &ScanTarget) -> Box<dyn ScanSource>;
}

/// Filesystem types that are never real storage: kernel interfaces, RAM-backed and
/// container overlay filesystems, read-only snap images (Linux) and macOS's devfs.
const PSEUDO_FILESYSTEMS: &[&str] = &[
    "autofs",
    "binfmt_misc",
    "bpf",
    "cgroup",
    "cgroup2",
    "configfs",
    "debugfs",
    "devfs",
    "devpts",
    "devtmpfs",
    "efivarfs",
    "fusectl",
    "hugetlbfs",
    "mqueue",
    "nsfs",
    "overlay",
    "proc",
    "pstore",
    "ramfs",
    "rpc_pipefs",
    "securityfs",
    "selinuxfs",
    "squashfs",
    "sysfs",
    "tmpfs",
    "tracefs",
];

/// Mount-path prefixes that hold system plumbing rather than user data: Linux's kernel and
/// runtime mounts and snap images (except [`USER_MEDIA_PREFIX`]), and macOS's APFS system volumes (`/System/Volumes/Data`
/// is the same data as `/` through firmlinks; VM, Preboot, Update and friends are internal).
const SYSTEM_MOUNT_PREFIXES: &[&str] = &[
    "/proc",
    "/sys",
    "/dev",
    "/run",
    "/snap",
    "/boot/efi",
    "/System/Volumes",
    "/private/var/vm",
];

/// Where udisks mounts removable drives on Fedora, Arch and others: inside `/run`, but user
/// storage.
const USER_MEDIA_PREFIX: &str = "/run/media/";

/// Whether a mounted filesystem is real storage worth offering. Applied to every OS: the
/// Linux and macOS rules cannot match a Windows drive (`C:\`).
pub fn is_listable_volume(fs_type: &str, mount_path: &Path, total_bytes: u64) -> bool {
    if total_bytes == 0 {
        return false;
    }
    if PSEUDO_FILESYSTEMS
        .iter()
        .any(|pseudo| fs_type.eq_ignore_ascii_case(pseudo))
    {
        return false;
    }
    let mount = mount_path.to_string_lossy();
    if mount.starts_with(USER_MEDIA_PREFIX) {
        return true;
    }
    !SYSTEM_MOUNT_PREFIXES.iter().any(|prefix| {
        mount == *prefix
            || mount
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// What identifies the storage behind a listed volume, for [`dedupe_volumes`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VolumeKey {
    /// The device name (`/dev/sda1`); empty when the OS gives none (unlabelled Windows
    /// drives), and then the volume is never merged with another.
    pub device: String,
    /// The folder of the filesystem the mount shows, from Linux's mountinfo (`/` for the
    /// whole filesystem, `/@home` for a btrfs subvolume); `None` elsewhere.
    pub fs_root: Option<String>,
}

/// Drop repeated listings of one volume and sort for display.
///
/// The same filesystem can be mounted more than once (Linux bind mounts list one device at
/// several paths). Two listings are one volume when they share a mount path, or the same
/// non-empty device and:
///
/// - with mount roots known (Linux): the same root, so a bind mount of a filesystem is
///   dropped but btrfs subvolumes of one device (`/` and `/home`) are both listed;
/// - otherwise: the same total and free space, unless one mount path is inside the other
///   (distinct mount points such as `/` and `/home` are kept).
///
/// The shortest mount path wins. The result is sorted by mount path.
pub fn dedupe_volumes(mut volumes: Vec<(VolumeKey, VolumeInfo)>) -> Vec<VolumeInfo> {
    volumes.sort_by(|(_, a), (_, b)| {
        let a_len = a.mount_path.as_os_str().len();
        let b_len = b.mount_path.as_os_str().len();
        a_len
            .cmp(&b_len)
            .then_with(|| a.mount_path.cmp(&b.mount_path))
    });
    let mut kept: Vec<(VolumeKey, VolumeInfo)> = Vec::new();
    for (key, volume) in volumes {
        let duplicate = kept
            .iter()
            .any(|(kept_key, kept_volume)| same_volume(kept_key, kept_volume, &key, &volume));
        if !duplicate {
            kept.push((key, volume));
        }
    }
    let mut result: Vec<VolumeInfo> = kept.into_iter().map(|(_, v)| v).collect();
    result.sort_by(|a, b| a.mount_path.cmp(&b.mount_path));
    result
}

/// The rule [`dedupe_volumes`] documents.
fn same_volume(a_key: &VolumeKey, a: &VolumeInfo, b_key: &VolumeKey, b: &VolumeInfo) -> bool {
    if a.mount_path == b.mount_path {
        return true;
    }
    if a_key.device.is_empty() || a_key.device != b_key.device {
        return false;
    }
    match (&a_key.fs_root, &b_key.fs_root) {
        (Some(a_root), Some(b_root)) => a_root == b_root,
        _ => {
            let nested =
                a.mount_path.starts_with(&b.mount_path) || b.mount_path.starts_with(&a.mount_path);
            !nested && a.total_bytes == b.total_bytes && a.free_bytes == b.free_bytes
        }
    }
}

/// A readable name for a volume: its label when it has one, otherwise (unlabelled drives,
/// Linux device paths such as `/dev/sda1`) the last component of the mount path, or the
/// mount path itself for a root like `/` or `C:\`.
pub fn volume_display_name(label: &str, mount_path: &Path) -> String {
    let label = label.trim();
    if !label.is_empty() && !label.starts_with("/dev/") {
        return label.to_string();
    }
    match mount_path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => mount_path.to_string_lossy().into_owned(),
    }
}

/// The volume holding `path`: the listed volume with the longest mount path that contains
/// it (the status bar's free space for a folder scan).
pub fn volume_containing<'a>(volumes: &'a [VolumeInfo], path: &Path) -> Option<&'a VolumeInfo> {
    volumes
        .iter()
        .filter(|v| path.starts_with(&v.mount_path))
        .max_by_key(|v| v.mount_path.as_os_str().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(mount: &str, total: u64, free: u64) -> VolumeInfo {
        VolumeInfo {
            id: mount.to_string(),
            name: mount.to_string(),
            mount_path: PathBuf::from(mount),
            total_bytes: total,
            free_bytes: free,
            description: None,
        }
    }

    #[test]
    fn real_filesystems_are_listed() {
        assert!(is_listable_volume("ext4", Path::new("/"), 100));
        assert!(is_listable_volume("btrfs", Path::new("/home"), 100));
        assert!(is_listable_volume("apfs", Path::new("/"), 100));
        assert!(is_listable_volume(
            "apfs",
            Path::new("/Volumes/Backup"),
            100
        ));
        assert!(is_listable_volume("NTFS", Path::new("C:\\"), 100));
        // A prefix match must be a whole path component.
        assert!(is_listable_volume("ext4", Path::new("/devdata"), 100));
        assert!(is_listable_volume("ext4", Path::new("/running"), 100));
        // Removable drives mounted by udisks under /run/media stay listed.
        assert!(is_listable_volume(
            "vfat",
            Path::new("/run/media/lars/USB"),
            100
        ));
    }

    #[test]
    fn pseudo_and_system_filesystems_are_hidden() {
        assert!(!is_listable_volume("tmpfs", Path::new("/tmp"), 100));
        assert!(!is_listable_volume("proc", Path::new("/proc"), 100));
        assert!(!is_listable_volume("overlay", Path::new("/var/lib/x"), 100));
        assert!(!is_listable_volume("squashfs", Path::new("/media/x"), 100));
        assert!(!is_listable_volume("ext4", Path::new("/boot/efi"), 100));
        assert!(!is_listable_volume("ext4", Path::new("/run/lock"), 100));
        assert!(!is_listable_volume("ext4", Path::new("/snap/core/1"), 100));
        assert!(!is_listable_volume(
            "apfs",
            Path::new("/System/Volumes/Data"),
            100
        ));
        assert!(!is_listable_volume(
            "apfs",
            Path::new("/System/Volumes/VM"),
            100
        ));
        assert!(!is_listable_volume("devfs", Path::new("/dev"), 100));
    }

    #[test]
    fn empty_volumes_are_hidden() {
        assert!(!is_listable_volume("ext4", Path::new("/mnt/empty"), 0));
    }

    fn key(device: &str, fs_root: Option<&str>) -> VolumeKey {
        VolumeKey {
            device: device.to_string(),
            fs_root: fs_root.map(str::to_string),
        }
    }

    fn mounts(listed: &[VolumeInfo]) -> Vec<PathBuf> {
        listed.iter().map(|v| v.mount_path.clone()).collect()
    }

    #[test]
    fn bind_mounts_of_one_root_collapse_to_the_shortest_path() {
        let listed = dedupe_volumes(vec![
            (
                key("/dev/sda1", Some("/")),
                volume("/var/lib/docker", 500, 100),
            ),
            (key("/dev/sda1", Some("/")), volume("/", 500, 100)),
            (key("/dev/sdb1", Some("/")), volume("/data", 900, 300)),
        ]);
        assert_eq!(
            mounts(&listed),
            vec![PathBuf::from("/"), PathBuf::from("/data")]
        );
    }

    #[test]
    fn btrfs_subvolumes_of_one_device_are_both_listed() {
        // Same device, same space (one filesystem), different subvolumes: / and /home.
        let listed = dedupe_volumes(vec![
            (key("/dev/nvme0n1p3", Some("/root")), volume("/", 900, 300)),
            (
                key("/dev/nvme0n1p3", Some("/home")),
                volume("/home", 900, 300),
            ),
        ]);
        assert_eq!(
            mounts(&listed),
            vec![PathBuf::from("/"), PathBuf::from("/home")]
        );
    }

    #[test]
    fn without_mount_roots_nested_mounts_are_kept_and_others_merged() {
        let listed = dedupe_volumes(vec![
            (key("/dev/sda1", None), volume("/", 500, 100)),
            (key("/dev/sda1", None), volume("/home", 500, 100)),
            (key("/dev/sdb1", None), volume("/mnt/a", 900, 300)),
            (key("/dev/sdb1", None), volume("/media/b", 900, 300)),
        ]);
        assert_eq!(
            mounts(&listed),
            vec![
                PathBuf::from("/"),
                PathBuf::from("/home"),
                PathBuf::from("/mnt/a")
            ]
        );
    }

    #[test]
    fn unnamed_volumes_are_never_merged() {
        let listed = dedupe_volumes(vec![
            (key("", None), volume("D:\\", 500, 100)),
            (key("", None), volume("C:\\", 500, 100)),
        ]);
        let mounts: Vec<_> = listed.iter().map(|v| v.mount_path.clone()).collect();
        assert_eq!(mounts, vec![PathBuf::from("C:\\"), PathBuf::from("D:\\")]);
    }

    // Unix and Windows paths are kept in separate tests: `Path` ordering between the two
    // styles differs by host OS (on Windows `C:\` is a prefix component).
    #[test]
    fn a_repeated_mount_path_is_listed_once() {
        let listed = dedupe_volumes(vec![
            (key("x", None), volume("/mnt/a", 10, 1)),
            (key("y", None), volume("/mnt/a", 10, 1)),
        ]);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].mount_path, PathBuf::from("/mnt/a"));
    }

    #[test]
    fn display_names_prefer_labels() {
        assert_eq!(
            volume_display_name("Macintosh HD", Path::new("/")),
            "Macintosh HD"
        );
        assert_eq!(volume_display_name("/dev/sda1", Path::new("/")), "/");
        assert_eq!(volume_display_name("/dev/sdb1", Path::new("/data")), "data");
        assert_eq!(volume_display_name("  ", Path::new("/media/usb")), "usb");
    }

    #[test]
    fn used_bytes_never_underflows() {
        assert_eq!(volume("/", 10, 4).used_bytes(), 6);
        assert_eq!(volume("/", 10, 40).used_bytes(), 0);
    }

    #[test]
    fn a_folder_belongs_to_its_deepest_volume() {
        let volumes = vec![
            volume("/", 100, 10),
            volume("/Volumes/USB", 50, 20),
            volume("/Volumes/USB2", 70, 30),
        ];
        let find = |p: &str| volume_containing(&volumes, Path::new(p)).map(|v| v.free_bytes);
        assert_eq!(find("/Users/me/Development"), Some(10));
        assert_eq!(find("/Volumes/USB/photos"), Some(20));
        assert_eq!(find("/Volumes/USB2"), Some(30));
        assert_eq!(volume_containing(&[], Path::new("/x")), None);
    }
}
