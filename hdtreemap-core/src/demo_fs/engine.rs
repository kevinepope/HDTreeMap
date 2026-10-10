//! The streaming engine behind the demo scanner: turns a profile's recipe into tree entries,
//! a bounded chunk per [`Generator::step`], depth first.
//!
//! A recipe describes a directory as a function that, given the directory's byte budget,
//! lists its children ([`Entry`]): files with their sizes, and subdirectories with their own
//! budget and recipe. The engine keeps a stack of open directories, each with its remaining
//! children; a step pops entries off it and adds them to the `TreeBuilder`, expanding a
//! subdirectory's recipe only when it is reached. So memory stays bounded by the depth of
//! the tree, and because every random draw happens in the same order however the work is
//! sliced, generating in chunks yields exactly the same tree as generating in one go.

use std::path::Path;

use super::rng::Rng;
use crate::model::{NodeFlags, NodeId, SizeMode, Sizes, Tree, TreeBuilder};
use crate::scan::{ScanErrors, ScanProgress};

/// Expands a directory with `bytes` to spend (and a recipe-specific `arg`) into its
/// children, which share those bytes (give or take a few small fixed-size files).
pub type Expand = fn(&mut Rng, u64, u32) -> Vec<Entry>;

/// A child produced by a recipe.
pub enum Entry {
    /// A file: its name and size in bytes (allocated size, before block rounding).
    File { name: String, bytes: u64 },
    /// A directory, expanded later by `expand(rng, bytes, arg)`.
    Dir {
        name: String,
        bytes: u64,
        expand: Expand,
        arg: u32,
    },
    /// A directory the simulated scan cannot read (permission denied): listed, flagged,
    /// counted as an error, holding nothing.
    Locked { name: String },
}

/// Allocation unit of the simulated filesystems.
const BLOCK: u64 = 4096;

/// What the simulated OS reports for an unreadable directory.
const LOCKED_MESSAGE: &str = "Permission denied (simulated)";

struct Frame {
    node: NodeId,
    entries: std::vec::IntoIter<Entry>,
    /// Length of `Generator::path` before this directory's name was appended.
    path_len: usize,
}

/// A demo tree being generated. See the module docs.
pub struct Generator {
    rng: Rng,
    builder: Option<TreeBuilder>,
    stack: Vec<Frame>,
    /// Path of the deepest open directory, kept incrementally for the progress display.
    path: String,
    errors: ScanErrors,
}

impl Generator {
    /// Start generating below a root named `root_name` with `bytes` to spend, laid out by
    /// `expand`, from `seed`.
    pub fn new(root_name: &str, seed: u64, bytes: u64, expand: Expand) -> Self {
        let mut rng = Rng::new(seed);
        let builder = TreeBuilder::new(root_name);
        let entries = expand(&mut rng, bytes, 0);
        let root = builder.root();
        Self {
            rng,
            builder: Some(builder),
            stack: vec![Frame {
                node: root,
                entries: entries.into_iter(),
                path_len: 0,
            }],
            path: root_name.to_string(),
            errors: ScanErrors::default(),
        }
    }

    /// Add up to `max_entries` entries (files and folders) to the tree, publishing counts to
    /// `progress`. Returns `true` once the whole tree has been generated.
    pub fn step(&mut self, max_entries: u64, progress: &ScanProgress) -> bool {
        let Some(builder) = self.builder.as_mut() else {
            return true;
        };
        let mut added = 0u64;
        let mut files = 0u64;
        let mut file_bytes = 0u64;
        while added < max_entries {
            let Some(frame) = self.stack.last_mut() else {
                break;
            };
            let parent = frame.node;
            let Some(entry) = frame.entries.next() else {
                let len = frame.path_len;
                self.stack.pop();
                if !self.stack.is_empty() {
                    self.path.truncate(len);
                }
                continue;
            };
            added += 1;
            match entry {
                Entry::File { name, bytes } => {
                    let sizes = block_sizes(&mut self.rng, bytes);
                    builder.add_file(parent, &name, sizes, NodeFlags::empty());
                    files += 1;
                    file_bytes += sizes.allocated;
                }
                Entry::Dir {
                    name,
                    bytes,
                    expand,
                    arg,
                } => {
                    let node = builder.add_dir(parent, &name);
                    progress.add_dir();
                    let path_len = self.path.len();
                    self.path.push('/');
                    self.path.push_str(&name);
                    let entries = expand(&mut self.rng, bytes, arg);
                    self.stack.push(Frame {
                        node,
                        entries: entries.into_iter(),
                        path_len,
                    });
                }
                Entry::Locked { name } => {
                    let node = builder.add_dir(parent, &name);
                    builder.mark(node, NodeFlags::ERROR);
                    progress.add_dir();
                    progress.add_error();
                    let path = format!("{}/{}", self.path, name);
                    self.errors.record(Path::new(&path), LOCKED_MESSAGE);
                }
            }
        }
        progress.add_files(files, file_bytes);
        progress.set_current_path(&self.path);
        self.stack.is_empty()
    }

