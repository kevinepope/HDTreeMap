# HDTreeMap

A small, fast disk-usage treemap viewer for macOS, Windows and Linux, written in Rust on
[agg-gui](https://github.com/larsbrubaker/agg-gui). Point it at a drive or folder and it
shows every file as a shaded rectangle whose area is proportional to its size, so the big
space hogs stand out at a glance.

**Live demo:** <https://larsbrubaker.github.io/HDTreeMap/> runs the app in your browser
against simulated drives. It never reads anything on your computer.

## Status

Early development. Milestones M0–M5 are done: pick a drive or folder and watch the map
fill in while the parallel scan runs, explore the result as a cushion treemap linked to
a folder tree and a legend, and act on what you find (reveal, open, copy the path,
rescan a folder, Move to Trash). Packaging and releases come next. See [docs/PLAN.md](docs/PLAN.md) for the full
plan.

## Features

- Parallel scan with a live map: the treemap, folder tree and legend fill in as the scan
  runs, with counters, progress and Cancel above them; you can hover and zoom
  meanwhile. Stays on one filesystem, doesn't follow symlinks, and counts hard links
  (and, on macOS, APFS clones) once. Unreadable folders are counted, not fatal. A whole
  Mac disk of 2.8 M files took 37 s on an Apple Silicon machine.
- Squarified treemap with cushion shading (or flat), coloured by file extension; folders
  too small to open and leftover small files show as grey blocks, so all space is shown.
- Hover for the path, size and share of each item (status bar and tooltip); click to
  select; double-click to zoom into a folder; Backspace, the mouse back button or the
  breadcrumb to zoom out.
- A folder tree beside the map, sorted by size with each folder's share, linked both ways:
  select or hover in either one and the other follows. Double-click a folder to zoom.
- An extension legend with the map's colours, sizes, shares and file counts for the
  current folder. Click an extension to highlight its files on the map.
- Colour the map by file type or by top-level folder (each folder's contents share its
  hue; the legend then lists the folders).
- Remembers the size mode, colours, shading, splitter positions, window size and the
  last five folders and drives you scanned (a Recent list on the start screen).
- "Size on disk" or apparent size, switchable without rescanning.
- Status bar with totals, unreadable items and free space.
- Right-click any file or folder (on the map or in the tree): Reveal in Finder / Show in
  Explorer / Show in File Manager, Open, Copy Path, Zoom Here, Zoom to Parent, Rescan
  Folder (in the background, spliced into the map) and Move to Trash… (confirmed first;
  never a permanent delete; the map, tree, legend and totals update).
- A menu bar (the system menu on macOS) with File, Edit, View and Help, and shortcuts:
  Cmd/Ctrl+O open a folder, Cmd/Ctrl+R rescan, Cmd/Ctrl+C copy the path,
  Cmd+Backspace (macOS) or Delete to move to the trash, Cmd/Ctrl+Up / Down to zoom out /
  in, Esc to clear the highlight or selection.
- Short notices in the corner of the map report what an action did.

## Known limitations

- **Hard links on Windows are counted once per link.** On macOS and Linux a file with
  several hard links is counted once (by device and inode). Windows only reports a
  file's link count and identity through an open handle (`GetFileInformationByHandle`);
  opening every file during the scan to find the few with more than one link would slow
  it down a lot, so a multiply-linked file (rare outside the WinSxS component store) is
  counted at each of its paths.
- **APFS clones that have since changed are counted at each copy.** macOS can clone a
  file so two copies share their blocks. HDTreeMap counts a family of clones once (the
  first copy it meets in full, the others at the bytes only they hold). A clone edited
  after cloning gets a clone id of its own, though, and APFS does not say which file it
  still shares blocks with, so its shared blocks are counted again. Space held only by
  Time Machine local snapshots, and purgeable space, is not shown, so a scan of `/`
  reads a little under the disk's used space.
- **Cloud files** (iCloud Drive, File Provider) that are not downloaded take no space and
  show as 0 bytes on disk. The scanner reads metadata only and never opens a file, so it
  never triggers a download; reading a folder that is itself only in the cloud may make
  macOS fetch its list of names.
- Names that are not valid Unicode are shown with replacement characters. Reveal, Open
  and Move to Trash refuse such items (and everything inside them), because the shown
  path is not the real one; use your file manager for them.

## Build and run

You need a Rust toolchain (1.87 or newer). The workspace expects
[agg-gui](https://github.com/larsbrubaker/agg-gui) checked out next to this repository
(`../agg-gui`), as it is in the `rust-apps` superproject:

```sh
git clone https://github.com/larsbrubaker/agg-gui.git ../agg-gui
```

Native app:

```sh
cargo run --release             # start screen
cargo run --release -- ~/Code   # open straight on a folder
cargo run --release -- --demo dev-workstation --highlight js   # a simulated drive
```

Web demo (needs [wasm-pack](https://rustwasm.github.io/wasm-pack/) and [bun](https://bun.sh/)):

```sh
wasm-pack build hdtreemap-wasm --target web --out-dir ../demo/public/pkg --no-typescript
cd demo
bun install
bun run dev
```

Checks (the same as CI): `scripts/pre-commit-check.sh` (or `.ps1` on Windows).

## License

MIT. See [LICENSE](LICENSE).
