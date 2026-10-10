//! One entry of the scanned tree: the compact, fixed-size [`Node`] that the arena
//! (`model/tree.rs`) stores millions of, plus the small value types around it ([`NodeId`],
//! [`NodeFlags`], [`SizeMode`], [`Sizes`]).
//!
//! Layout (see `docs/PLAN.md` §3.1): links are `u32` arena indices, the name is an offset
//! into the tree's single name slab (its end is the next node's offset, because names are
//! appended in node order), both sizes are `u64`, the subtree file count is `u32`, the
//! extension is a `u16` id into the tree's extension list, and the flags fit one byte.
//! That is 40 bytes per node; a unit test keeps it from growing.

/// Index of a node in the tree's arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    /// "No node": the end of a sibling list, a childless directory, the root's parent.
    pub const NONE: NodeId = NodeId(u32::MAX);

    pub fn is_none(self) -> bool {
        self == Self::NONE
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Which size the tree is sorted and drawn by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SizeMode {
    /// Space actually used on disk ("size on disk"): blocks allocated, after compression,
    /// sparseness and cloud placeholders. The default, because it is what frees space.
    #[default]
    Allocated,
    /// The file length programs see.
    Apparent,
}

impl SizeMode {
    /// Short label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            SizeMode::Allocated => "Size on disk",
            SizeMode::Apparent => "Apparent size",
        }
    }
}

/// Both sizes of a file (or the totals of a subtree).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sizes {
    pub allocated: u64,
    pub apparent: u64,
}

impl Sizes {
    pub fn new(allocated: u64, apparent: u64) -> Self {
        Self {
            allocated,
            apparent,
        }
    }

    /// The size `mode` reads.
    pub fn get(self, mode: SizeMode) -> u64 {
        match mode {
            SizeMode::Allocated => self.allocated,
            SizeMode::Apparent => self.apparent,
        }
    }
}

/// Per-node flags, one byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeFlags(u8);

impl NodeFlags {
    /// A directory (may have children).
    pub const DIR: NodeFlags = NodeFlags(1);
    /// Its contents (or its metadata) could not be read; counted in the scan's errors.
    pub const ERROR: NodeFlags = NodeFlags(1 << 1);
    /// A symbolic link (or Windows junction), counted by its own size and never followed.
    pub const SYMLINK: NodeFlags = NodeFlags(1 << 2);
    /// Stands for many small entries merged into one (the layout's leftover blocks).
    pub const AGGREGATE: NodeFlags = NodeFlags(1 << 3);
    /// A directory whose contents were not scanned: another filesystem, or a known
    /// duplicate mount (macOS's `/System/Volumes/Data` when scanning `/`).
    pub const SKIPPED: NodeFlags = NodeFlags(1 << 4);
    /// Removed from the tree (moved to the trash); unlinked, kept in the arena.
    pub const REMOVED: NodeFlags = NodeFlags(1 << 5);
    /// The name on disk is not valid Unicode and was stored with replacement characters,
    /// so the stored path does not name the real file (see `Tree::exact_path`).
    pub const LOSSY_NAME: NodeFlags = NodeFlags(1 << 6);

    pub const fn empty() -> Self {
        NodeFlags(0)
    }

    pub fn contains(self, other: NodeFlags) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: NodeFlags) {
        self.0 |= other.0;
    }

    pub fn union(self, other: NodeFlags) -> NodeFlags {
        NodeFlags(self.0 | other.0)
    }

    /// `self` with every flag in `other` cleared.
    pub fn without(self, other: NodeFlags) -> NodeFlags {
        NodeFlags(self.0 & !other.0)
    }
}

/// Extension id for "no extension".
pub const NO_EXT: u16 = 0;

/// One file or directory. Read through the accessors on `Tree`; the fields are crate-private
/// so the arena's invariants (links, name order, size totals) are kept in one place.
#[derive(Clone, Debug)]
pub struct Node {
    /// Allocated size: a file's own, a directory's subtree total.
    pub(crate) allocated: u64,
    /// Apparent size, likewise.
    pub(crate) apparent: u64,
    pub(crate) parent: NodeId,
    pub(crate) first_child: NodeId,
    pub(crate) next_sibling: NodeId,
    /// Start of the name in the tree's name slab.
    pub(crate) name_start: u32,
    /// Files in the subtree (1 for a file, 0 for an empty directory).
    pub(crate) file_count: u32,
    /// Extension id (files only; [`NO_EXT`] otherwise).
    pub(crate) ext: u16,
    pub(crate) flags: NodeFlags,
}

impl Node {
    pub(crate) fn new(parent: NodeId, name_start: u32, flags: NodeFlags) -> Self {
        Self {
            allocated: 0,
            apparent: 0,
            parent,
            first_child: NodeId::NONE,
            next_sibling: NodeId::NONE,
            name_start,
            file_count: 0,
            ext: NO_EXT,
            flags,
        }
    }

    pub fn sizes(&self) -> Sizes {
        Sizes::new(self.allocated, self.apparent)
    }

    pub fn size(&self, mode: SizeMode) -> u64 {
        self.sizes().get(mode)
    }

    pub fn is_dir(&self) -> bool {
        self.flags.contains(NodeFlags::DIR)
    }

    pub fn flags(&self) -> NodeFlags {
        self.flags
    }

    pub fn parent(&self) -> NodeId {
        self.parent
    }

    pub fn first_child(&self) -> NodeId {
        self.first_child
    }

    pub fn next_sibling(&self) -> NodeId {
        self.next_sibling
    }

    pub fn file_count(&self) -> u32 {
        self.file_count
    }

    pub fn ext(&self) -> u16 {
        self.ext
    }
}

/// The extension of a file name, lower-cased: `"Photo.JPG"` → `"jpg"`, `"a.tar.gz"` →
/// `"gz"`. Dot-files (`.bashrc`), names ending in a dot, and implausibly long or non-
/// alphanumeric tails (`"report.final version"`) have none.
pub fn extension_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() || ext.len() > 16 {
        return None;
    }
    if !ext
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some(ext.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_stay_compact() {
        // 5 million files must fit in about 200 MB (docs/PLAN.md §3.1).
        assert_eq!(std::mem::size_of::<Node>(), 40);
    }

    #[test]
    fn extensions_are_lower_cased_tails() {
        assert_eq!(extension_of("Photo.JPG").as_deref(), Some("jpg"));
        assert_eq!(extension_of("archive.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension_of("lib.d.ts").as_deref(), Some("ts"));
        assert_eq!(extension_of(".bashrc"), None);
        assert_eq!(extension_of("Makefile"), None);
        assert_eq!(extension_of("trailing."), None);
        assert_eq!(extension_of("report.final version"), None);
        assert_eq!(extension_of("x.abcdefghijklmnopq"), None);
    }

    #[test]
    fn flags_combine() {
        let mut f = NodeFlags::DIR;
        assert!(f.contains(NodeFlags::DIR));
        assert!(!f.contains(NodeFlags::ERROR));
        f.insert(NodeFlags::ERROR);
        assert!(f.contains(NodeFlags::DIR.union(NodeFlags::ERROR)));
        assert!(!NodeFlags::empty().contains(NodeFlags::SYMLINK));
    }

    #[test]
    fn sizes_pick_the_mode() {
        let s = Sizes::new(4096, 10);
        assert_eq!(s.get(SizeMode::Allocated), 4096);
        assert_eq!(s.get(SizeMode::Apparent), 10);
        assert_eq!(SizeMode::default(), SizeMode::Allocated);
    }
}
