//! The filesystem scan's tree assembly, apart from the walk: the walker (`scan/walk.rs`)
//! turns each entry jwalk yields into a [`Record`], and an [`Assembler`] turns records into
//! the tree, with a stack of open directories (jwalk yields depth first).
//!
//! Keeping the two apart lets the work run in either of two places:
//!
//! - headless (`scan_blocking`, `--dump-top`, the tests): the walking thread applies each
//!   record as it is made;
//! - live (`FsScanner`, the app): the worker thread sends records in batches over a channel
//!   and the UI thread applies them, so the UI owns the growing tree and can lay out the
//!   partial map without locks or copies (`docs/PLAN.md` §3.2, "live map").
//!
//! Either way the same records in the same order build the same tree.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{ScanErrors, ScanProgress, ScanResult};
use crate::model::{NodeFlags, NodeId, SizeMode, Sizes, Tree, TreeBuilder};

/// What the error list says when the tree is full.
pub(super) const FULL_MESSAGE: &str = "The scan stopped: too many files to hold their names \
                                       (over 4 GiB of names). The map shows what was scanned \
                                       until then.";

/// Why an entry could not be read: its path and the OS's message.
#[derive(Debug)]
pub(super) struct Failure {
    pub path: PathBuf,
    pub message: String,
}

/// One walked entry, as the assembler needs it.
#[derive(Debug)]
pub(super) enum Record {
    /// The scanned folder itself (depth 0).
    Root {
        flags: NodeFlags,
        failure: Option<Box<Failure>>,
    },
    /// The scan root is a single file: it becomes the only child of its folder.
    RootFile {
        name: String,
        sizes: Sizes,
        flags: NodeFlags,
        failure: Option<Box<Failure>>,
    },
    /// A folder at `depth` (≥ 1) below the root.
    Dir {
        depth: usize,
        name: String,
        flags: NodeFlags,
        failure: Option<Box<Failure>>,
    },
    /// A file (or symlink) at `depth` (≥ 1).
    File {
        depth: usize,
        name: String,
        sizes: Sizes,
        flags: NodeFlags,
        failure: Option<Box<Failure>>,
    },
    /// The walk could not read `path` at all.
    WalkError { path: PathBuf, message: String },
}

/// Builds the tree from [`Record`]s. See the module docs.
#[derive(Debug)]
pub(super) struct Assembler {
    builder: TreeBuilder,
    /// `stack[d]` is the open directory at depth `d`.
    stack: Vec<NodeId>,
    errors: ScanErrors,
    /// The resolved scan root (the path the walk starts at).
    root: PathBuf,
    progress: Arc<ScanProgress>,
    /// The tree refused an entry ([`TreeBuilder::is_full`]); nothing more is added.
    full: bool,
}

impl Assembler {
    /// Start the tree for a scan of `root` (already resolved, `fs_meta::resolve_root`),
    /// whose names may take `name_limit` bytes. A single file's tree is rooted at its
    /// folder, so the file's path comes out right.
    pub fn new(root: &Path, progress: &Arc<ScanProgress>, name_limit: usize) -> Self {
        let parent = root_file_name(root).and_then(|_| root.parent());
        let named = parent.unwrap_or(root);
        let mut builder = TreeBuilder::with_name_limit(&named.to_string_lossy(), name_limit);
        if named.to_str().is_none() {
            builder.mark(builder.root(), NodeFlags::LOSSY_NAME);
        }
        let stack = vec![builder.root()];
        Self {
            builder,
            stack,
            errors: ScanErrors::default(),
            root: root.to_path_buf(),
            progress: Arc::clone(progress),
            full: false,
        }
    }

    /// The tree so far: unsorted children, correct partial totals.
    pub fn tree(&self) -> &Tree {
        self.builder.tree()
    }

    pub fn errors(&self) -> &ScanErrors {
        &self.errors
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The size the finished tree is sorted by (and the partial tree reads).
    pub fn set_size_mode(&mut self, mode: SizeMode) {
        self.builder.set_size_mode(mode);
    }

    /// Add `record` to the tree. Returns false once the tree is full (the record was not
    /// added, the error list says why); every later record is ignored.
    pub fn apply(&mut self, record: Record) -> bool {
        if self.full {
            return false;
        }
        let root = self.builder.root();
        let (node, failure) = match record {
            Record::WalkError { path, message } => {
                if path == self.root {
                    // The root itself is missing or unreadable: flag it, as any folder.
                    self.builder.mark(root, NodeFlags::ERROR);
                }
                self.errors.record(&path, message);
                return true;
            }
            Record::Root { flags, failure } => {
                self.builder.mark(root, flags);
                (root, failure)
            }
            Record::RootFile {
                name,
                sizes,
                flags,
                failure,
            } => (self.builder.add_file(root, &name, sizes, flags), failure),
            Record::Dir {
                depth,
                name,
                flags,
                failure,
            } => {
                let parent = self.parent_at(depth);
                let id = self.builder.add_dir(parent, &name);
                self.builder.mark(id, flags);
                if !id.is_none() {
                    self.stack.push(id);
                }
                (id, failure)
            }
            Record::File {
                depth,
                name,
                sizes,
                flags,
                failure,
            } => {
                let parent = self.parent_at(depth);
                (self.builder.add_file(parent, &name, sizes, flags), failure)
            }
        };
        if self.builder.is_full() {
            self.full = true;
            self.errors.record(&self.root, FULL_MESSAGE);
            self.progress.add_error();
            return false;
        }
        if let Some(failure) = failure {
            self.builder.mark(node, NodeFlags::ERROR);
            self.errors.record(&failure.path, failure.message);
        }
        true
    }

    /// The open directory an entry at `depth` belongs in (closing deeper ones).
    fn parent_at(&mut self, depth: usize) -> NodeId {
        self.stack.truncate(depth.max(1));
        self.stack
            .last()
            .copied()
            .unwrap_or_else(|| self.builder.root())
    }

    /// Sort the tree and hand over the result; `progress` is marked done.
    pub fn finish(self) -> ScanResult {
        self.progress.mark_done();
        let mode = self.builder.size_mode();
        ScanResult {
            root: self.root,
            tree: self.builder.finish(mode),
            errors: self.errors,
            elapsed: self.progress.elapsed(),
        }
    }
}

/// The file name to list when `root` is a file (or a link to one) rather than a folder.
pub(super) fn root_file_name(root: &Path) -> Option<String> {
    let meta = fs::metadata(root).ok()?;
    if meta.is_dir() {
        return None;
    }
    root.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// A result for a scan that could not run: the root alone, flagged, with `message` as its
/// one error.
pub(super) fn failed_result(root: &Path, message: String, progress: &ScanProgress) -> ScanResult {
    let mut errors = ScanErrors::default();
    errors.record(root, message);
    progress.add_error();
    progress.mark_done();
    let mut builder = TreeBuilder::new(&root.to_string_lossy());
    builder.mark(builder.root(), NodeFlags::ERROR);
    ScanResult {
        root: root.to_path_buf(),
        tree: builder.finish(SizeMode::Allocated),
        errors,
        elapsed: progress.elapsed(),
    }
}
