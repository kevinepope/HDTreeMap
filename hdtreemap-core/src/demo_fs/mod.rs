//! `demo_fs`: the simulated drives' file trees, and the scan source that streams them.
//!
//! Each simulated drive on the web demo's start screen (`platform::DemoPlatform`) has a
//! profile id; [`DemoProfile`] maps it to a recipe (`recipes/`) that describes a realistic
//! tree for that kind of disk, sized to the drive's advertised used space. [`DemoScanner`]
//! is the [`ScanSource`] that generates it, deterministically from a fixed seed, through the
//! same `TreeBuilder` the real scanner uses, in time-sliced chunks so the single-threaded
//! browser keeps drawing frames, and at a believable pace so the scan screen is worth
//! watching. Natively it backs `--demo <id>` and the tests.

mod engine;
pub mod recipes;
pub mod rng;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use web_time::Instant;

pub use engine::{Entry, Expand, Generator};

use crate::model::Tree;
use crate::platform::{profile, DemoPlatform, PlatformServices};
use crate::scan::{ScanPoll, ScanProgress, ScanResult, ScanSource};

/// The six simulated drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DemoProfile {
    LaptopSsd,
    DevWorkstation,
    MediaLibrary,
    UsbStick,
    MessyDownloads,
    ArchiveServer,
}

impl DemoProfile {
    pub const ALL: [DemoProfile; 6] = [
        DemoProfile::LaptopSsd,
        DemoProfile::DevWorkstation,
        DemoProfile::MediaLibrary,
        DemoProfile::UsbStick,
        DemoProfile::MessyDownloads,
        DemoProfile::ArchiveServer,
    ];

    /// The drive's profile id (`VolumeInfo::id` on the demo platform).
    pub fn id(self) -> &'static str {
        match self {
            DemoProfile::LaptopSsd => profile::LAPTOP_SSD,
            DemoProfile::DevWorkstation => profile::DEV_WORKSTATION,
            DemoProfile::MediaLibrary => profile::MEDIA_LIBRARY,
            DemoProfile::UsbStick => profile::USB_STICK,
            DemoProfile::MessyDownloads => profile::MESSY_DOWNLOADS,
            DemoProfile::ArchiveServer => profile::ARCHIVE_SERVER,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The used space the demo platform advertises for this drive: what the tree adds up to.
    pub fn used_bytes(self) -> u64 {
        DemoPlatform
            .list_volumes()
            .iter()
            .find(|v| v.id == self.id())
            .map_or(0, |v| v.used_bytes())
    }

    /// Where the drive is "mounted" (the tree's root name).
    pub fn mount_path(self) -> PathBuf {
        DemoPlatform::mount_path(self.id())
    }

    /// Fixed seed: every visitor sees the same tree.
    fn seed(self) -> u64 {
        match self {
            DemoProfile::LaptopSsd => 0x1A97_0B55_D001,
            DemoProfile::DevWorkstation => 0xDE5C_0DE5_0002,
            DemoProfile::MediaLibrary => 0x9407_05F1_0003,
            DemoProfile::UsbStick => 0x05B5_71C4_0004,
            DemoProfile::MessyDownloads => 0xD0E1_0AD5_0005,
            DemoProfile::ArchiveServer => 0xA4C1_1FE5_0006,
        }
    }

    fn recipe(self) -> Expand {
        match self {
            DemoProfile::LaptopSsd => recipes::laptop::root,
            DemoProfile::DevWorkstation => recipes::workstation::root,
            DemoProfile::MediaLibrary => recipes::media::root,
            DemoProfile::UsbStick => recipes::small::usb_root,
            DemoProfile::MessyDownloads => recipes::small::downloads_root,
            DemoProfile::ArchiveServer => recipes::archive::root,
        }
    }

    /// About how many entries the tree has (measured; checked by a test), for pacing.
    pub fn approx_entries(self) -> u64 {
        match self {
            DemoProfile::LaptopSsd => 205_000,
            DemoProfile::DevWorkstation => 530_000,
            DemoProfile::MediaLibrary => 129_000,
            DemoProfile::UsbStick => 300,
            DemoProfile::MessyDownloads => 2_600,
            DemoProfile::ArchiveServer => 1_775_000,
        }
    }

    /// How long the paced scan takes.
    pub fn scan_duration(self) -> Duration {
        Duration::from_millis(match self {
            DemoProfile::UsbStick => 1_500,
            DemoProfile::MessyDownloads => 2_500,
            DemoProfile::MediaLibrary => 4_000,
            DemoProfile::LaptopSsd => 5_000,
            DemoProfile::DevWorkstation => 6_000,
            DemoProfile::ArchiveServer => 20_000,
        })
    }
}

/// How fast a [`DemoScanner`] goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoPace {
    /// About [`DemoProfile::scan_duration`]: the scan screen shows real progress.
    Realistic,
    /// As fast as the time budget per poll allows (tests, `--dump-top`).
    Unpaced,
}

