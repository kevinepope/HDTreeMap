# HDTreeMap

[![HDTreeMap showing a nearly full laptop drive](readme_hero.png)](https://larsbrubaker.github.io/HDTreeMap/)

A small, fast disk-usage treemap viewer for macOS, Windows and Linux, written in Rust on
[agg-gui](https://github.com/larsbrubaker/agg-gui). Point it at a drive or folder and it
shows every file as a shaded rectangle whose area is proportional to its size, so the big
space hogs stand out at a glance. Then reveal, open or trash them from the same window.

**Live demo:** <https://larsbrubaker.github.io/HDTreeMap/> runs the real app in your
browser against six simulated drives (a nearly full laptop, a developer workstation, a
photo library, a USB stick, a messy Downloads folder and a 1.5-million-file archive
server). It never reads anything on your computer.

**Download:** prebuilt apps for macOS (universal), Windows and Linux are on the
[Releases page](https://github.com/larsbrubaker/HDTreeMap/releases).

## Features

- **Fast parallel scan with a live map.** The treemap, folder tree and legend fill in as
  the scan runs, with counters, progress and Cancel above them; you can hover and zoom
  meanwhile. A whole Mac disk of 2.8 M files took 37 s on an Apple Silicon machine.
- **Correct totals.** Stays on one filesystem, doesn't follow symlinks, and counts hard
  links (and, on macOS, APFS clones) once. Unreadable folders are counted, not fatal.
  "Size on disk" or apparent size, switchable without rescanning.
- **Squarified cushion treemap** (or flat blocks, bevelled tiles, or a cityscape whose
  buildings rise with file size), coloured by file type or by top-level folder; folders
  too small to open and leftover small files show as grey blocks, so all space is shown.
- **Three linked views.** A folder tree beside the map, sorted by size with each folder's
  share, and a legend of file types with sizes, shares and file counts. Select or hover in
  one and the others follow; click a file type to highlight its files on the map.
- **Explore.** Hover for the path, size and share of each item; click to select;
  double-click to zoom into a folder; Backspace, the mouse back button or the breadcrumb
  to zoom out.
- **Act on what you find.** Right-click any file or folder: Reveal in Finder / Show in
  Explorer / Show in File Manager, Open, Copy Path, Zoom Here, Zoom to Parent, Rescan
  Folder (in the background, spliced into the map) and Move to Trash (confirmed first;
  never a permanent delete). Short notices in the corner say what happened.
- **Light or dark.** Light by default; switch to dark, or follow the system, from
  View ▸ Theme or the sun/moon button in the toolbar. It changes at once, no restart.
- **Native feel.** The system menu bar on macOS (with shortcuts), an in-window menu bar
  elsewhere; remembers the size mode, colours, shading, theme, splitter positions, window
  size and the last five folders and drives you scanned.

## Install

Download the package for your system from the
[latest release](https://github.com/larsbrubaker/HDTreeMap/releases/latest):

- **macOS** (Apple Silicon and Intel): `HDTreeMap-<version>-macos-universal.zip`. Unzip it
  and move `HDTreeMap.app` to Applications. The app is not signed with an Apple
  Developer ID, so the first time macOS refuses to open it: right-click (or
  Control-click) the app and choose **Open**, then **Open** again. On macOS 15 and later,
  if that offers no Open button, open **System Settings ▸ Privacy & Security** and click
  **Open Anyway** next to the message about HDTreeMap (or run
  `xattr -dr com.apple.quarantine /Applications/HDTreeMap.app`). To measure folders such
  as `~/Library` and Mail, give it **Full Disk Access** in the same settings pane.
- **Windows** (x86-64): `hdtreemap-<version>-windows-x86_64.zip`. Unzip and run
  `hdtreemap.exe`. SmartScreen may warn about an unrecognised app: choose **More info ▸
  Run anyway**.
- **Linux** (x86-64): `hdtreemap-<version>-linux-x86_64.tar.gz`. Unpack and run
  `./hdtreemap`. It needs a desktop with Vulkan or OpenGL drivers; the folder picker uses
  the desktop portal (`xdg-desktop-portal`).

## Usage

Start it and pick a drive from the list (or **Choose Folder…**), or pass a folder on the
command line:

```sh
hdtreemap                    # start screen
hdtreemap ~/Code             # open straight on a folder
hdtreemap --demo laptop-ssd  # one of the web demo's simulated drives
hdtreemap --help             # every option
```

| Action | macOS | Windows / Linux |
|---|---|---|
| Open a folder | Cmd+O | Ctrl+O |
| Rescan | Cmd+R | Ctrl+R |
| Copy the selected path | Cmd+C | Ctrl+C |
| Move the selection to the Trash | Cmd+Backspace | Delete |
| Zoom into the selected folder | Cmd+Down, or double-click | Ctrl+Down, or double-click |
| Zoom out | Cmd+Up, Backspace, mouse back | Ctrl+Up, Backspace, mouse back |
| Clear the highlight or selection | Esc | Esc |
| Context menu | right-click | right-click |
| Quit | Cmd+Q | Ctrl+Q |

## How it works

- **Scanning.** A parallel directory walk ([jwalk](https://crates.io/crates/jwalk) on a
  rayon pool, the walker of [dua-cli](https://github.com/Byron/dua-cli)) reads metadata
  only, never file contents, and streams records to the UI thread, which assembles them
  into a compact arena tree (40 bytes a node) and shows the partial tree as it grows. On
  macOS it reads folders in bulk with `getattrlistbulk` and accounts for APFS clones.
- **Layout.** The squarified treemap algorithm of Bruls, Huizing and van Wijk
  (*Squarified Treemaps*, 2000) keeps rectangles close to square, so sizes are easy to
  compare.
- **Shading.** Cushion treemaps (van Wijk and van de Wetering, *Cushion Treemaps:
  Visualization of Hierarchical Information*, 1999): each level of folders adds a
  parabolic ridge, and per-pixel lighting of the summed surface makes the nesting visible
  without borders. The rasteriser runs in parallel row bands natively and serially in the
  browser.
- **UI.** Everything on screen is drawn by agg-gui, a Rust port of
  [Anti-Grain Geometry](https://agg.sourceforge.net/antigrain.com/) with a widget toolkit
  on top; the same code runs natively (winit + wgpu) and in the browser (WebAssembly).

HDTreeMap is inspired by [WinDirStat](https://windirstat.net/),
[QDirStat](https://github.com/shundhammer/qdirstat) (and KDirStat before them) and
[GrandPerspective](https://grandperspectiv.sourceforge.net/): the linked tree, map and
legend, the cushion shading, and the idea that a picture of your disk beats a list. It
borrows no code from them. See [docs/PLAN.md](docs/PLAN.md) for the design.

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
- No duplicate finder, saved scans or sunburst view, and it doesn't read the NTFS MFT
  directly (as WizTree does), so a Windows scan is as fast as a parallel walk allows.
- The release builds are not code-signed or notarized (see Install).

## Build from source

You need a Rust toolchain (1.95 or newer). The workspace expects
[agg-gui](https://github.com/larsbrubaker/agg-gui) checked out next to this repository
(`../agg-gui`), as it is in the `rust-apps` superproject:

```sh
git clone https://github.com/larsbrubaker/HDTreeMap.git
git clone https://github.com/larsbrubaker/agg-gui.git
cd HDTreeMap
cargo run --release             # start screen
cargo run --release -- ~/Code   # open straight on a folder
```

On macOS you can double-click `run-mac.command` (or run `./run-mac.command [PATH | --demo ID]`);
on Windows, double-click `run-windows.cmd` (or run `run-windows.cmd [PATH | --demo ID]`).

Web demo (needs [wasm-pack](https://rustwasm.github.io/wasm-pack/) and [bun](https://bun.sh/)):

```sh
wasm-pack build hdtreemap-wasm --target web --out-dir ../demo/public/pkg --no-typescript
cd demo
bun install
bun run dev
```

Packages: `scripts/package-macos.sh` builds the universal `HDTreeMap.app` and its zip;
the [release workflow](.github/workflows/release.yml) builds all three platforms. The app
icon is drawn by the app's own treemap renderer
(`cargo run -p hdtreemap-core --example make_icons` regenerates `assets/icon/`).

Checks (the same as CI): `scripts/pre-commit-check.sh` (or `.ps1` on Windows).

## License

MIT. See [LICENSE](LICENSE).
