//! Per-OS file metadata for the filesystem scanner (`scan/fs.rs`): allocated size, the
//! device an entry lives on, the hard-link identity, and the mounts that duplicate other
//! paths. Kept apart from the walk so every `cfg` lives in one place.
//!
//! - Allocated size: Unix `st_blocks * 512`; Windows `GetCompressedFileSizeW` (compressed,
//!   sparse and cloud-placeholder files report what they really occupy), falling back to
//!   the apparent size; elsewhere the apparent size.
//! - Devices and hard links are Unix-only (`st_dev`, `st_ino`, `st_nlink`). On Windows,
//!   junctions and mounted volumes are reparse points that std reports as symlinks, which the
//!   walk never follows, so it stays on one volume without a device check.
//! - Linux: which mount points to skip comes from the mount table ([`mount_plan`]): btrfs
//!   subvolumes have their own `st_dev` yet are the same disk, and bind mounts keep theirs
//!   yet repeat folders already counted.
//! - macOS: scanning `/` would visit the data volume twice, once through the firmlinks
//!   (`/Users`, `/Applications`, ...) and once at `/System/Volumes/Data`, so that path is
//!   skipped and its device counts as the root's.

use std::fs::Metadata;
use std::path::{Path, PathBuf};

use crate::model::Sizes;

/// The path a scan of `root` walks and names its tree after: absolute and canonical, so
/// every node's path can be handed to the file manager as it is (a relative or `-dash`
/// root cannot be mistaken for an option), aliases such as macOS's `/Volumes/Macintosh HD`
/// (a link to `/`) get the root's own rules, and the containing volume is found by prefix.
/// A root that cannot be canonicalised (it does not exist) is made absolute instead.
/// Windows' canonical form has a `\\?\` prefix; it is dropped where a plain path means the
/// same, so names read as the user knows them.
pub(super) fn resolve_root(root: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(root)
        .or_else(|_| std::path::absolute(root))
        .unwrap_or_else(|_| root.to_path_buf());
    match resolved.to_str().and_then(strip_verbatim) {
        Some(plain) => PathBuf::from(plain),
        None => resolved,
    }
}

/// `C:\...` for `\\?\C:\...` and `\\server\share\...` for `\\?\UNC\server\share\...`;
/// `None` for anything else (no prefix, or a verbatim path with no plain spelling).
pub(super) fn strip_verbatim(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    if let Some(unc) = rest.strip_prefix(r"UNC\") {
        return Some(format!(r"\\{unc}"));
    }
    let bytes = rest.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    drive.then(|| rest.to_string())
}

/// Allocated and apparent size of the entry at `path` described by `meta` (its `lstat`).
pub(super) fn sizes(path: &Path, meta: &Metadata) -> Sizes {
    Sizes::new(allocated(path, meta), meta.len())
}

#[cfg(unix)]
fn allocated(_path: &Path, meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks().saturating_mul(512)
}

#[cfg(windows)]
fn allocated(path: &Path, meta: &Metadata) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{GetLastError, NO_ERROR};
    use windows_sys::Win32::Storage::FileSystem::{GetCompressedFileSizeW, INVALID_FILE_SIZE};

    if !meta.is_file() {
        return meta.len();
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut high: u32 = 0;
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call, and `high` is a
    // valid, writable u32 for the high half of the size. GetLastError is read right after on
    // the same thread, as the API documents for telling INVALID_FILE_SIZE from a real size.
    let (low, error) = unsafe {
        let low = GetCompressedFileSizeW(wide.as_ptr(), &mut high);
        let error = if low == INVALID_FILE_SIZE {
            GetLastError()
        } else {
            NO_ERROR
        };
        (low, error)
    };
    if error != NO_ERROR {
        return meta.len();
    }
    (u64::from(high) << 32) | u64::from(low)
}

#[cfg(not(any(unix, windows)))]
fn allocated(_path: &Path, meta: &Metadata) -> u64 {
    meta.len()
}

/// The device an entry lives on (`st_dev`); `None` where the OS has no cheap equivalent.
#[cfg(unix)]
pub(super) fn device(meta: &Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.dev())
}

#[cfg(not(unix))]
pub(super) fn device(_meta: &Metadata) -> Option<u64> {
    None
}

