//! The two small simulated drives:
//!
//! - "Small USB Stick": a few hundred files (movies, an installer ISO, trip photos, music,
//!   documents, a couple of backup zips) and the folders Windows and macOS leave behind.
//! - "Messy Downloads Folder": thousands of loose downloads: installers (`.dmg`, `.exe`,
//!   `.msi`, `.pkg`), archives (`.zip`, `.7z`, `.tar.gz`), PDFs, duplicate-named copies
//!   (`file (1).pdf`), half-finished downloads and odd extensions, plus a few folders of
//!   extracted archives and an "Old Downloads" dump.

use std::collections::HashSet;

use super::{
    dir, document_name, file, files, layout, locked, photo_name, shares, split, subdirs, unique,
    Entry, KB, OFFICE_EXTS,
};
use crate::demo_fs::rng::Rng;

/// USB stick top level.
pub fn usb_root(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("Movies", 50, usb_movies, 0),
            ("Installers", 25, usb_installers, 0),
            ("Photos", 8, usb_photos, 0),
            ("Music", 6, usb_music, 0),
            ("Documents", 3, usb_documents, 0),
            ("Backups", 8, usb_backups, 0),
        ],
    );
    out.push(locked("System Volume Information"));
    out.push(file(".DS_Store", 6 * KB));
    out.push(dir(".Trashes", 0, nothing, 0));
    out.push(file("autorun.inf", 1));
    out
}

fn nothing(_rng: &mut Rng, _bytes: u64, _arg: u32) -> Vec<Entry> {
    Vec::new()
}

fn usb_movies(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (5, 8), 0.4, |rng, _| {
        format!(
            "{} ({}).{}",
            rng.pick(&[
                "The Long Road",
                "Night Train",
                "Blue Planet",
                "Into the Woods",
                "Paper Towns",
                "Last Summer",
                "Old Friends"
            ]),
            rng.range(1995, 2023),
            rng.pick(&["mp4", "mkv", "avi"])
        )
    })
}

fn usb_installers(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[85, 15]);
    let mut out = vec![file("ubuntu-24.04-desktop-amd64.iso", parts[0])];
    out.extend(files(rng, parts[1], (3, 8), 1.0, |rng, _| {
        format!(
            "{}.{}",
            rng.pick(&[
                "VLCSetup",
                "FirefoxInstaller",
                "7z2401-x64",
                "drivers",
                "rufus-4.4",
                "balenaEtcher"
            ]),
            rng.pick(&["exe", "msi", "dmg"])
        )
    }));
    out
}

fn usb_photos(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 3, 0.4, trip, 0, |rng, _| {
        format!(
            "{} {}",
            rng.pick(&[
                "Lisbon",
                "Yosemite",
                "Tokyo",
                "Camping",
                "Grandma's 80th",
                "Road trip"
            ]),
            rng.range(2018, 2024)
        )
    })
}

fn trip(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let start = rng.range_usize(0, 9000);
    files(rng, bytes, (30, 70), 0.3, |_, i| {
        photo_name("IMG_", start + i, "JPG")
    })
}

fn usb_music(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 3, 0.3, usb_album, 0, |rng, _| {
        format!(
            "{} - {}",
            rng.pick(&[
                "Fleetwood Mac",
                "Nina Simone",
                "Arctic Monkeys",
                "Bob Marley",
                "ABBA"
            ]),
            rng.pick(&["Rumours", "Best Of", "AM", "Legend", "Gold"])
        )
    })
}

fn usb_album(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (10, 14), 0.2, |rng, i| {
        format!(
            "{:02} {}.mp3",
            i + 1,
            rng.pick(&[
                "Dreams",
                "Feeling Good",
                "Do I Wanna Know",
                "Three Little Birds",
                "Dancing Queen",
                "Go Your Own Way"
            ])
        )
    })
}

fn usb_documents(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (40, 80), 1.3, |rng, _| {
        document_name(rng, OFFICE_EXTS)
    })
}

fn usb_backups(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, 2, 0.3, |rng, i| {
        format!(
            "laptop-backup-{}-{:02}{}.zip",
            rng.range(2021, 2024),
            rng.range(1, 12),
            if i == 1 { "-old" } else { "" }
        )
    })
}

const DOWNLOAD_STEMS: &[&str] = &[
    "invoice",
    "statement",
    "boarding-pass",
    "ticket",
    "manual",
    "datasheet",
    "whitepaper",
    "slides",
    "syllabus",
    "lecture-notes",
    "lease-agreement",
    "tax-form",
    "w2",
    "receipt",
    "photo",
    "wallpaper",
    "screenshot",
    "scan",
    "export",
    "report",
    "resume",
    "template",
    "font-pack",
    "icons",
    "mockup",
    "dataset",
    "backup",
    "setup",
    "installer",
    "driver",
    "firmware",
    "patch",
    "update",
    "game",
    "mod",
    "sample-pack",
    "presets",
    "brushes",
    "ebook",
    "audiobook",
    "podcast-episode",
    "recording",
    "clip",
    "trailer",
    "concert",
];

