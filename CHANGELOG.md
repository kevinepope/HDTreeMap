# Changelog

All notable changes to HDTreeMap are documented here.

## Unreleased

### Added
- M2 treemap: finished scans open straight on a squarified treemap with cushion shading
  (van Wijk), coloured by the 16 biggest extensions. Sub-pixel items merge into
  aggregate blocks so every byte keeps its area. Hover outlines the item and its folder,
  shows a tooltip and fills the status bar. Click selects and Escape clears. Double click
  zooms into a folder; Backspace, the mouse back button or the breadcrumb zooms out. The
  toolbar has the size mode, a cushion on/off toggle and Rescan. The status bar shows
  totals, unreadable items (with a Full Disk Access hint on macOS) and free space. The map
  is laid out and rasterised (in parallel natively) only when its size, zoom, tree or
  shading changes; hovering never re-rasterises.
- `HDTREEMAP_STATS=1` prints layout, raster and whole-frame times;
  `cargo run --release -p hdtreemap-core --example bench_treemap -- [DEMO_ID | PATH] [WxH]`
  times the treemap headless.
- M1 scanner and model: an arena tree of 40-byte nodes with both allocated and apparent
  sizes, size-mode switching without rescanning, an extension table, and subtree removal.
- Parallel filesystem scan (jwalk) on a worker thread: stays on one filesystem, counts hard
  links once, never follows symlinks or junctions, skips macOS's `/System/Volumes/Data`
  duplicate when scanning `/`, and lists unreadable folders instead of failing.
- Simulated drives for the web demo: a deterministic generator for six realistic drives
  (up to 1.8 million entries for the archive server) streamed at a believable pace.
- Scan screen with live counters, current folder (elided in the middle), an estimated
  progress bar for drives and Cancel. (M1's interim results screen was replaced by the
  treemap view in M2.)
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
