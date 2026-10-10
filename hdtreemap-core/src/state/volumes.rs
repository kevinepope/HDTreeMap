//! The mounted volumes as last listed, cached in `AppState` so the results view can show
//! the free space of the scanned volume without asking the OS again on the UI thread
//! (listing volumes can hang for a long time on a dead network mount).
//!
//! The list arrives two ways: the start screen lists volumes anyway and stores what it got
//! ([`AppState::set_volumes`]), and a shell that starts straight on a scan (a folder given
//! on the command line) lists them on a background thread and hands over the receiving end
//! ([`AppState::receive_volumes`]). Readers poll the receiver on the UI thread; nothing
//! here blocks.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};

use super::AppState;
use crate::platform::{volume_containing, VolumeInfo};

/// See the module docs.
#[derive(Default)]
pub(super) struct VolumeCache {
    list: RefCell<Option<Vec<VolumeInfo>>>,
    feed: RefCell<Option<Receiver<Vec<VolumeInfo>>>>,
    /// Bumped whenever a new list arrives.
    generation: Cell<u64>,
}

impl AppState {
    fn volume_cache(&self) -> &VolumeCache {
        &self.inner.volumes
    }

    /// Remember `volumes` (as just listed) for later free-space lookups.
    pub fn set_volumes(&self, volumes: Vec<VolumeInfo>) {
        let cache = self.volume_cache();
        *cache.list.borrow_mut() = Some(volumes);
        cache.generation.set(cache.generation.get().wrapping_add(1));
        agg_gui::animation::request_draw();
    }

    /// Take the volume list from `feed` when it arrives (sent by a background lookup).
    pub fn receive_volumes(&self, feed: Receiver<Vec<VolumeInfo>>) {
        *self.volume_cache().feed.borrow_mut() = Some(feed);
        self.poll_volumes();
    }

    /// Take a list waiting in the feed, if any.
    fn poll_volumes(&self) {
        let received = {
            let mut feed = self.volume_cache().feed.borrow_mut();
            match feed.as_ref().map(Receiver::try_recv) {
                Some(Ok(list)) => {
                    *feed = None;
                    Some(list)
                }
                Some(Err(TryRecvError::Disconnected)) => {
                    *feed = None;
                    None
                }
                Some(Err(TryRecvError::Empty)) | None => None,
            }
        };
        if let Some(list) = received {
            self.set_volumes(list);
        }
    }

    /// Changes when a new volume list arrives; cheap to poll each layout.
    pub fn volumes_version(&self) -> u64 {
        self.poll_volumes();
        self.volume_cache().generation.get()
    }

    /// The cached volume list (`None` until one was listed).
    pub fn volumes(&self) -> Option<Vec<VolumeInfo>> {
        self.poll_volumes();
        self.volume_cache().list.borrow().clone()
    }

    /// Free bytes on the cached volume holding `path`.
    pub fn free_space_for(&self, path: &Path) -> Option<u64> {
        self.poll_volumes();
        let list = self.volume_cache().list.borrow();
        volume_containing(list.as_deref()?, path).map(|v| v.free_bytes)
    }
}
