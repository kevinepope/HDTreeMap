//! `FsScanner`: the native scan source. Walks a real folder with jwalk (`scan/walk.rs`) on
//! a worker thread and builds the tree on the thread that polls it (`docs/PLAN.md` §3.2).
//!
//! The worker turns entries into records and sends them over a channel in batches (every
//! [`BATCH_RECORDS`] records or [`BATCH_INTERVAL`], whichever comes first): first the
//! [`Assembler`] for the root, then the batches, then "done". [`ScanSource::poll`] (the UI
//! thread, once a frame) applies whatever has arrived within its time budget, so the UI
//! owns the growing tree: the live map lays out the partial tree directly, with no locks
//! and no copies ([`ScanSource::partial`]). When the walk is done and every batch applied,
//! the polls sort the tree, a time slice each (a million-file tree takes about 100 ms),
//! and then hand back the [`ScanResult`].
//!
//! [`scan_blocking`] runs the same walk and assembly on the calling thread, for
//! `--dump-top` and the tests. Dropping an `FsScanner` cancels its walk.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use agg_gui::ui_thread::UiQueue;
use web_time::Instant;

use super::assemble::{failed_result, root_file_name, Assembler, Record};
use super::fs_meta;
use super::walk::walk;
use super::{PartialScan, ScanPoll, ScanProgress, ScanResult, ScanSource};
use crate::model::{ExtTable, SizeMode, NAME_SLAB_LIMIT};

/// Most records per batch sent to the polling thread.
pub const BATCH_RECORDS: usize = 4096;
/// Longest a record waits on the worker before its batch is sent.
pub const BATCH_INTERVAL: Duration = Duration::from_millis(50);

/// What the worker sends the polling thread.
enum Message {
    /// The tree's assembler (made on the worker: it looks at the root on disk).
    Start(Box<Assembler>),
    Batch(Vec<Record>),
    /// The walk ended (finished or cancelled); no more batches follow.
    Done,
}

/// A scan of a real folder running on a worker thread. Dropping it cancels the scan.
pub struct FsScanner {
    progress: Arc<ScanProgress>,
    messages: Receiver<Message>,
    root: PathBuf,
    assembler: Option<Box<Assembler>>,
    /// The size mode to sort by (chosen before the assembler arrived, or since).
    mode: SizeMode,
    /// The walk ended (or the tree filled up): only sorting is left.
    walked: bool,
    finished: bool,
}

impl FsScanner {
    /// Start scanning `root` on a new thread. `notify` (the UI thread's queue) is woken as
    /// progress is published, as batches go out, and when the walk ends.
    pub fn start(root: PathBuf, notify: Option<UiQueue>) -> Self {
        let root = fs_meta::resolve_root(&root);
        let progress = Arc::new(ScanProgress::new());
        let (tx, rx) = mpsc::channel();
        let worker_progress = Arc::clone(&progress);
        let worker_root = root.clone();
        // If the thread cannot be started, the closure (and `tx`) is dropped unrun; the
        // poll then sees the channel closed with no assembler and reports the failure.
        let _ = std::thread::Builder::new()
            .name("hdtreemap-scan".into())
            .spawn(move || {
                walk_to_channel(&worker_root, &worker_progress, notify.as_ref(), &tx);
                // The receiver is gone only when the scan was dropped (cancelled).
                let _ = tx.send(Message::Done);
                if let Some(queue) = &notify {
                    queue.signal_async_state_change();
                }
            });
        Self {
            progress,
            messages: rx,
            root,
            assembler: None,
            mode: SizeMode::Allocated,
            walked: false,
            finished: false,
        }
    }

    /// Sort the tree until `deadline`; once it is sorted, hand it over.
    fn sort_then_finish(&mut self, deadline: Option<Instant>) -> ScanPoll {
        if let Some(assembler) = self.assembler.as_mut() {
            if !assembler.sort_some(deadline) {
                return ScanPoll::Running;
            }
        }
        self.finish()
    }

    /// Hand the (sorted) tree over (once).
    fn finish(&mut self) -> ScanPoll {
        self.finished = true;
        let result = match self.assembler.take() {
            Some(assembler) => assembler.finish(),
            // The worker died (or never started) before it looked at the root.
            None => failed_result(
                &self.root,
                "The scan stopped unexpectedly.".to_string(),
                &self.progress,
            ),
        };
        ScanPoll::Finished(Box::new(result))
    }
}

