//! macOS: a directory's metadata in bulk (`getattrlistbulk(2)`), with what APFS knows about
//! clones, for the scanner's per-entry filter (`scan/walk.rs`).
//!
//! One `getattrlistbulk` call returns the attributes of many entries at once, so reading a
//! directory this way replaces one `lstat` per entry, and it also returns each file's
//! APFS clone facts at no extra cost:
//!
//! - `ATTR_CMNEXT_EXT_FLAGS`: `EF_MAY_SHARE_BLOCKS` is set when a file may share blocks
//!   with another (a clone, `cp -c`, or a copy the Finder made).
//! - `ATTR_CMNEXT_CLONEID`: files that are pure clones of each other have the same id.
//! - `ATTR_CMNEXT_PRIVATESIZE`: the bytes only this file holds (freed if it were deleted).
//!
//! The bulk read asks only for the flags: computing the clone id and private size for
//! every entry doubled the scan's system time (measured on a 1 M-file folder). Only a file
//! that may share blocks gets a second call, `getattrlist(2)` for those two ([`clone_facts`]).
//! The scanner then counts a clone family's shared blocks once ([`CloneKey`], as it counts
//! a hard-linked file once): the first member it meets is counted at its full allocated
//! size, every later member at its private size. Apparent sizes are not affected.
//!
//! Only metadata is read: no file is opened, so cloud placeholders (iCloud, File
//! Provider) are never downloaded. Entries the bulk read cannot vouch for (an error, a
//! mount point or firmlink, whose device is that of the directory holding it) are left to
//! the caller's `lstat`.

use std::collections::HashMap;
use std::ffi::{c_int, c_void, OsStr, OsString};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// `attrlist` (`<sys/attr.h>`).
#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

extern "C" {
    /// `int getattrlistbulk(int dirfd, struct attrlist *, void *buf, size_t size,
    /// uint64_t options)`.
    fn getattrlistbulk(
        dirfd: c_int,
        attr_list: *mut AttrList,
        buf: *mut c_void,
        size: usize,
        options: u64,
    ) -> c_int;
}

const ATTR_BIT_MAP_COUNT: u16 = 5;
const ATTR_CMN_NAME: u32 = 0x0000_0001;
const ATTR_CMN_DEVID: u32 = 0x0000_0002;
const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
const ATTR_CMN_FLAGS: u32 = 0x0004_0000;
const ATTR_CMN_FILEID: u32 = 0x0200_0000;
const ATTR_CMN_ERROR: u32 = 0x2000_0000;
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
const ATTR_DIR_MOUNTSTATUS: u32 = 0x0000_0004;
const ATTR_FILE_LINKCOUNT: u32 = 0x0000_0001;
const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;
const ATTR_FILE_DATALENGTH: u32 = 0x0000_0200;
const ATTR_CMNEXT_PRIVATESIZE: u32 = 0x0000_0008;
const ATTR_CMNEXT_CLONEID: u32 = 0x0000_0100;
const ATTR_CMNEXT_EXT_FLAGS: u32 = 0x0000_0200;
const FSOPT_ATTR_CMN_EXTENDED: u64 = 0x0000_0020;
const DIR_MNTSTATUS_MNTPOINT: u32 = 0x0000_0001;
/// `st_flags`: the directory is a firmlink (`/Users` on the system volume).
const SF_FIRMLINK: u32 = 0x0080_0000;
/// `<sys/stat.h>` extended flag: the file may share blocks with another.
pub const EF_MAY_SHARE_BLOCKS: u64 = 0x0000_0001;

/// `enum vtype` (`<sys/vnode.h>`).
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VLNK: u32 = 5;

/// What kind of entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    /// A device, socket, FIFO...: counted like a file.
    Other,
}

