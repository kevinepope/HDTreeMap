//! Edits to the finished tree after a scan: Move to Trash ([`AppState::remove_node`]) and
//! Rescan Folder ([`AppState::start_rescan`]), with what every edit needs afterwards
//! (the extension table recomputed, the zoom root, selection and hover moved off removed
//! nodes, the tree version bumped so the map, tree, legend and status bar follow).
//!
//! A folder rescan runs in the background while the map stays up: its source is pumped
//! once per frame by the same pump as a full scan (`state/pump.rs`), its progress shows in
//! the status bar ([`AppState::rescan_status`]), and when it finishes the folder's part of
//! the result is spliced into the tree (`Tree::replace_subtree`). The source may scan just
//! the folder (the filesystem) or a whole drive the folder is part of (the web demo
//! regenerates its drive); the folder is found in the result by its path either way.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::AppState;
use crate::format::format_bytes;
use crate::model::{ExtTable, NodeId, RemoveError, Sizes, SpliceError, Tree};
use crate::scan::{ProgressSnapshot, ScanPoll, ScanResult, ScanSource};

/// A folder rescan in progress.
pub(super) struct RescanJob {
    node: NodeId,
    /// The folder's path when the rescan started.
    path: PathBuf,
    source: Box<dyn ScanSource>,
}

/// What the status bar shows of a running folder rescan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RescanStatus {
    /// The folder's path.
    pub path: PathBuf,
    pub progress: ProgressSnapshot,
}

impl AppState {
    /// Remove `node` and everything below it from the finished tree (after it was moved to
    /// the trash). Returns the sizes removed.
    pub fn remove_node(&self, node: NodeId) -> Result<Sizes, RemoveError> {
        let removed = {
            let mut finished = self.inner.finished.borrow_mut();
            let Some(done) = finished.as_mut() else {
                return Err(RemoveError::Missing);
            };
            let removed = done.result.tree.remove_subtree(node)?;
            done.extensions = ExtTable::from_tree(&done.result.tree);
            removed
        };
        self.after_edit();
        Ok(removed)
    }

    /// Rescan folder `node` with `source` in the background, replacing any folder rescan
    /// already running. Returns false (and drops `source`) when `node` is not a live folder.
    pub fn start_rescan(&self, node: NodeId, source: Box<dyn ScanSource>) -> bool {
        let path = self
            .with_result(|r, _| {
                let n = r.tree.node(node)?;
                (r.tree.is_live(node) && n.is_dir()).then(|| r.tree.path(node))
            })
            .flatten();
        let Some(path) = path else {
            source.cancel();
            return false;
        };
        self.cancel_rescan();
        *self.inner.rescan.borrow_mut() = Some(RescanJob { node, path, source });
        super::pump::register(&self.inner);
        self.bump_progress();
        true
    }

    /// Stop a running folder rescan (nothing is changed).
    pub fn cancel_rescan(&self) {
        if let Some(job) = self.inner.rescan.borrow_mut().take() {
            job.source.cancel();
        }
    }

    /// Whether a folder rescan is running.
    pub fn is_rescanning(&self) -> bool {
        self.inner.rescan.borrow().is_some()
    }

    /// The running folder rescan's folder and counters.
    pub fn rescan_status(&self) -> Option<RescanStatus> {
        let job = self.inner.rescan.borrow();
        job.as_ref().map(|j| RescanStatus {
            path: j.path.clone(),
            progress: j.source.progress().snapshot(),
        })
    }

    /// Advance the folder rescan by up to `budget`; splice it in when it finishes. Returns
    /// whether it is still running.
    pub(super) fn pump_rescan(&self, budget: Duration) -> bool {
        let poll = {
            let mut job = self.inner.rescan.borrow_mut();
            let Some(job) = job.as_mut() else {
                return false;
            };
            job.source.poll(budget)
        };
        match poll {
            ScanPoll::Running => {
                self.bump_progress();
                true
            }
            ScanPoll::Finished(result) => {
                let job = self.inner.rescan.borrow_mut().take();
                if let Some(job) = job {
                    self.finish_rescan(job.node, &job.path, &result);
                }
                self.bump_progress();
                false
            }
        }
    }

    /// Splice the rescanned folder at `path` from `result` into the tree at `node`.
    fn finish_rescan(&self, node: NodeId, path: &Path, result: &ScanResult) {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        if result.errors.total() > 0 && result.tree.file_count() == 0 && result.root == path {
            if let Some(first) = result.errors.entries().first() {
                self.notify_error(format!("Could not rescan {name}: {}", first.message));
                return;
            }
        }
        let Some(source_root) = locate(&result.tree, path) else {
            self.notify_error(format!(
                "Rescan of {name} found no folder at {}",
                path.display()
            ));
            return;
        };
        let outcome = {
            let mut finished = self.inner.finished.borrow_mut();
            let Some(done) = finished.as_mut() else {
                return;
            };
            let mode = done.result.tree.size_mode();
            let spliced = done
                .result
                .tree
                .replace_subtree(node, &result.tree, source_root)
                .map(|old| (old.get(mode), done.result.tree.size(node)));
            if spliced.is_ok() {
                done.extensions = ExtTable::from_tree(&done.result.tree);
                done.result.errors.replace_under(path, &result.errors);
            }
            spliced
        };
        match outcome {
            Ok((before, after)) => {
                self.after_edit();
                self.notify(format!(
                    "Rescanned {name}: {} \u{2192} {}",
                    format_bytes(before),
                    format_bytes(after)
                ));
            }
            Err(SpliceError::Missing) => {
                self.notify_error(format!("{name} was removed while it was being rescanned"))
            }
            Err(SpliceError::NotAFolder) => {
                self.notify_error(format!("{name} is no longer a folder"))
            }
            Err(SpliceError::Full) => self.notify_error(format!(
                "Could not add the rescan of {name}: too many names. Rescan the whole drive."
            )),
        }
    }

    /// After an edit of the finished tree: move the zoom root off removed folders (to the
    /// nearest live one above), drop a removed selection or hover, and bump the versions.
    fn after_edit(&self) {
        let fix = self.with_result(|r, _| {
            let tree = &r.tree;
            let zoom = self.zoom_root();
            let zoom = std::iter::once(zoom)
                .chain(tree.ancestors(zoom))
                .find(|&n| tree.is_live(n))
                .unwrap_or_else(|| tree.root());
            let selected = self.selected().filter(|&n| tree.is_live(n));
            let hovered = self.hovered().filter(|h| tree.is_live(h.node));
            (zoom, selected, hovered)
        });
        if let Some((zoom, selected, hovered)) = fix {
            self.set_zoom_root(zoom);
            self.set_selected(selected);
            self.set_hovered(hovered);
        }
        self.tree_changed();
    }

    fn bump_progress(&self) {
        let v = self.inner.progress_version.get();
        self.inner.progress_version.set(v.wrapping_add(1));
        agg_gui::animation::request_draw();
    }
}

/// The folder at `path` in `tree`: its root when the tree is a scan of `path`, else the
/// folder reached by following `path` below the root's path.
fn locate(tree: &Tree, path: &Path) -> Option<NodeId> {
    let base = tree.path(tree.root());
    let below = path.strip_prefix(&base).ok()?;
    let mut node = tree.root();
    for part in below.components() {
        let name = part.as_os_str().to_str()?;
        node = tree.children(node).find(|&c| tree.name(c) == name)?;
    }
    tree.node(node).is_some_and(|n| n.is_dir()).then_some(node)
}
