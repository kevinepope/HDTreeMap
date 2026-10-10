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
//! - macOS: scanning `/` would visit the data volume twice, once through the firmlinks
//!   (`/Users`, `/Applications`, ...) and once at `/System/Volumes/Data`, so that path is
//!   skipped and its device counts as the root's.

use std::fs::Metadata;
use std::path::{Path, PathBuf};

use crate::model::Sizes;

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
