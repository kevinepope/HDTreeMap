# Changelog

All notable changes to HDTreeMap are documented here.

## Unreleased

### Added
- M3 linked views. The results view puts a folder tree over an extension legend on the
  left (a splitter at 28% of the width, kept for the session) beside the map.
  - Folder tree: Font Awesome icons, name, size and share of the parent folder, children
    by size. Rows are added only when a folder opens, at most 200 per folder plus an
    "N more" row, so million-file scans stay fast. Selecting or hovering a row does the
    same on the map. Selecting on the map opens the tree down to the item and scrolls to
    it. Double click or Enter on a folder zooms the map. The zoom path opens by default.
  - Extension legend: colour swatch (the map's own colours), extension, size, share and
    file count for the zoom root, sorted by size. Click a header to sort by that column
    (again to reverse). Click a row to dim everything on the map but that extension's
    files; click it again or press Escape to clear. The status bar says what is
    highlighted ("Highlighting .mov: 37 files, 120 GB").
- The map's tooltip has one part per line again, and each rectangle gets its own hover
  delay.
- `hdtreemap --highlight EXT` highlights an extension once the scan finishes.
- `cargo run --release -p hdtreemap-core --example bench_views` times whole frames of the
  results screen headless (hover, select, highlight).
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