/// One entry's attributes, as `getattrlistbulk` reported them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BulkEntry {
    pub kind: Kind,
    /// `st_dev`, as std reports it.
    pub device: u64,
    pub file_id: u64,
    /// Hard links (files only; 0 for directories).
    pub links: u32,
    /// Allocated bytes (all forks): `st_blocks * 512`.
    pub allocated: u64,
    /// The data's length: `st_size`.
    pub apparent: u64,
    /// See the module docs.
    pub ext_flags: u64,
}

impl BulkEntry {
    /// Whether this is a file that may share blocks with another (see [`clone_facts`]).
    pub fn may_share_blocks(&self) -> bool {
        self.kind == Kind::File && self.ext_flags & EF_MAY_SHARE_BLOCKS != 0
    }
}

extern "C" {
    /// `int getattrlist(const char *path, struct attrlist *, void *buf, size_t size,
    /// unsigned int options)`.
    fn getattrlist(
        path: *const std::ffi::c_char,
        attr_list: *mut AttrList,
        buf: *mut c_void,
        size: usize,
        options: u32,
    ) -> c_int;
}

const FSOPT_NOFOLLOW: u32 = 0x0000_0001;

/// The clone family of the file at `path` on `device`, and its private bytes: one
/// `getattrlist` call. `None` when it cannot be read or has no clone id.
pub fn clone_facts(path: &Path, device: u64) -> Option<(CloneKey, u64)> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut list = AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: ATTR_CMNEXT_PRIVATESIZE | ATTR_CMNEXT_CLONEID,
    };
    let mut buf = [0u8; 64];
    // SAFETY: `c_path` is a NUL-terminated path that outlives the call; `list` is a valid
    // attrlist; `buf` is writable for its whole length, which is passed as the size.
    let rc = unsafe {
        getattrlist(
            c_path.as_ptr(),
            &mut list,
            buf.as_mut_ptr().cast(),
            buf.len(),
            FSOPT_NOFOLLOW | FSOPT_ATTR_CMN_EXTENDED as u32,
        )
    };
    if rc != 0 {
        return None;
    }
    // Length, the returned attribute_set_t, then PRIVATESIZE and CLONEID.
    let fork = read_u32(&buf, 20)?;
    let mut c = Cursor { rec: &buf, at: 24 };
    let private = c.u64_if(fork, ATTR_CMNEXT_PRIVATESIZE)??;
    let clone_id = c.u64_if(fork, ATTR_CMNEXT_CLONEID)??;
    (clone_id != 0).then_some((
        CloneKey { device, clone_id },
        (private as i64).max(0) as u64,
    ))
}

/// A clone family: its device and clone id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CloneKey {
    pub device: u64,
    pub clone_id: u64,
}

/// The attributes asked for, in the order they come back.
fn attr_list() -> AttrList {
    AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_NAME
            | ATTR_CMN_ERROR
            | ATTR_CMN_DEVID
            | ATTR_CMN_OBJTYPE
            | ATTR_CMN_FLAGS
            | ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: ATTR_DIR_MOUNTSTATUS,
        fileattr: ATTR_FILE_LINKCOUNT | ATTR_FILE_ALLOCSIZE | ATTR_FILE_DATALENGTH,
        // Only the flags: see the module docs.
        forkattr: ATTR_CMNEXT_EXT_FLAGS,
    }
}

/// Bytes per bulk call.
const BUFFER: usize = 128 * 1024;

/// The attributes of every entry of `dir` that the bulk read can vouch for, by name.
pub fn read_dir(dir: &Path) -> std::io::Result<HashMap<OsString, BulkEntry>> {
    let file = File::open(dir)?;
    let mut buf = vec![0u8; BUFFER];
    let mut out = HashMap::new();
    loop {
        let mut list = attr_list();
        // SAFETY: `file` is an open directory for the whole call; `list` is a valid,
        // writable attrlist; `buf` is a writable buffer of `buf.len()` bytes the kernel
        // fills with at most that many bytes. The return value is the number of entries
        // written (0 at the end, -1 on error).
        let n = unsafe {
            getattrlistbulk(
                file.as_raw_fd(),
                &mut list,
                buf.as_mut_ptr().cast(),
                buf.len(),
                FSOPT_ATTR_CMN_EXTENDED,
            )
        };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            return Ok(out);
        }
        let mut at = 0usize;
        for _ in 0..n {
            let Some(len) = read_u32(&buf, at) else {
                break;
            };
            let end = at.saturating_add(len as usize).min(buf.len());
            if let Some((name, entry)) = parse_entry(&buf[at..end]) {
                out.insert(name, entry);
            }
            if len == 0 {
                break;
            }
            at = end;
        }
    }
}

