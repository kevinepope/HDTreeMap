//! Linux `/proc/self/mountinfo`, parsed and reasoned about as plain data, so the decisions
//! are unit-tested on every OS from fixture text. Used by the scanner (`scan/fs_meta.rs`:
//! which mount points under a scan root to leave out) and by the native volume list (which
//! mounts are the same filesystem root listed twice).
//!
//! Why not just `st_dev`: btrfs gives every subvolume its own device number, so `/home` (a
//! subvolume mounted on `/`) looks like another filesystem although it is the same disk and
//! its contents are reachable nowhere else, while a bind mount keeps the device number of
//! what it repeats, so it looks like ordinary folders and is counted twice. mountinfo says
//! what each mount really is: the filesystem (`major:minor` plus its source) and which of
//! its folders the mount shows (the mount's root).

use std::path::{Path, PathBuf};

/// Where the kernel lists the calling process's mounts.
pub const MOUNTINFO_PATH: &str = "/proc/self/mountinfo";

/// One line of mountinfo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountEntry {
    /// `st_dev` of the filesystem as the kernel numbers it (the same for every subvolume of
    /// one btrfs filesystem in mountinfo, unlike `stat`).
    pub major_minor: (u32, u32),
    /// The folder of the filesystem this mount shows (`/` for the whole filesystem, `/@home`
    /// for a btrfs subvolume, the bound folder for a bind mount).
    pub root: String,
    /// Where it is mounted.
    pub mount_point: PathBuf,
    pub fs_type: String,
    /// The device or other source (`/dev/nvme0n1p3`, `proc`, `server:/export`).
    pub source: String,
}

impl MountEntry {
    /// Whether `other` is a mount of the same filesystem (whatever folder of it each shows).
    pub fn same_filesystem(&self, other: &MountEntry) -> bool {
        self.major_minor == other.major_minor && self.source == other.source
    }
}

/// Parse mountinfo text. Malformed lines are skipped.
pub fn parse_mountinfo(text: &str) -> Vec<MountEntry> {
    text.lines().filter_map(parse_line).collect()
}

/// `36 35 98:0 /mnt1 /mnt2 rw,noatime master:1 - ext3 /dev/root rw,errors=continue`
fn parse_line(line: &str) -> Option<MountEntry> {
    let (before, after) = line.split_once(" - ")?;
    let mut fields = before.split(' ');
    let _mount_id = fields.next()?;
    let _parent_id = fields.next()?;
    let (major, minor) = fields.next()?.split_once(':')?;
    let root = unescape(fields.next()?);
    let mount_point = PathBuf::from(unescape(fields.next()?));
    let mut tail = after.split(' ');
    let fs_type = unescape(tail.next()?);
    let source = unescape(tail.next().unwrap_or(""));
    Some(MountEntry {
        major_minor: (major.parse().ok()?, minor.parse().ok()?),
        root,
        mount_point,
        fs_type,
        source,
    })
}

/// mountinfo writes space, tab, newline and backslash as three-digit octal escapes.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 4 <= bytes.len() {
            let digits = &bytes[i + 1..i + 4];
            if digits.iter().all(|b| (b'0'..=b'7').contains(b)) {
                let value = digits
                    .iter()
                    .fold(0u32, |acc, d| acc * 8 + u32::from(d - b'0'));
                if let Ok(value) = u8::try_from(value) {
                    out.push(value);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The mounts that are visible: when several are stacked on one mount point, the last one
/// listed (mounted last) is what a path lookup sees.
fn visible(entries: &[MountEntry]) -> Vec<&MountEntry> {
    let mut seen: Vec<&MountEntry> = Vec::new();
    for entry in entries.iter().rev() {
        if !seen.iter().any(|e| e.mount_point == entry.mount_point) {
            seen.push(entry);
        }
    }
    seen.reverse();
    seen
}

/// The visible mount `path` is on: the one with the longest mount point above it.
pub fn mount_of<'a>(entries: &'a [MountEntry], path: &Path) -> Option<&'a MountEntry> {
    visible(entries)
        .into_iter()
        .filter(|e| path.starts_with(&e.mount_point))
        .max_by_key(|e| e.mount_point.components().count())
}

