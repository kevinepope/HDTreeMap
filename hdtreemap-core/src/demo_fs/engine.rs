//! The streaming engine behind the demo scanner: turns a profile's recipe into tree entries,
//! a bounded chunk per [`Generator::step`].
//!
//! A recipe describes a directory as a function that, given the directory's byte budget,
//! lists its children ([`Entry`]): files with their sizes, and subdirectories with their own
//! budget and recipe. The root's children are added first. Each top-level folder then
//! becomes a *lane*: a stack of open directories, each with its remaining children, walked
//! depth first, expanding a subdirectory's recipe only when it is reached. So memory stays
//! bounded by the depth of the tree times the (few) top-level folders.
//!
//! The lanes take turns of [`TURN`] entries, and each turn goes to the lane that has
//! generated the smallest share of its byte budget. Every top-level folder therefore grows
//! at about the same rate, and the live map during a demo scan looks like a scaled-down
//! version of the finished one (a varied mix from the start) rather than filling one
//! folder at a time. Each lane draws from its own random stream (seeded from the drive's
//! seed and the lane's position), and the turn state carries over between steps, so the
//! tree is the same whatever order the lanes run in and however the work is sliced.

use std::path::Path;

use super::rng::Rng;
use web_time::Instant;

use crate::model::{ExtTable, NodeFlags, NodeId, SizeMode, Sizes, Tree, TreeBuilder};
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

/// Entries a lane generates before the next lane is picked.
pub const TURN: u64 = 64;

/// The seed of lane `index`'s random stream.
fn lane_seed(seed: u64, index: usize) -> u64 {
    seed ^ (index as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03)
}

struct Frame {
    node: NodeId,
    entries: std::vec::IntoIter<Entry>,
    /// Length of the lane's path before this directory's name was appended.
    path_len: usize,
}

/// One top-level folder being generated. See the module docs.
struct Lane {
    rng: Rng,
    stack: Vec<Frame>,
    /// Path of the lane's deepest open directory, kept incrementally for the progress
    /// display.
    path: String,
    /// The folder's byte budget, and the allocated bytes generated in it so far.
    budget: u64,
    generated: u64,
}

impl Lane {
    /// Whether this lane has generated a smaller share of its budget than `other`.
    fn behind(&self, other: &Lane) -> bool {
        // generated / budget < other.generated / other.budget, without division.
        u128::from(self.generated) * u128::from(other.budget.max(1))
            < u128::from(other.generated) * u128::from(self.budget.max(1))
    }
}

/// A demo tree being generated. See the module docs.
pub struct Generator {
    seed: u64,
    /// Draws for the root's own files (sizes) and locked folders.
    rng: Rng,
    builder: Option<TreeBuilder>,
    /// The root's children not yet added.
    root_entries: std::vec::IntoIter<Entry>,
    root_name: String,
    /// Top-level folders still being generated, in the order they were listed.
    lanes: Vec<Lane>,
    /// Lanes created so far (each one's seed index).
    lanes_started: usize,
    /// The lane taking the current turn, and how many entries the turn has left.
    turn: Option<usize>,
    turn_left: u64,
    /// The path shown as being read.
    current: String,
    errors: ScanErrors,
}

