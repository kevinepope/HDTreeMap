# Changelog

All notable changes to HDTreeMap are documented here.

## Unreleased

### Added
- M1 scanner and model: an arena tree of 40-byte nodes with both allocated and apparent
  sizes, size-mode switching without rescanning, an extension table, and subtree removal.
- Parallel filesystem scan (jwalk) on a worker thread: stays on one filesystem, counts hard
  links once, never follows symlinks or junctions, skips macOS's `/System/Volumes/Data`
  duplicate when scanning `/`, and lists unreadable folders instead of failing.
- Simulated drives for the web demo: a deterministic generator for six realistic drives
  (up to 1.8 million entries for the archive server) streamed at a believable pace.
- Scan screen with live counters, current folder, an estimated progress bar for drives and
  Cancel; results screen with totals, largest folders, top file types, unreadable items,
  a size-mode switch, Back and Rescan.
- Start-screen drive cards are now one big click target.
- `hdtreemap --demo [ID]` (the demo drives natively) and `hdtreemap --dump-top N PATH`
  (scan headless, print the largest folders and files, totals and timing).
- M0 scaffold: a workspace with `hdtreemap-core`, `hdtreemap-native` (the `hdtreemap`
  binary) and `hdtreemap-wasm`, plus the `demo/` page for GitHub Pages.
- Start screen listing mounted volumes with a used/free bar and sizes, and a
  "Choose Folder…" button with the native folder picker. A folder given on the command
  line opens straight on that target.
- `PlatformServices` (volumes, folder picker, reveal, open, trash) with a native
  implementation and a simulated `DemoPlatform` offering six descriptive demo drives.
- CI on Linux, macOS and Windows (fmt, clippy, tests), a wasm check, and a Pages deploy.
