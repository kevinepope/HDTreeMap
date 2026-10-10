# Changelog

All notable changes to HDTreeMap are documented here.

## Unreleased

### Added
- M5 live map. The results view opens as soon as a scan starts and the map, folder tree,
  legend and totals fill in while it runs, refreshed at most every 400 ms (slower when a
  refresh would take over 30% of the time between them). A strip under the toolbar shows
  the counters, the folder being read, a progress bar for drives, and Cancel. Hover,
  tooltips, selection and zoom work during the scan; Rescan and Move to Trash wait for
  it. When the scan finishes the same view shows the sorted tree, keeping the zoom and
  selection. The old scan screen is gone.
- Colour By (View menu, and a By Type / By Folder switch in the toolbar): colour the map
  by file type, or by the top-level folders of the zoom root, each with its own hue that
  everything inside it shares. By folder, the legend lists those folders; clicking one
  selects it.
- Remembered settings: size mode, cushions, colour scheme, splitter positions and the
  last five scanned folders and drives (a Recent list on the start screen), in
  `<config dir>/HDTreeMap/settings.json` (localStorage on the web). The native window
  size is remembered too. `HDTREEMAP_SETTINGS_DIR` moves the files.
- The web demo takes `?drive=<id>` (scan that simulated drive at once) and `?stats`
  (timing lines in the console).
- `HDTREEMAP_STATS` also reports how a scan read its metadata and its slowest folders;
  `examples/bench_live_scan` times a live scan against a blocking one;
  `HDTREEMAP_SCREENSHOT_SHOW=color-folder`.
- M4 actions. Right-click a file or folder on the map or in the folder tree for Reveal in
  Finder / Show in Explorer / Show in File Manager, Open, Copy Path, Zoom Here, Zoom to
  Parent, Rescan Folder and Move to Trash…. Moving to the trash asks first (name, size,
  item count), then removes the item from the map, tree, legend and totals; the scan root
  and names that cannot be represented exactly are refused. Rescan Folder runs in the
  background with progress in the status bar and splices the folder back in.
- A menu bar: File (Open Folder…, Scan Drive, Rescan, Back to Drives, Quit), Edit (Copy
  Path, Move to Trash…), View (Size Mode, Cushion Shading, Zoom In / Out / to Root, Clear
  Highlight), Help (About HDTreeMap, View on GitHub). It is the system menu bar on macOS
  and in the window elsewhere and on the web. Keyboard shortcuts for the common commands.
- Toasts in the corner of the map report what an action did; the web demo's actions say
  they are simulated (its Move to Trash removes the item from the simulated drive only).
- `HDTREEMAP_SCREENSHOT_SHOW=context-menu|trash|menu|about` for screenshots of UI that
  needs the mouse.
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

### Changed
- macOS: APFS clones are counted once. A file that may share blocks with another
  (`EF_MAY_SHARE_BLOCKS`) is looked up for its clone id and private size; the first
  member of a clone family counts in full, the others at their private size. Folder
  metadata is read in bulk (`getattrlistbulk`) instead of one `lstat` per entry:
  `~/Development` (1.05 M files) scans in 8.4 s instead of 9.7 s and 504 GB on disk
  instead of 527 GB; a whole-disk scan of `/` reports 681 GB instead of 955 GB (the data
  volume has 689 GB in use).
- The tree is now assembled on the UI thread from batches the scanning thread sends, and
  sorted a time slice at a time when the scan ends, so no frame stalls on a big tree (the
  web demo's longest frame on the 1.5 M-file archive server: 133 ms before, 50 ms now).
- The folder tree uses agg-gui's new TreeView API (events, programmatic selection and
  scrolling, glyph icons, sizes as right-aligned text with a share bar); selecting on the
  map is faster (8.7 ms from 12.4 ms on the archive server, software renderer).
- The scan root is canonicalised, and command-line paths are made absolute.
- Scanning `/` on macOS counts the data volume's own items (Spotlight, fseventsd, ...)
  that no firmlink reaches, reading `/usr/share/firmlinks`.
- Linux: mounts are judged from `/proc/self/mountinfo`, so btrfs subvolumes (`/home`) are
  scanned and listed as drives, and bind mounts are skipped instead of counted twice.

### Fixed
- The folder tree's root row showed the whole scanned path; it shows the drive's or the
  folder's own name, with the path as a tooltip.
- The web demo's start screen offered to choose a folder, which the demo cannot do.
- Scanning a single file gave it a doubled path (`/a/movie.mkv/movie.mkv`).
- Names that are not valid Unicode are flagged; actions refuse them.
- Long paths on Windows lost their allocated size; Show in Explorer now quotes the path.
- The results view could list volumes on the UI thread (a hang on a dead network mount).
- Overflow on bogus sizes in the layout and extension totals; name offsets past 4 GiB
  now stop the scan with an error instead of wrapping.
- The map raster is capped at 8192×8192 pixels in all (it could reach ~17 GB).
- A click right after a zoom could select a node from the old layout.
- A missing root is flagged as an error.
