//! `--dump-top N`: scan without a window and print the largest folders and files, the
//! totals and the timing. A debugging aid for the scanner (`docs/PLAN.md` M1): it runs the
//! same `FsScanner` / `DemoScanner` the app uses, with live progress on stderr.

use std::fmt::Write as _;
use std::io::{IsTerminal, Write};
use std::time::Duration;

use hdtreemap_core::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use hdtreemap_core::format::{format_bytes, format_count, format_duration, format_percent};
use hdtreemap_core::scan::{FsScanner, ScanPoll, ScanResult, ScanSource};

use crate::cli::DumpSource;

/// How many unreadable paths the report lists.
const ERRORS_LISTED: usize = 10;

/// Run the scan and print the report. `Err` explains a bad drive id.
pub fn run(top: usize, source: &DumpSource) -> Result<(), String> {
    let mut scan: Box<dyn ScanSource> = match source {
        DumpSource::Path(path) => Box::new(FsScanner::start(path.clone(), None)),
        DumpSource::Demo(id) => {
            let profile =
                DemoProfile::from_id(id).ok_or_else(|| format!("unknown demo drive {id:?}"))?;
            Box::new(DemoScanner::new(profile, DemoPace::Unpaced))
        }
    };
    let progress = scan.progress();
    let live = std::io::stderr().is_terminal();
    let result = loop {
        match scan.poll(Duration::from_millis(100)) {
            ScanPoll::Finished(result) => break result,
            ScanPoll::Running => {
                if live {
                    let s = progress.snapshot();
                    eprint!(
                        "\r{} files, {} folders, {} \u{2026}   ",
                        format_count(s.files),
                        format_count(s.dirs),
                        format_bytes(s.bytes)
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    };
    if live {
        eprint!("\r{:60}\r", "");
    }
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(report(&result, top).as_bytes());
    Ok(())
}

/// The printed report for `result`, listing `top` folders and files.
pub fn report(result: &ScanResult, top: usize) -> String {
    let tree = &result.tree;
    let total = tree.total();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "Scanned {} in {}",
        result.root.display(),
        format_duration(result.elapsed)
    );
    let _ = writeln!(
        s,
        "  {} files, {} folders, {} unreadable",
        format_count(tree.file_count()),
        format_count(tree.dir_count()),
        format_count(result.errors.total())
    );
    let _ = writeln!(
        s,
        "  {} on disk, {} apparent",
        format_bytes(total.allocated),
        format_bytes(total.apparent)
    );
    let root_size = tree.size(tree.root());
    let _ = writeln!(s, "\nLargest folders:");
    for id in tree.largest_folders(top) {
        let size = tree.size(id);
        let _ = writeln!(
            s,
            "  {:>10}  {:>6}  {}",
            format_bytes(size),
            format_percent(size, root_size),
            tree.relative_path(id)
        );
    }
    let _ = writeln!(s, "\nLargest files:");
    for id in tree.largest_files(top) {
        let size = tree.size(id);
        let _ = writeln!(
            s,
            "  {:>10}  {:>6}  {}",
            format_bytes(size),
            format_percent(size, root_size),
            tree.relative_path(id)
        );
    }
    if !result.errors.is_empty() {
        let _ = writeln!(s, "\nUnreadable ({}):", format_count(result.errors.total()));
        for error in result.errors.entries().iter().take(ERRORS_LISTED) {
            let _ = writeln!(s, "  {}: {}", error.path.display(), error.message);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_lists_totals_folders_files_and_errors() {
        let result = DemoScanner::new(DemoProfile::UsbStick, DemoPace::Unpaced)
            .run_to_end()
            .expect("result");
        let text = report(&result, 3);
        assert!(text.starts_with("Scanned /demo/usb-stick in "));
        assert!(text.contains("Largest folders:\n"));
        assert!(text.contains("Largest files:\n"));
        assert!(text.contains("Unreadable (1):"));
        assert!(text.contains("System Volume Information"));
        let folder_lines = text
            .split("Largest folders:\n")
            .nth(1)
            .and_then(|rest| rest.split("\n\n").next())
            .map_or(0, |block| block.lines().count());
        assert_eq!(folder_lines, 3);
    }

    #[test]
    fn an_unknown_demo_drive_is_an_error() {
        assert!(run(3, &DumpSource::Demo("floppy".into())).is_err());
    }
}