/// Reads a record's attributes in order.
struct Cursor<'a> {
    rec: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    /// The next `u32` if `bit` is in `mask` (an attribute that came back), else `0`.
    fn u32_if(&mut self, mask: u32, bit: u32) -> Option<u32> {
        if mask & bit == 0 {
            return Some(0);
        }
        let v = read_u32(self.rec, self.at)?;
        self.at += 4;
        Some(v)
    }

    /// The next `u64` if `bit` is in `mask`, else `None`. Attributes are only 4-byte
    /// aligned, so 64-bit values are read byte by byte.
    fn u64_if(&mut self, mask: u32, bit: u32) -> Option<Option<u64>> {
        if mask & bit == 0 {
            return Some(None);
        }
        let v = read_u64(self.rec, self.at)?;
        self.at += 8;
        Some(Some(v))
    }
}

/// One entry's record (starting with its length): its name and attributes, or `None`
/// when the bulk read cannot vouch for it (see the module docs) or the record is short.
fn parse_entry(rec: &[u8]) -> Option<(OsString, BulkEntry)> {
    // After the length: the returned attribute_set_t (common, vol, dir, file, fork).
    let common = read_u32(rec, 4)?;
    let dir = read_u32(rec, 12)?;
    let fileattr = read_u32(rec, 16)?;
    let fork = read_u32(rec, 20)?;
    let mut c = Cursor { rec, at: 24 };
    // ATTR_CMN_ERROR comes first, right after the returned set.
    if c.u32_if(common, ATTR_CMN_ERROR)? != 0 || common & ATTR_CMN_NAME == 0 {
        return None;
    }
    // The name: an attrreference (offset from itself, length with the NUL).
    let name_ref = c.at;
    let offset = read_u32(rec, c.at)? as i32;
    let len = read_u32(rec, c.at + 4)? as usize;
    c.at += 8;
    let start = usize::try_from(name_ref as i64 + i64::from(offset)).ok()?;
    let bytes = rec.get(start..start.checked_add(len)?)?;
    let bytes = bytes.split(|&b| b == 0).next().unwrap_or(bytes);
    let name = OsStr::from_bytes(bytes).to_os_string();
    if common & (ATTR_CMN_DEVID | ATTR_CMN_OBJTYPE | ATTR_CMN_FILEID)
        != (ATTR_CMN_DEVID | ATTR_CMN_OBJTYPE | ATTR_CMN_FILEID)
    {
        return None;
    }
    // dev_t is an i32; std widens st_dev the same way.
    let device = c.u32_if(common, ATTR_CMN_DEVID)? as i32 as i64 as u64;
    let objtype = c.u32_if(common, ATTR_CMN_OBJTYPE)?;
    let flags = c.u32_if(common, ATTR_CMN_FLAGS)?;
    let file_id = c.u64_if(common, ATTR_CMN_FILEID)??;
    let mount = c.u32_if(dir, ATTR_DIR_MOUNTSTATUS)?;
    let links = c.u32_if(fileattr, ATTR_FILE_LINKCOUNT)?;
    let allocated = c.u64_if(fileattr, ATTR_FILE_ALLOCSIZE)?;
    let apparent = c.u64_if(fileattr, ATTR_FILE_DATALENGTH)?.unwrap_or(0);
    let ext_flags = c.u64_if(fork, ATTR_CMNEXT_EXT_FLAGS)?.unwrap_or(0);
    let kind = match objtype {
        VREG => Kind::File,
        VDIR => Kind::Dir,
        VLNK => Kind::Symlink,
        _ => Kind::Other,
    };
    if kind == Kind::Dir && (mount & DIR_MNTSTATUS_MNTPOINT != 0 || flags & SF_FIRMLINK != 0) {
        // Its attributes are the covered directory's (or the firmlink's), not those of
        // what the walk would enter.
        return None;
    }
    let allocated = match (kind, allocated) {
        (Kind::Dir, _) => 0,
        (_, Some(bytes)) => bytes,
        // A file without its size: let lstat answer.
        (_, None) => return None,
    };
    Some((
        name,
        BulkEntry {
            kind,
            device,
            file_id,
            links,
            allocated,
            apparent,
            ext_flags,
        },
    ))
}

fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
    let bytes = buf.get(at..at.checked_add(4)?)?;
    Some(u32::from_ne_bytes(bytes.try_into().ok()?))
}

fn read_u64(buf: &[u8], at: usize) -> Option<u64> {
    let bytes = buf.get(at..at.checked_add(8)?)?;
    Some(u64::from_ne_bytes(bytes.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    /// Every entry the bulk read vouches for agrees with `lstat`.
    fn assert_matches_lstat(dir: &Path) {
        let bulk = read_dir(dir).expect("bulk read");
        let listed = std::fs::read_dir(dir).expect("read_dir").count();
        assert!(bulk.len() <= listed);
        for (name, b) in &bulk {
            let meta = std::fs::symlink_metadata(dir.join(name)).expect("lstat");
            let what = format!("{}", dir.join(name).display());
            let kind = if meta.file_type().is_symlink() {
                Kind::Symlink
            } else if meta.is_dir() {
                Kind::Dir
            } else if meta.is_file() {
                Kind::File
            } else {
                Kind::Other
            };
            assert_eq!(b.kind, kind, "{what}");
            assert_eq!(b.device, meta.dev(), "{what}");
            assert_eq!(b.file_id, meta.ino(), "{what}");
            if kind != Kind::Dir {
                assert_eq!(b.allocated, meta.blocks() * 512, "{what}");
                assert_eq!(b.apparent, meta.len(), "{what}");
                assert_eq!(u64::from(b.links), meta.nlink(), "{what}");
            }
        }
    }

    #[test]
    fn bulk_attributes_agree_with_lstat() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("data.bin"), vec![1u8; 70_000]).expect("write");
        std::fs::write(root.join("empty"), b"").expect("write");
        std::fs::create_dir(root.join("sub")).expect("mkdir");
        std::os::unix::fs::symlink("data.bin", root.join("link")).expect("symlink");
        std::fs::hard_link(root.join("data.bin"), root.join("hard")).expect("hard link");
        let bulk = read_dir(root).expect("bulk");
        assert_eq!(bulk.len(), 5);
        assert_eq!(bulk[OsStr::new("hard")].links, 2);
        assert_eq!(bulk[OsStr::new("link")].kind, Kind::Symlink);
        assert_matches_lstat(root);
        // Real system folders: binaries, compressed files, frameworks.
        for system in ["/usr/bin", "/usr/lib", "/System/Library/CoreServices"] {
            assert_matches_lstat(Path::new(system));
        }
    }

    #[test]
    fn mount_points_and_firmlinks_are_left_to_lstat() {
        // `/dev` is a mount point; `/Users` a firmlink to the data volume.
        let bulk = read_dir(Path::new("/")).expect("bulk read of /");
        assert!(!bulk.contains_key(OsStr::new("dev")));
        if Path::new("/usr/share/firmlinks").exists() {
            assert!(!bulk.contains_key(OsStr::new("Users")));
        }
        assert!(read_dir(Path::new("/no/such/folder")).is_err());
    }
}