impl Generator {
    /// Start generating below a root named `root_name` with `bytes` to spend, laid out by
    /// `expand`, from `seed`.
    pub fn new(root_name: &str, seed: u64, bytes: u64, expand: Expand) -> Self {
        let mut rng = Rng::new(seed);
        let builder = TreeBuilder::new(root_name);
        let entries = expand(&mut rng, bytes, 0);
        Self {
            seed,
            rng,
            builder: Some(builder),
            root_entries: entries.into_iter(),
            root_name: root_name.to_string(),
            lanes: Vec::new(),
            lanes_started: 0,
            turn: None,
            turn_left: 0,
            current: root_name.to_string(),
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
            // The root's own children first: the top-level folders open as lanes.
            if let Some(entry) = self.root_entries.next() {
                added += 1;
                let root = builder.root();
                match entry {
                    Entry::File { name, bytes } => {
                        let sizes = block_sizes(&mut self.rng, bytes);
                        builder.add_file(root, &name, sizes, NodeFlags::empty());
                        files += 1;
                        file_bytes += sizes.allocated;
                    }
                    Entry::Dir {
                        name,
                        bytes,
                        expand,
                        arg,
                    } => {
                        let node = builder.add_dir(root, &name);
                        progress.add_dir();
                        let mut rng = Rng::new(lane_seed(self.seed, self.lanes_started));
                        self.lanes_started += 1;
                        let entries = expand(&mut rng, bytes, arg);
                        let path = format!("{}/{}", self.root_name, name);
                        self.lanes.push(Lane {
                            rng,
                            stack: vec![Frame {
                                node,
                                entries: entries.into_iter(),
                                path_len: path.len(),
                            }],
                            path,
                            budget: bytes,
                            generated: 0,
                        });
                    }
                    Entry::Locked { name } => {
                        let path = format!("{}/{}", self.root_name, name);
                        lock(builder, root, &name, &path, progress, &mut self.errors);
                    }
                }
                continue;
            }
            let index = match self.turn.filter(|_| self.turn_left > 0) {
                Some(index) => index,
                None => {
                    let Some(index) = (0..self.lanes.len()).reduce(|best, i| {
                        if self.lanes[i].behind(&self.lanes[best]) {
                            i
                        } else {
                            best
                        }
                    }) else {
                        break;
                    };
                    self.turn = Some(index);
                    self.turn_left = TURN;
                    index
                }
            };
            let lane = &mut self.lanes[index];
            let Some(frame) = lane.stack.last_mut() else {
                // The lane is finished: the next turn picks among the others.
                self.lanes.remove(index);
                self.turn = None;
                continue;
            };
            let parent = frame.node;
            let Some(entry) = frame.entries.next() else {
                let len = frame.path_len;
                lane.stack.pop();
                lane.path.truncate(len);
                continue;
            };
            added += 1;
            self.turn_left -= 1;
            match entry {
                Entry::File { name, bytes } => {
                    let sizes = block_sizes(&mut lane.rng, bytes);
                    builder.add_file(parent, &name, sizes, NodeFlags::empty());
                    files += 1;
                    file_bytes += sizes.allocated;
                    lane.generated += sizes.allocated;
                }
                Entry::Dir {
                    name,
                    bytes,
                    expand,
                    arg,
                } => {
                    let node = builder.add_dir(parent, &name);
                    progress.add_dir();
                    let path_len = lane.path.len();
                    lane.path.push('/');
                    lane.path.push_str(&name);
                    let entries = expand(&mut lane.rng, bytes, arg);
                    lane.stack.push(Frame {
                        node,
                        entries: entries.into_iter(),
                        path_len,
                    });
                }
                Entry::Locked { name } => {
                    let path = format!("{}/{}", lane.path, name);
                    lock(builder, parent, &name, &path, progress, &mut self.errors);
                }
            }
            self.current.clone_from(&lane.path);
        }
        progress.add_files(files, file_bytes);
        progress.set_current_path(&self.current);
        self.is_done()
    }

    /// Whether every entry has been generated.
    pub fn is_done(&self) -> bool {
        self.root_entries.len() == 0 && self.lanes.iter().all(|l| l.stack.is_empty())
    }
    /// The tree so far (partial totals are correct while generating).
    pub fn tree(&self) -> Option<&Tree> {
        self.builder.as_ref().map(TreeBuilder::tree)
    }

    /// Totals per extension of the tree so far.
    pub fn extensions(&self) -> Option<ExtTable> {
        self.builder.as_ref().map(TreeBuilder::extensions)
    }

    /// Sort the generated tree's child lists until done (true) or `deadline` (false).
    pub fn sort_some(&mut self, deadline: Option<Instant>) -> bool {
        self.builder.as_mut().is_none_or(|b| b.sort_some(deadline))
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
        Some((builder.finish_sorted(), std::mem::take(&mut self.errors)))
    }
}

/// Add the unreadable folder `name` (at `path`) under `parent`, counted as an error.
fn lock(
    builder: &mut TreeBuilder,
    parent: NodeId,
    name: &str,
    path: &str,
    progress: &ScanProgress,
    errors: &mut ScanErrors,
) {
    let node = builder.add_dir(parent, name);
    builder.mark(node, NodeFlags::ERROR);
    progress.add_dir();
    progress.add_error();
    errors.record(Path::new(path), LOCKED_MESSAGE);
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