/// The mount points below `scan_root` that a scan must list but not enter:
///
/// - mounts of other filesystems (stay on one filesystem);
/// - mounts of the scan root's own filesystem that show a folder the scan reaches anyway
///   (bind mounts, a second mount of the same subvolume), which would be counted twice.
///
/// A mount of the same filesystem showing a folder the scan cannot otherwise reach (a btrfs
/// `/home` subvolume when scanning `/`) is entered: it is the same disk, and its space would
/// be missing from the map without it. Returns `None` when `scan_root` is on no listed
/// mount (no usable mountinfo).
pub fn mounts_to_skip(entries: &[MountEntry], scan_root: &Path) -> Option<Vec<PathBuf>> {
    let root_mount = mount_of(entries, scan_root)?;
    let below = scan_root
        .strip_prefix(&root_mount.mount_point)
        .unwrap_or(Path::new(""));
    // Folders of the root's filesystem the walk reaches, as paths inside the filesystem.
    let mut reached: Vec<PathBuf> = vec![Path::new(&root_mount.root).join(below)];
    let mut nested: Vec<&MountEntry> = visible(entries)
        .into_iter()
        .filter(|e| e.mount_point != scan_root && e.mount_point.starts_with(scan_root))
        .collect();
    // Shallow first: a mount's region is known before the mounts inside it are judged.
    nested.sort_by_key(|e| e.mount_point.components().count());
    let mut skip: Vec<PathBuf> = Vec::new();
    for entry in nested {
        let inside_skipped = skip.iter().any(|s| entry.mount_point.starts_with(s));
        let reachable = reached
            .iter()
            .any(|region| Path::new(&entry.root).starts_with(region));
        if inside_skipped || !entry.same_filesystem(root_mount) || reachable {
            skip.push(entry.mount_point.clone());
        } else {
            reached.push(PathBuf::from(&entry.root));
        }
    }
    Some(skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Fedora-style btrfs install (`/` is subvolume `root`, `/home` subvolume `home`) with
    /// an ext4 `/boot`, kernel filesystems, a bind mount of a folder inside `/`, a second
    /// disk whose mount point has a space, and a bind mount of `/home/me/shared` elsewhere.
    const FEDORA: &str = "\
22 1 0:31 /root / rw,relatime shared:1 - btrfs /dev/nvme0n1p3 rw,compress=zstd:1,subvol=/root
23 22 0:21 / /proc rw,nosuid,nodev,noexec,relatime shared:12 - proc proc rw
24 22 0:22 / /sys rw,nosuid,nodev,noexec,relatime shared:2 - sysfs sysfs rw
64 22 0:31 /home /home rw,relatime shared:31 - btrfs /dev/nvme0n1p3 rw,subvol=/home
70 22 259:2 / /boot rw,relatime shared:33 - ext4 /dev/nvme0n1p2 rw
71 22 0:31 /root/srv/data /mnt/bind rw,relatime shared:1 - btrfs /dev/nvme0n1p3 rw
72 22 8:17 / /mnt/my\\040disk rw,relatime shared:40 - ext4 /dev/sdb1 rw
73 22 0:31 /home/me/shared /srv/shared rw,relatime shared:31 - btrfs /dev/nvme0n1p3 rw
";

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn lines_parse_with_escapes() {
        let entries = parse_mountinfo(FEDORA);
        assert_eq!(entries.len(), 8);
        assert_eq!(
            entries[0],
            MountEntry {
                major_minor: (0, 31),
                root: "/root".into(),
                mount_point: "/".into(),
                fs_type: "btrfs".into(),
                source: "/dev/nvme0n1p3".into(),
            }
        );
        assert_eq!(entries[6].mount_point, PathBuf::from("/mnt/my disk"));
        assert_eq!(unescape(r"a\134b\011c"), "a\\b\tc");
        assert_eq!(unescape(r"trailing\04"), r"trailing\04");
        assert!(parse_mountinfo("garbage\n\n1 2 x / / rw - a b").is_empty());
    }

    #[test]
    fn scanning_btrfs_root_enters_the_home_subvolume() {
        let entries = parse_mountinfo(FEDORA);
        let skip = mounts_to_skip(&entries, Path::new("/")).expect("plan");
        assert!(!skip.contains(&PathBuf::from("/home")), "{skip:?}");
        for other in ["/proc", "/sys", "/boot", "/mnt/my disk"] {
            assert!(skip.contains(&PathBuf::from(other)), "{other}: {skip:?}");
        }
        // Bind mounts repeat folders the scan reaches anyway.
        assert!(skip.contains(&PathBuf::from("/mnt/bind")), "{skip:?}");
        assert!(skip.contains(&PathBuf::from("/srv/shared")), "{skip:?}");
    }

    #[test]
    fn scanning_inside_a_mount_only_judges_mounts_below_it() {
        let entries = parse_mountinfo(FEDORA);
        assert_eq!(
            mounts_to_skip(&entries, Path::new("/home/me")),
            Some(Vec::new())
        );
        // Scanning /mnt alone, the bind mount's /root/srv/data is reachable nowhere else.
        assert_eq!(
            mounts_to_skip(&entries, Path::new("/mnt")),
            Some(paths(&["/mnt/my disk"]))
        );
        assert_eq!(
            mount_of(&entries, Path::new("/home/me/x")).map(|e| e.root.as_str()),
            Some("/home")
        );
    }

    #[test]
    fn a_bind_mount_of_another_subvolume_is_entered_once() {
        // /srv/shared shows /home/me/shared: scanning /srv alone must enter it (nothing else
        // under /srv reaches it).
        let entries = parse_mountinfo(FEDORA);
        assert_eq!(
            mounts_to_skip(&entries, Path::new("/srv")),
            Some(Vec::new())
        );
    }

    #[test]
    fn an_ext4_bind_mount_of_the_root_is_skipped() {
        let text = "\
25 1 8:1 / / rw - ext4 /dev/sda1 rw
30 25 8:1 / /var/lib/docker rw - ext4 /dev/sda1 rw
31 25 8:1 /var/www /srv/www rw - ext4 /dev/sda1 rw
";
        let entries = parse_mountinfo(text);
        assert_eq!(
            mounts_to_skip(&entries, Path::new("/")),
            Some(paths(&["/srv/www", "/var/lib/docker"]))
        );
    }

    #[test]
    fn the_last_of_stacked_mounts_wins() {
        let text = "\
25 1 8:1 / / rw - ext4 /dev/sda1 rw
30 25 8:2 / /data rw - ext4 /dev/sdb1 rw
31 30 8:1 / /data rw - ext4 /dev/sda1 rw
";
        let entries = parse_mountinfo(text);
        assert_eq!(
            mount_of(&entries, Path::new("/data/x")).map(|e| e.major_minor),
            Some((8, 1))
        );
        assert_eq!(
            mounts_to_skip(&entries, Path::new("/")),
            Some(paths(&["/data"])),
            "a second mount of the root is a duplicate"
        );
    }

    #[test]
    fn no_mounts_means_no_plan() {
        assert_eq!(mounts_to_skip(&[], Path::new("/")), None);
    }
}
