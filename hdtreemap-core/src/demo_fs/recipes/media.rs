//! "Photo & Video Library": years of photos in year / month folders (`.jpg`, `.heic`, and
//! camera raws `.cr2` / `.dng`), a Lightroom catalog with its previews, a music collection,
//! and a handful of huge `.mov` / `.mp4` files that dominate the map.

use super::{dir, file, files, layout, named_files, photo_name, subdirs, Entry, KB, MONTHS};
use crate::demo_fs::rng::Rng;

/// First and last year of the photo archive.
const FIRST_YEAR: u32 = 2009;
const LAST_YEAR: u32 = 2024;

/// The drive's top level.
pub fn root(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("Photos", 470, photos, 0),
            ("Videos", 400, videos, 0),
            ("Lightroom", 70, lightroom, 0),
            ("Music", 45, music, 0),
            ("Projects", 15, projects, 0),
        ],
    );
    out.push(file(".DS_Store", 12 * KB));
    out
}

fn photos(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    // Later years have more (and bigger) photos.
    let years: Vec<u32> = (FIRST_YEAR..=LAST_YEAR).collect();
    let weights: Vec<u64> = years
        .iter()
        .map(|y| u64::from(y - FIRST_YEAR + 3))
        .collect();
    let sizes = super::shares(bytes, &weights);
    let mut out: Vec<Entry> = years
        .iter()
        .zip(sizes)
        .map(|(&year, size)| dir(year.to_string(), size, photo_year, year))
        .collect();
    out.push(dir(
        "Imports",
        rng.range(1, 40) * 1024 * KB,
        loose_photos,
        0,
    ));
    out
}

fn photo_year(rng: &mut Rng, bytes: u64, year: u32) -> Vec<Entry> {
    let sizes = super::split(rng, bytes, 12, 0.5);
    sizes
        .into_iter()
        .enumerate()
        .map(|(m, size)| {
            dir(
                format!("{:02} - {}", m + 1, MONTHS[m]),
                size,
                photo_month,
                year,
            )
        })
        .collect()
}

/// One month of photos: phone pictures (HEIC from 2017), camera JPEGs and raws, a few clips.
fn photo_month(rng: &mut Rng, bytes: u64, year: u32) -> Vec<Entry> {
    let n = rng.range_usize(250, 800);
    let mut counter = rng.range_usize(0, 9000);
    let raw_month = rng.chance(0.3);
    files(rng, bytes, n, 0.6, |rng, _| {
        counter += 1;
        let roll = rng.below(100);
        if raw_month && roll < 40 {
            let ext = if year >= 2016 { "CR3" } else { "CR2" };
            photo_name("_MG_", counter, ext)
        } else if roll < 45 && year >= 2017 {
            photo_name("IMG_", counter, "HEIC")
        } else if roll < 48 {
            photo_name("IMG_", counter, "MOV")
        } else if roll < 52 {
            photo_name("DSC", counter, "DNG")
        } else {
            photo_name("IMG_", counter, "JPG")
        }
    })
}

fn loose_photos(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (20, 80), 0.6, |rng, i| {
        photo_name("IMG_", i + rng.range_usize(0, 5000), "JPG")
    })
}

fn videos(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = super::shares(bytes, &[70, 30]);
    // A few huge masters at the top level...
    let mut out = files(rng, parts[0], (6, 10), 0.5, |rng, _| {
        format!(
            "{} {} (ProRes master).{}",
            rng.pick(&[
                "Wedding",
                "Iceland trip",
                "Graduation",
                "Drone footage",
                "Family reunion",
                "Band concert",
                "Short film"
            ]),
            rng.range(2016, 2024),
            rng.pick(&["mov", "mp4"])
        )
    });
    // ...and folders of ordinary clips.
    out.extend(subdirs(rng, parts[1], 5, 0.7, video_folder, 0, |rng, _| {
        (*rng.pick(&[
            "Family",
            "Travel",
            "Drone",
            "GoPro",
            "Screen recordings",
            "Events",
            "Timelapse",
        ]))
        .to_string()
    }));
    out
}

fn video_folder(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (30, 160), 1.4, |rng, i| {
        let ext = rng.weighted(&[("mp4", 5), ("MOV", 4), ("mkv", 1), ("MTS", 1)]);
        format!(
            "{}_{:04}.{ext}",
            rng.pick(&["GX01", "DJI", "VID", "MVI", "clip"]),
            i + 1
        )
    })
}