/// What a download is, how common, and how big it tends to be (log-normal weight bias).
const DOWNLOAD_KINDS: &[(&str, u32)] = &[
    ("dmg", 10),
    ("exe", 10),
    ("msi", 5),
    ("pkg", 4),
    ("zip", 14),
    ("7z", 4),
    ("tar.gz", 4),
    ("rar", 2),
    ("iso", 2),
    ("pdf", 18),
    ("docx", 4),
    ("xlsx", 3),
    ("pptx", 2),
    ("epub", 2),
    ("mp4", 5),
    ("mkv", 2),
    ("mov", 2),
    ("mp3", 2),
    ("jpg", 6),
    ("png", 6),
    ("heic", 2),
    ("webp", 2),
    ("svg", 1),
    ("psd", 1),
    ("ai", 1),
    ("sketch", 1),
    ("fig", 1),
    ("stl", 2),
    ("gcode", 1),
    ("torrent", 3),
    ("part", 1),
    ("crdownload", 1),
    ("ics", 2),
    ("vcf", 1),
    ("webloc", 1),
    ("csv", 2),
    ("json", 1),
    ("apk", 1),
    ("deb", 1),
    ("AppImage", 1),
    ("jar", 1),
    ("bak", 1),
    ("tmp", 1),
    ("log", 1),
    ("srt", 1),
    ("ttf", 1),
    ("xyz", 1),
];

/// Typical size class (bytes multiplier) for an extension, so installers and videos are big
/// and calendar invites tiny.
fn heft(ext: &str) -> f64 {
    match ext {
        "iso" | "mkv" | "mov" | "mp4" => 400.0,
        "dmg" | "exe" | "msi" | "pkg" | "7z" | "rar" | "tar.gz" | "zip" | "AppImage" | "deb"
        | "part" | "crdownload" => 60.0,
        "apk" | "jar" | "psd" | "ai" | "sketch" | "fig" | "mp3" | "epub" | "stl" => 6.0,
        "pdf" | "pptx" | "jpg" | "png" | "heic" | "webp" | "gcode" | "docx" | "xlsx" | "ttf" => 1.0,
        _ => 0.02,
    }
}

/// Messy downloads top level.
pub fn downloads_root(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[70, 18, 8, 4]);
    let loose = rng.range(1400, 1800) as u32;
    let mut out = downloads(rng, parts[0], loose);
    out.push(dir("Old Downloads", parts[1], downloads_folder, 500));
    out.push(dir("Telegram Desktop", parts[2], downloads_folder, 150));
    let extracted = rng.range_usize(4, 8);
    let mut seen = HashSet::new();
    for size in split(rng, parts[3], extracted, 1.0) {
        let name = unique(
            &mut seen,
            format!(
                "{}-{}",
                rng.pick(DOWNLOAD_STEMS),
                rng.pick(&["main", "master", "v2.1", "extracted", "unzipped", "1.0.3"])
            ),
        );
        out.push(dir(name, size, extracted_archive, 0));
    }
    out.push(dir("New Folder", 0, nothing, 0));
    out.push(dir("New Folder (2)", 0, nothing, 0));
    out.push(file(".DS_Store", 18 * KB));
    out.push(file("desktop.ini", 282));
    out
}

fn downloads_folder(rng: &mut Rng, bytes: u64, count: u32) -> Vec<Entry> {
    downloads(rng, bytes, count)
}

/// `count` loose downloads sharing `bytes`, with duplicate copies ("x (1).pdf").
fn downloads(rng: &mut Rng, bytes: u64, count: u32) -> Vec<Entry> {
    let mut names: Vec<(String, f64)> = Vec::with_capacity(count as usize);
    let mut seen = HashSet::new();
    while names.len() < count as usize {
        let ext = *rng.weighted(DOWNLOAD_KINDS);
        let stem = rng.pick(DOWNLOAD_STEMS);
        let base = match rng.below(6) {
            0 => format!("{stem}-{}", rng.range(1, 9999)),
            1 => format!(
                "{stem}_{}{:02}{:02}",
                rng.range(2019, 2025),
                rng.range(1, 12),
                rng.range(1, 28)
            ),
            2 => format!("{stem} final"),
            3 => format!("{stem}_v{}", rng.range(1, 6)),
            4 => format!("{}-{}", stem, rng.hex(6)),
            _ => stem.to_string(),
        };
        let w = heft(ext) * (1.3 * rng.gaussian()).exp();
        let name = unique(&mut seen, format!("{base}.{ext}"));
        names.push((name, w));
        // Browsers number repeated downloads: "x (1).pdf", "x (2).pdf".
        if rng.chance(0.12) {
            for copy in 1..=rng.range(1, 3) {
                if names.len() >= count as usize {
                    break;
                }
                let dup = unique(&mut seen, format!("{base} ({copy}).{ext}"));
                names.push((dup, w));
            }
        }
    }
    let weights: Vec<u64> = names.iter().map(|(_, w)| (w * 1000.0) as u64 + 1).collect();
    names
        .into_iter()
        .zip(shares(bytes, &weights))
        .map(|((name, _), size)| file(name, size))
        .collect()
}

fn extracted_archive(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[60, 40]);
    let mut out = files(rng, parts[0], (4, 30), 1.4, |rng, _| {
        format!(
            "{}.{}",
            rng.pick(&["README", "LICENSE", "setup", "install", "data", "config", "main", "notes"]),
            rng.pick(&["txt", "md", "exe", "dll", "json", "ini", "py", "bin"])
        )
    });
    out.push(dir(
        *rng.pick(&["assets", "docs", "bin", "lib", "res"]),
        parts[1],
        extracted_inner,
        0,
    ));
    out
}

fn extracted_inner(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (5, 60), 1.4, |rng, i| {
        format!(
            "{}{i}.{}",
            rng.pick(&["file", "asset", "page", "img", "chunk"]),
            rng.pick(&["png", "dat", "html", "dll", "pak", "wav"])
        )
    })
}