/// The worker: walk `root`, sending the assembler and then batches of records to `tx`
/// until the walk ends, the scan is cancelled, or the receiver is dropped.
fn walk_to_channel(
    root: &Path,
    progress: &Arc<ScanProgress>,
    notify: Option<&UiQueue>,
    tx: &Sender<Message>,
) {
    let assembler = Assembler::new(root, progress, NAME_SLAB_LIMIT);
    if tx.send(Message::Start(Box::new(assembler))).is_err() {
        return;
    }
    let file_root = root_file_name(root);
    let mut batch = Vec::with_capacity(BATCH_RECORDS);
    let mut last_send = Instant::now();
    let mut connected = true;
    let send = |batch: &mut Vec<Record>| {
        let full = std::mem::replace(batch, Vec::with_capacity(BATCH_RECORDS));
        tx.send(Message::Batch(full)).is_ok()
    };
    walk(
        root,
        file_root.as_deref(),
        progress,
        notify,
        &mut |record| {
            batch.push(record);
            if batch.len() >= BATCH_RECORDS
                || (batch.len() % 64 == 0 && last_send.elapsed() >= BATCH_INTERVAL)
            {
                last_send = Instant::now();
                connected = send(&mut batch);
            }
            connected
        },
    );
    if connected && !batch.is_empty() {
        send(&mut batch);
    }
}

impl ScanSource for FsScanner {
    fn progress(&self) -> Arc<ScanProgress> {
        Arc::clone(&self.progress)
    }

    fn poll(&mut self, budget: Duration) -> ScanPoll {
        if self.finished {
            return ScanPoll::Running;
        }
        let deadline = Instant::now().checked_add(budget);
        if self.walked {
            return self.sort_then_finish(deadline);
        }
        loop {
            match self.messages.try_recv() {
                Ok(Message::Start(mut assembler)) => {
                    assembler.set_size_mode(self.mode);
                    self.assembler = Some(assembler);
                }
                Ok(Message::Batch(records)) => {
                    let Some(assembler) = self.assembler.as_mut() else {
                        continue;
                    };
                    for record in records {
                        if !assembler.apply(record) {
                            // The tree is full: stop the walk and show what fitted.
                            self.progress.cancel();
                            self.walked = true;
                            return self.sort_then_finish(deadline);
                        }
                    }
                }
                Ok(Message::Done) | Err(TryRecvError::Disconnected) => {
                    self.walked = true;
                    return self.sort_then_finish(deadline);
                }
                Err(TryRecvError::Empty) => return ScanPoll::Running,
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return ScanPoll::Running;
            }
        }
    }

    fn partial(&self) -> Option<PartialScan<'_>> {
        let assembler = self.assembler.as_ref()?;
        Some(PartialScan {
            root: assembler.root(),
            tree: assembler.tree(),
            errors: assembler.errors(),
        })
    }

    fn partial_extensions(&self) -> Option<ExtTable> {
        self.assembler.as_ref().map(|a| a.extensions())
    }

    fn set_size_mode(&mut self, mode: SizeMode) {
        self.mode = mode;
        if let Some(assembler) = self.assembler.as_mut() {
            assembler.set_size_mode(mode);
        }
    }
}

impl Drop for FsScanner {
    fn drop(&mut self) {
        self.progress.cancel();
    }
}

/// Scan `root` on the calling thread (a headless `--dump-top`, the tests), publishing to
/// `progress` and waking `notify` at most every [`super::NOTIFY_INTERVAL`]. Returns early,
/// with the partial tree, once `progress` is cancelled.
pub fn scan_blocking(
    root: &Path,
    progress: &Arc<ScanProgress>,
    notify: Option<&UiQueue>,
) -> ScanResult {
    scan_with_limit(root, progress, notify, NAME_SLAB_LIMIT)
}

/// [`scan_blocking`] with the tree's names capped at `name_limit` bytes. When they reach
/// it the scan stops, and says so in its errors.
fn scan_with_limit(
    root: &Path,
    progress: &Arc<ScanProgress>,
    notify: Option<&UiQueue>,
    name_limit: usize,
) -> ScanResult {
    let root = fs_meta::resolve_root(root);
    let mut assembler = Assembler::new(&root, progress, name_limit);
    let file_root = root_file_name(&root);
    walk(
        &root,
        file_root.as_deref(),
        progress,
        notify,
        &mut |record| assembler.apply(record),
    );
    assembler.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_name_slab_stops_the_scan_with_an_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        for i in 0..20 {
            std::fs::write(dir.path().join(format!("file-{i:02}.txt")), b"x").expect("write");
        }
        let progress = Arc::new(ScanProgress::new());
        let root_len = resolve_root_len(dir.path());
        let result = scan_with_limit(dir.path(), &progress, None, root_len + 40);
        assert!(result.tree.file_count() < 20, "the scan stopped early");
        assert!(result.tree.file_count() > 0);
        assert_eq!(result.errors.total(), 1);
        let message = &result.errors.entries()[0].message;
        assert!(message.contains("too many"), "{message}");
        assert!(progress.is_done());
    }

    fn resolve_root_len(root: &Path) -> usize {
        fs_meta::resolve_root(root).to_string_lossy().len()
    }
}