fn lightroom(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = super::shares(bytes, &[80, 15, 5]);
    let mut out = vec![
        dir("Lightroom Catalog Previews.lrdata", parts[0], previews, 0),
        dir("Backups", parts[1], lr_backups, 0),
    ];
    out.extend(named_files(
        parts[2],
        &[
            ("Lightroom Catalog.lrcat", 95),
            ("Lightroom Catalog.lrcat-data", 5),
        ],
    ));
    out
}

fn previews(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = subdirs(rng, bytes, 16, 0.2, preview_bucket, 0, |_, i| {
        format!("{i:X}")
    });
    out.push(file("previews.db", 40 * 1024 * KB));
    out
}

fn preview_bucket(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 16, 0.3, preview_leaf, 0, |_, i| {
        format!("{i:04X}")
    })
}

fn preview_leaf(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (20, 120), 0.5, |rng, _| {
        format!(
            "{}-{}-{}-{}-{}-{}.lrprev",
            rng.hex(8),
            rng.hex(4),
            rng.hex(4),
            rng.hex(4),
            rng.hex(12),
            rng.hex(8)
        )
        .to_uppercase()
    })
}

fn lr_backups(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (5, 15), 0.3, |rng, _| {
        format!(
            "{}-{:02}-{:02} 1{}{}.zip",
            rng.range(2021, 2024),
            rng.range(1, 12),
            rng.range(1, 28),
            rng.range(0, 9),
            rng.range(0, 9)
        )
    })
}

fn music(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(80, 200);
    subdirs(rng, bytes, n, 0.6, artist, 0, |rng, _| {
        format!(
            "{} {}",
            rng.pick(&[
                "The", "Los", "Black", "Silver", "Electric", "Velvet", "Neon", "Paper", "Golden"
            ]),
            rng.pick(&[
                "Keys", "Tides", "Wolves", "Lights", "Rivers", "Kites", "Echoes", "Giants",
                "Machines"
            ])
        )
    })
}

fn artist(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(1, 6);
    subdirs(rng, bytes, n, 0.4, album, 0, |rng, _| {
        format!(
            "{} ({})",
            rng.pick(&[
                "Live at the Roxy",
                "First Light",
                "Northbound",
                "B-Sides",
                "Late Nights",
                "Deluxe Edition",
                "Hollow"
            ]),
            rng.range(1975, 2024)
        )
    })
}

fn album(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let ext = *rng.weighted(&[("flac", 3), ("mp3", 4), ("m4a", 2), ("wav", 1)]);
    let n = rng.range_usize(8, 18);
    let mut out = files(rng, bytes, n, 0.3, |rng, i| {
        format!(
            "{:02} - {}.{ext}",
            i + 1,
            rng.pick(&[
                "Opening",
                "Afterglow",
                "Paper Moon",
                "Undertow",
                "Satellite",
                "Homecoming",
                "Static",
                "Finale"
            ])
        )
    });
    out.push(file("folder.jpg", 250 * KB));
    out
}

fn projects(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(2, 5);
    subdirs(rng, bytes, n, 0.6, fcp_library, 0, |rng, _| {
        format!(
            "{} {}.fcpbundle",
            rng.pick(&["Wedding Edit", "Travel Montage", "Documentary", "YouTube"]),
            rng.range(2019, 2024)
        )
    })
}

fn fcp_library(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = super::shares(bytes, &[60, 35, 5]);
    vec![
        dir("Render Files", parts[0], render_files, 0),
        dir("Transcoded Media", parts[1], render_files, 1),
        file("CurrentVersion.flexolibrary", parts[2]),
        file("Settings.plist", 2 * KB),
    ]
    .into_iter()
    .chain(rng.chance(0.5).then(|| file("__Temp", 0)))
    .collect()
}

fn render_files(rng: &mut Rng, bytes: u64, kind: u32) -> Vec<Entry> {
    files(rng, bytes, (40, 300), 1.0, |rng, _| {
        if kind == 1 {
            format!("{}.mov", rng.hex(12))
        } else {
            format!("{}-{}.mov", rng.hex(8).to_uppercase(), rng.range(0, 99))
        }
    })
}