    /// Whether every entry has been generated.
    pub fn is_done(&self) -> bool {
        self.stack.is_empty()
    }

    /// The tree so far (partial totals are correct while generating).
    pub fn tree(&self) -> Option<&Tree> {
        self.builder.as_ref().map(TreeBuilder::tree)
    }

    /// The simulated errors so far.
    pub fn errors(&self) -> &ScanErrors {
        &self.errors
    }

    /// The size the partial tree reads and the finished tree is sorted by.
    pub fn set_size_mode(&mut self, mode: SizeMode) {
        if let Some(builder) = self.builder.as_mut() {
            builder.set_size_mode(mode);
        }
    }

    /// Finish: sort the tree (by its size mode, allocated unless set) and hand it over with
    /// the simulated errors. `None` if already taken.
    pub fn take(&mut self) -> Option<(Tree, ScanErrors)> {
        let builder = self.builder.take()?;
        let mode = builder.size_mode();
        Some((builder.finish(mode), std::mem::take(&mut self.errors)))
    }
}

/// Sizes of a simulated file of about `bytes`: allocated is rounded to the nearest whole
/// block (at least one block for a non-empty file), apparent is somewhere in the last block.
fn block_sizes(rng: &mut Rng, bytes: u64) -> Sizes {
    if bytes == 0 {
        return Sizes::default();
    }
    let blocks = ((bytes + BLOCK / 2) / BLOCK).max(1);
    let allocated = blocks * BLOCK;
    let slack = rng.below(BLOCK);
    Sizes::new(allocated, allocated.saturating_sub(slack).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_level(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
        let half = bytes / 2;
        vec![
            Entry::File {
                name: "a.bin".into(),
                bytes: half,
            },
            Entry::Dir {
                name: "sub".into(),
                bytes: bytes - half,
                expand: leaf,
                arg: rng.range(1, 3) as u32,
            },
            Entry::Locked {
                name: "private".into(),
            },
        ]
    }

    fn leaf(_rng: &mut Rng, bytes: u64, arg: u32) -> Vec<Entry> {
        let n = u64::from(arg);
        (0..n)
            .map(|i| Entry::File {
                name: format!("f{i}.txt"),
                bytes: if i + 1 == n {
                    bytes - bytes / n * (n - 1)
                } else {
                    bytes / n
                },
            })
            .collect()
    }

    #[test]
    fn a_recipe_becomes_a_tree() {
        let progress = ScanProgress::new();
        let mut generator = Generator::new("/demo/t", 5, 1 << 20, two_level);
        assert!(generator.step(u64::MAX, &progress));
        assert!(generator.is_done());
        let (tree, errors) = generator.take().expect("tree");
        assert!(generator.take().is_none());
        let total = tree.total();
        assert!(total.allocated.abs_diff(1 << 20) <= 4 * BLOCK, "{total:?}");
        assert!(total.apparent <= total.allocated);
        assert_eq!(errors.total(), 1);
        assert_eq!(errors.entries()[0].path, Path::new("/demo/t/private"));
        let snap = progress.snapshot();
        assert_eq!(snap.dirs, 2);
        assert_eq!(snap.errors, 1);
        assert_eq!(snap.files, tree.file_count());
        assert_eq!(snap.bytes, total.allocated);
    }

    #[test]
    fn block_sizes_round_to_whole_blocks() {
        let mut rng = Rng::new(1);
        assert_eq!(block_sizes(&mut rng, 0), Sizes::default());
        let s = block_sizes(&mut rng, 10);
        assert_eq!(s.allocated, BLOCK);
        assert!(s.apparent >= 1 && s.apparent <= BLOCK);
        assert_eq!(
            block_sizes(&mut rng, 10 * BLOCK + 100).allocated,
            10 * BLOCK
        );
    }
}