/// `(device, inode)` of a regular file with more than one hard link, so the scanner can
/// count it once. `None` for everything else.
#[cfg(unix)]
pub(super) fn hard_link_key(meta: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (meta.is_file() && meta.nlink() > 1).then(|| (meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
pub(super) fn hard_link_key(_meta: &Metadata) -> Option<(u64, u64)> {
    None
}

/// On Linux, the mount points below `root` to list but not enter, decided from
/// `/proc/self/mountinfo` (`platform::mountinfo::mounts_to_skip`): other filesystems and
/// bind mounts are skipped, btrfs subvolumes of the root's own filesystem entered. When
/// this answers, the scan relies on it instead of comparing `st_dev` (which differs for
/// every btrfs subvolume). `None` off Linux or without a readable mount table.
pub(super) fn mount_plan(root: &Path) -> Option<Vec<PathBuf>> {
    #[cfg(target_os = "linux")]
    {
        use crate::platform::mountinfo::{mounts_to_skip, parse_mountinfo, MOUNTINFO_PATH};
        let text = std::fs::read_to_string(MOUNTINFO_PATH).ok()?;
        mounts_to_skip(&parse_mountinfo(&text), root)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = root;
        None
    }
}

/// The firmlink-duplicate data volume on macOS.
#[cfg(target_os = "macos")]
const MACOS_DATA_VOLUME: &str = "/System/Volumes/Data";

/// Paths under `root` that duplicate data the scan reaches another way, and must be
/// skipped (see the module docs).
pub(super) fn duplicate_mounts(root: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if root == Path::new("/") {
            return vec![PathBuf::from(MACOS_DATA_VOLUME)];
        }
    }
    let _ = root;
    Vec::new()
}

/// Devices besides the root's own that count as "the same filesystem" for `root`: on macOS
/// the data volume behind `/`'s firmlinks.
pub(super) fn sibling_devices(root: &Path) -> Vec<u64> {
    #[cfg(target_os = "macos")]
    {
        if root == Path::new("/") {
            return std::fs::metadata(MACOS_DATA_VOLUME)
                .ok()
                .and_then(|m| device(&m))
                .into_iter()
                .collect();
        }
    }
    let _ = root;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_of_a_small_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("f.bin");
        std::fs::write(&path, vec![7u8; 10_000]).expect("write");
        let meta = std::fs::symlink_metadata(&path).expect("lstat");
        let s = sizes(&path, &meta);
        assert_eq!(s.apparent, 10_000);
        // Whole blocks on Unix filesystems; the exact length for an uncompressed NTFS file.
        assert!(s.allocated >= 10_000, "{s:?}");
    }

    #[test]
    fn roots_are_resolved_to_absolute_paths() {
        let relative = resolve_root(Path::new("some-relative-name"));
        assert!(relative.is_absolute(), "{relative:?}");
        assert!(relative.ends_with("some-relative-name"));
        let dashed = resolve_root(Path::new("-rf"));
        assert!(
            dashed.is_absolute() && dashed.ends_with("-rf"),
            "{dashed:?}"
        );
        let here = std::env::current_dir().expect("cwd");
        assert_eq!(
            resolve_root(Path::new(".")),
            std::fs::canonicalize(&here).expect("canonical cwd")
        );
    }

    #[test]
    fn verbatim_prefixes_are_stripped_from_drive_and_unc_paths() {
        assert_eq!(
            strip_verbatim(r"\\?\C:\Users\me").as_deref(),
            Some(r"C:\Users\me")
        );
        assert_eq!(
            strip_verbatim(r"\\?\UNC\server\share\x").as_deref(),
            Some(r"\\server\share\x")
        );
        assert_eq!(strip_verbatim(r"\\?\Volume{abc}\x"), None);
        assert_eq!(strip_verbatim(r"C:\plain"), None);
        assert_eq!(strip_verbatim("/usr/local"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_macos_boot_volume_alias_resolves_to_the_root() {
        let alias = Path::new("/Volumes/Macintosh HD");
        if alias.exists() {
            assert_eq!(resolve_root(alias), PathBuf::from("/"));
        }
    }

    #[test]
    fn plain_folders_have_no_duplicate_mounts() {
        assert!(duplicate_mounts(Path::new("/home/me")).is_empty());
        assert!(sibling_devices(Path::new("/home/me")).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_root_skips_the_data_volume() {
        assert_eq!(
            duplicate_mounts(Path::new("/")),
            vec![PathBuf::from("/System/Volumes/Data")]
        );
        assert_eq!(sibling_devices(Path::new("/")).len(), 1);
    }
}
