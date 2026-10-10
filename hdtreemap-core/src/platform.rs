//! The contract between the app and the operating system.
//!
//! Everything the UI needs from the platform (mounted volumes, a folder picker, reveal /
//! open / trash) goes through [`PlatformServices`], so the core stays free of platform code
//! and the same widgets run natively and in the browser:
//!
//! - `hdtreemap-native` implements it with the real OS (sysinfo, rfd, opener, trash). The
//!   pure volume-list cleanup it applies ([`is_listable_volume`], [`dedupe_volumes`],
//!   [`volume_display_name`]) lives here so it is unit-tested on every OS.
//! - [`DemoPlatform`] (all targets) is the browser demo's simulated machine: a fixed set of
//!   descriptive drives, and actions that only report they are simulated.
//!
//! Scanning is a separate abstraction (`ScanSource`, milestone M1); this trait only covers
//! the services around it.

mod demo;

use std::fmt;
use std::path::{Path, PathBuf};

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

/// Drop repeated listings of one volume and sort for display.
///
/// The same filesystem can be mounted more than once (Linux bind mounts list one device at
/// several paths). Volumes with the same non-empty `device` name and the same total and free
/// space are one volume; the shortest mount path wins. Unnamed volumes are never merged
/// (Windows reports an empty name for unlabelled drives). The result is sorted by mount path.
pub fn dedupe_volumes(mut volumes: Vec<(String, VolumeInfo)>) -> Vec<VolumeInfo> {
    volumes.sort_by(|(_, a), (_, b)| {
        let a_len = a.mount_path.as_os_str().len();
        let b_len = b.mount_path.as_os_str().len();
        a_len
            .cmp(&b_len)
            .then_with(|| a.mount_path.cmp(&b.mount_path))
    });
    let mut kept: Vec<(String, VolumeInfo)> = Vec::new();
    for (device, volume) in volumes {
        let duplicate = kept.iter().any(|(kept_device, kept_volume)| {
            kept_volume.mount_path == volume.mount_path
                || (!device.is_empty()
                    && *kept_device == device
                    && kept_volume.total_bytes == volume.total_bytes
                    && kept_volume.free_bytes == volume.free_bytes)
        });
        if !duplicate {
            kept.push((device, volume));
        }
    }
    let mut result: Vec<VolumeInfo> = kept.into_iter().map(|(_, v)| v).collect();
    result.sort_by(|a, b| a.mount_path.cmp(&b.mount_path));
    result
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

    #[test]
    fn bind_mounts_of_one_device_collapse_to_the_shortest_path() {
        let listed = dedupe_volumes(vec![
            ("/dev/sda1".into(), volume("/var/lib/docker", 500, 100)),
            ("/dev/sda1".into(), volume("/", 500, 100)),
            ("/dev/sdb1".into(), volume("/data", 900, 300)),
        ]);
        let mounts: Vec<_> = listed.iter().map(|v| v.mount_path.clone()).collect();
        assert_eq!(mounts, vec![PathBuf::from("/"), PathBuf::from("/data")]);
    }

    #[test]
    fn unnamed_volumes_are_never_merged() {
        let listed = dedupe_volumes(vec![
            (String::new(), volume("D:\\", 500, 100)),
            (String::new(), volume("C:\\", 500, 100)),
        ]);
        let mounts: Vec<_> = listed.iter().map(|v| v.mount_path.clone()).collect();
        assert_eq!(mounts, vec![PathBuf::from("C:\\"), PathBuf::from("D:\\")]);
    }

    // Unix and Windows paths are kept in separate tests: `Path` ordering between the two
    // styles differs by host OS (on Windows `C:\` is a prefix component).
    #[test]
    fn a_repeated_mount_path_is_listed_once() {
        let listed = dedupe_volumes(vec![
            ("x".into(), volume("/mnt/a", 10, 1)),
            ("y".into(), volume("/mnt/a", 10, 1)),
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
}