/// Entries generated per slice between clock checks.
const CHUNK: u64 = 2048;

/// The demo scan source. See the module docs.
pub struct DemoScanner {
    profile: DemoProfile,
    generator: Generator,
    progress: Arc<ScanProgress>,
    /// Entries per second, when paced.
    rate: Option<f64>,
    emitted: u64,
    started: Instant,
    finished: bool,
}

impl DemoScanner {
    pub fn new(profile: DemoProfile, pace: DemoPace) -> Self {
        let root = profile.mount_path();
        let generator = Generator::new(
            &root.to_string_lossy(),
            profile.seed(),
            profile.used_bytes(),
            profile.recipe(),
        );
        let rate = match pace {
            DemoPace::Realistic => {
                Some(profile.approx_entries() as f64 / profile.scan_duration().as_secs_f64())
            }
            DemoPace::Unpaced => None,
        };
        Self {
            profile,
            generator,
            progress: Arc::new(ScanProgress::new()),
            rate,
            emitted: 0,
            started: Instant::now(),
            finished: false,
        }
    }

    pub fn profile(&self) -> DemoProfile {
        self.profile
    }

    /// Generate up to `max_entries` more entries, ignoring the pace. Returns `true` when the
    /// tree is complete. The time-slicing primitive behind [`ScanSource::poll`].
    pub fn step(&mut self, max_entries: u64) -> bool {
        let done = self.generator.step(max_entries, &self.progress);
        self.emitted = self.emitted.saturating_add(max_entries);
        done
    }

    /// The tree generated so far (partial totals), until the result is taken.
    pub fn partial_tree(&self) -> Option<&Tree> {
        self.generator.tree()
    }

    /// The finished scan, once [`Self::step`] has reported completion (once).
    pub fn take_result(&mut self) -> Option<ScanResult> {
        if !self.generator.is_done() {
            return None;
        }
        let (tree, errors) = self.generator.take()?;
        self.progress.mark_done();
        Some(ScanResult {
            root: self.profile.mount_path(),
            tree,
            errors,
            elapsed: self.started.elapsed(),
        })
    }

    /// Generate everything now and return the result (`None` if it was already taken).
    pub fn run_to_end(&mut self) -> Option<ScanResult> {
        while !self.step(u64::MAX) {}
        self.finished = true;
        self.take_result()
    }
}

impl ScanSource for DemoScanner {
    fn progress(&self) -> Arc<ScanProgress> {
        Arc::clone(&self.progress)
    }

    fn poll(&mut self, budget: Duration) -> ScanPoll {
        if self.finished || self.progress.is_cancelled() {
            return ScanPoll::Running;
        }
        let deadline = Instant::now().checked_add(budget);
        loop {
            let allowed = match self.rate {
                Some(rate) => {
                    let due = (self.started.elapsed().as_secs_f64() * rate) as u64;
                    due.saturating_sub(self.emitted).min(CHUNK)
                }
                None => CHUNK,
            };
            if allowed == 0 {
                return ScanPoll::Running;
            }
            if self.step(allowed) {
                self.finished = true;
                return match self.take_result() {
                    Some(result) => ScanPoll::Finished(Box::new(result)),
                    None => ScanPoll::Running,
                };
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return ScanPoll::Running;
            }
        }
    }
}

#[cfg(test)]
mod tests;
