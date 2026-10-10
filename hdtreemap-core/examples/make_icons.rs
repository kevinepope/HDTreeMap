//! Writes the app icon assets from `hdtreemap_core::icon` (the app's own treemap renderer)
//! into `assets/icon/`, for the release packages and the README:
//!
//! - `hdtreemap-1024.png` (macOS grid, transparent margin) and `hdtreemap-{16..512}.png`
//!   (tight, for Linux and the web);
//! - `hdtreemap.ico` (Windows: 16–256 px, PNG-compressed entries);
//! - `hdtreemap.icns` (macOS: 16–1024 px, PNG entries).
//!
//! ```sh
//! cargo run --release -p hdtreemap-core --example make_icons [-- OUT_DIR]
//! ```
//!
//! Each size is rendered directly (not scaled down), so small icons stay crisp. The
//! outputs are committed; rerun this after changing the icon.

use std::path::{Path, PathBuf};

use agg_gui::screenshot::encode_png_rgba;
use hdtreemap_core::icon::{rgba, MAC_MARGIN, TIGHT_MARGIN};

/// A PNG of the icon at `size` with `margin`.
fn png(size: u32, margin: f64) -> Vec<u8> {
    encode_png_rgba(&rgba(size, margin), size, size).expect("encode icon PNG")
}

/// A Windows `.ico` holding a PNG of each size in `sizes` (all at most 256).
fn ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = sizes.iter().map(|&s| (s, png(s, TIGHT_MARGIN))).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (size, data) in &images {
        // 256 is written as 0.
        let side = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in &images {
        out.extend_from_slice(data);
    }
    out
}

/// A macOS `.icns` with a PNG entry of each (type, size).
fn icns(entries: &[(&[u8; 4], u32)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (kind, size) in entries {
        let data = png(*size, MAC_MARGIN);
        body.extend_from_slice(*kind);
        body.extend_from_slice(&(data.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(&data);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

fn write(dir: &Path, name: &str, data: &[u8]) {
    let path = dir.join(name);
    std::fs::write(&path, data).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!("wrote {} ({} bytes)", path.display(), data.len());
}

fn main() {
    let dir = std::env::args().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/icon"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&dir).expect("create the output folder");
    write(&dir, "hdtreemap-1024.png", &png(1024, MAC_MARGIN));
    for size in [16, 32, 48, 64, 128, 256, 512] {
        write(
            &dir,
            &format!("hdtreemap-{size}.png"),
            &png(size, TIGHT_MARGIN),
        );
    }
    write(&dir, "hdtreemap.ico", &ico(&[16, 24, 32, 48, 64, 128, 256]));
    write(
        &dir,
        "hdtreemap.icns",
        &icns(&[
            (b"icp4", 16),
            (b"icp5", 32),
            (b"icp6", 64),
            (b"ic07", 128),
            (b"ic08", 256),
            (b"ic09", 512),
            (b"ic10", 1024),
            (b"ic11", 32),
            (b"ic12", 64),
            (b"ic13", 256),
            (b"ic14", 512),
        ]),
    );
}
