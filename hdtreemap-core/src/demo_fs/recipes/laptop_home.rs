//! The user's home folder on the "Nearly Full Laptop SSD" (`laptop.rs` has the rest of the
//! drive): `~/Library` with Xcode's DerivedData, simulator runtimes and three old iPhone
//! backups, two giant virtual machines, a Photos library, and the everyday folders.

use super::dev;
use super::laptop::{app_support, caches, support_folder, APPS, FRAMEWORKS};
use super::{
    dir, document_name, file, files, layout, locked, named_files, photo_name, shares, subdirs,
    Entry, KB, OFFICE_EXTS,
};
use crate::demo_fs::rng::Rng;

pub(super) fn users(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(bytes, &[("alex", 990, home, 0), ("Shared", 10, shared, 0)])
}

fn shared(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (5, 30), 1.4, |rng, _| {
        document_name(rng, OFFICE_EXTS)
    })
}

fn home(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("Library", 420, user_library, 0),
            ("Virtual Machines", 230, virtual_machines, 0),
            ("Pictures", 110, pictures, 0),
            ("Movies", 50, movies, 0),
            ("Music", 25, music, 0),
            ("Documents", 45, documents, 0),
            ("Downloads", 55, downloads, 0),
            ("Desktop", 12, desktop, 0),
            ("Developer", 45, developer, 0),
            (".Trash", 8, trash, 0),
        ],
    );
    out.push(file(".zsh_history", 180 * KB));
    out.push(file(".zshrc", 3 * KB));
    out.push(file(".gitconfig", KB));
    out
}

fn user_library(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("Developer", 420, xcode_developer, 0),
            ("Application Support", 300, user_app_support, 0),
            ("Caches", 120, caches, 0),
            ("Containers", 90, containers, 0),
            ("Group Containers", 40, containers, 1),
            ("Logs", 10, logs, 0),
            ("Preferences", 2, preferences, 0),
        ],
    );
    out.push(locked("Mail"));
    out.push(locked("Safari"));
    out
}

fn xcode_developer(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("Xcode", 45, xcode_data, 0),
            ("CoreSimulator", 55, simulators, 0),
        ],
    )
}

fn xcode_data(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("DerivedData", 60, derived_data, 0),
            ("iOS DeviceSupport", 30, device_support, 0),
            ("Archives", 10, archives, 0),
        ],
    )
}

fn derived_data(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(8, 20);
    subdirs(rng, bytes, n, 1.0, derived_project, 0, |rng, _| {
        format!(
            "{}-{}",
            rng.pick(&["MyApp", "Weather", "Notes", "Tracker", "Game", "Widget", "Budget"]),
            rng.hex(28)
        )
    })
}

fn derived_project(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("Build", 70, build_products, 0),
            ("Index.noindex", 30, index_store, 0),
        ],
    )
}

fn build_products(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(80, 600);
    files(rng, bytes, n, 1.3, |rng, _| {
        let ext = rng.weighted(&[
            ("o", 8),
            ("swiftmodule", 2),
            ("dia", 2),
            ("d", 2),
            ("swiftdeps", 2),
            ("a", 1),
        ]);
        format!(
            "{}{}.{ext}",
            rng.pick(&["View", "Model", "Controller", "Store", "Service", "Cell"]),
            rng.range(1, 999)
        )
    })
}

fn index_store(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (100, 900), 1.0, |rng, _| {
        format!("{}.unit", rng.hex(20))
    })
}

fn device_support(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(3, 7);
    subdirs(rng, bytes, n, 0.4, symbols, 0, |rng, _| {
        format!(
            "iPhone15,{} 17.{} (21{}{})",
            rng.range(2, 5),
            rng.range(0, 6),
            rng.pick(&["A", "B", "C", "D", "E", "F"]),
            rng.range(100, 400)
        )
    })
}

fn symbols(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (30, 200), 1.6, |rng, _| {
        format!("{}.dylib", rng.pick(FRAMEWORKS))
    })
}

fn archives(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (3, 12), 0.8, |rng, _| {
        format!(
            "MyApp {}-{:02}-{:02}.xcarchive.zip",
            rng.range(2021, 2025),
            rng.range(1, 12),
            rng.range(1, 28)
        )
    })
}

fn simulators(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[55, 45]);
    let mut out = vec![dir("Devices", parts[0], sim_devices, 0)];
    out.push(dir(
        "Caches",
        parts[1],
        sim_runtimes,
        rng.range(3, 6) as u32,
    ));
    out
}

fn sim_devices(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(8, 24);
    subdirs(rng, bytes, n, 1.2, sim_device, 0, |rng, _| {
        format!(
            "{}-{}-{}-{}-{}",
            rng.hex(8),
            rng.hex(4),
            rng.hex(4),
            rng.hex(4),
            rng.hex(12)
        )
        .to_uppercase()
    })
}

fn sim_device(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(40, 300);
    let mut out = files(rng, bytes, n, 1.9, |rng, _| {
        let ext = rng.weighted(&[
            ("sqlite", 3),
            ("plist", 4),
            ("db", 2),
            ("png", 2),
            ("dylib", 1),
            ("dat", 1),
        ]);
        format!(
            "{}{}.{ext}",
            rng.pick(&["data", "container", "prefs", "cache", "store", "media"]),
            rng.range(0, 9999)
        )
    });
    out.push(file("device.plist", 2 * KB));
    out
}

fn sim_runtimes(rng: &mut Rng, bytes: u64, n: u32) -> Vec<Entry> {
    files(rng, bytes, n as usize, 0.3, |rng, _| {
        format!("iOS 17.{} Simulator Runtime.dmg", rng.range(0, 5))
    })
}

fn user_app_support(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    // Three old iPhone backups take most of it.
    let parts = shares(bytes, &[72, 28]);
    let mut out = vec![dir("MobileSync", parts[0], mobile_sync, 0)];
    out.extend(app_support(rng, parts[1], 0));
    out
}

fn mobile_sync(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir("Backup", bytes, iphone_backups, 3)]
}

fn iphone_backups(rng: &mut Rng, bytes: u64, n: u32) -> Vec<Entry> {
    subdirs(rng, bytes, n as usize, 0.3, iphone_backup, 0, |rng, _| {
        format!("{}-{}", rng.hex(8), rng.hex(16)).to_uppercase()
    })
}

/// An iTunes/Finder backup: 256 two-hex-digit folders of hash-named files.
fn iphone_backup(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[995, 5]);
    let mut out = subdirs(rng, parts[0], 256, 0.5, backup_bucket, 0, |_, i| {
        format!("{i:02x}")
    });
    out.extend(named_files(
        parts[1],
        &[
            ("Manifest.db", 90),
            ("Manifest.plist", 5),
            ("Info.plist", 4),
            ("Status.plist", 1),
        ],
    ));
    out
}

fn backup_bucket(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (5, 40), 2.0, |rng, _| rng.hex(40))
}

fn containers(rng: &mut Rng, bytes: u64, group: u32) -> Vec<Entry> {
    let n = rng.range_usize(20, 70);
    subdirs(rng, bytes, n, 1.7, support_folder, 0, |rng, _| {
        let prefix = if group == 1 { "group." } else { "" };
        format!(
            "{prefix}com.{}.{}",
            rng.pick(&[
                "apple",
                "microsoft",
                "agilebits",
                "culturedcode",
                "readdle",
                "spotify"
            ]),
            rng.pick(&[
                "Notes",
                "mail",
                "photos",
                "Office",
                "onepassword",
                "Things",
                "helper"
            ])
        )
    })
}

pub(super) fn logs(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (30, 200), 1.2, |rng, i| {
        format!("{}_{}.log", rng.pick(APPS).replace(' ', ""), i)
    })
}

fn preferences(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (120, 300), 0.8, |rng, _| {
        format!(
            "com.{}.{}.plist",
            rng.pick(&["apple", "google", "microsoft", "figma", "docker"]),
            rng.pick(APPS).replace(' ', "")
        )
    })
}

fn virtual_machines(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("Windows 11.pvm", 62, pvm, 0),
            ("Ubuntu 22.04.utm", 38, pvm, 1),
        ],
    )
}

fn pvm(_rng: &mut Rng, bytes: u64, kind: u32) -> Vec<Entry> {
    if kind == 1 {
        return vec![file("config.plist", 6 * KB), dir("Data", bytes, vm_data, 1)];
    }
    vec![
        file("config.pvs", 40 * KB),
        file("VmInfo.pvi", 2 * KB),
        dir("Windows 11-0.hdd", bytes, vm_data, 0),
    ]
}

fn vm_data(rng: &mut Rng, bytes: u64, kind: u32) -> Vec<Entry> {
    let disk = bytes - bytes / 50;
    let mut out = vec![file(
        if kind == 1 {
            format!("{}.qcow2", rng.hex(8).to_uppercase())
        } else {
            format!("Windows 11-0.hdd.0.{{{}}}.hds", rng.hex(8))
        },
        disk,
    )];
    out.push(file(
        if kind == 1 {
            "efi_vars.fd"
        } else {
            "DiskDescriptor.xml"
        },
        bytes / 50,
    ));
    out
}

fn pictures(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("Photos Library.photoslibrary", 92, photos_library, 0),
            ("Screenshots", 8, screenshots, 0),
        ],
    )
}

fn photos_library(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("originals", 85, photo_buckets, 0),
            ("resources", 12, photo_buckets, 1),
            ("database", 3, photo_database, 0),
        ],
    )
}

fn photo_buckets(rng: &mut Rng, bytes: u64, derivatives: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 16, 0.2, photo_bucket, derivatives, |_, i| {
        format!("{i:X}")
    })
}

fn photo_bucket(rng: &mut Rng, bytes: u64, derivatives: u32) -> Vec<Entry> {
    let n = rng.range_usize(200, 500);
    files(rng, bytes, n, 1.0, |rng, _| {
        let id = format!(
            "{}-{}-{}-{}-{}",
            rng.hex(8),
            rng.hex(4),
            rng.hex(4),
            rng.hex(4),
            rng.hex(12)
        )
        .to_uppercase();
        let ext = if derivatives == 1 {
            "jpeg"
        } else {
            rng.weighted(&[("heic", 70), ("jpeg", 15), ("mov", 12), ("png", 3)])
        };
        format!("{id}.{ext}")
    })
}

fn photo_database(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    named_files(
        bytes,
        &[
            ("Photos.sqlite", 80),
            ("Photos.sqlite-wal", 15),
            ("metaSchema.db", 5),
        ],
    )
}

fn screenshots(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (80, 400), 0.6, |rng, _| {
        format!(
            "Screenshot {}-{:02}-{:02} at {}.{:02}.{:02}.png",
            rng.range(2022, 2025),
            rng.range(1, 12),
            rng.range(1, 28),
            rng.range(8, 23),
            rng.range(0, 59),
            rng.range(0, 59)
        )
    })
}

fn movies(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (10, 40), 1.6, |rng, _| match rng.below(3) {
        0 => format!(
            "Screen Recording {}-{:02}-{:02}.mov",
            rng.range(2022, 2025),
            rng.range(1, 12),
            rng.range(1, 28)
        ),
        1 => format!(
            "{}.mp4",
            rng.pick(&[
                "Birthday",
                "Wedding toast",
                "Beach day",
                "Graduation",
                "Concert",
                "Holiday"
            ])
        ),
        _ => format!("{}.m4v", rng.pick(&["Talk", "Demo", "Tutorial", "Keynote"])),
    })
}

fn music(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir("Music", bytes, albums, rng.range(20, 60) as u32)]
}

fn albums(rng: &mut Rng, bytes: u64, n: u32) -> Vec<Entry> {
    subdirs(rng, bytes, n as usize, 0.5, album, 0, |rng, _| {
        format!(
            "{} - {}",
            rng.pick(&[
                "The Midnight",
                "Bonobo",
                "Khruangbin",
                "Tycho",
                "Radiohead",
                "Daft Punk",
                "Nils Frahm",
                "Moderat"
            ]),
            rng.pick(&[
                "Live",
                "Greatest Hits",
                "Sessions",
                "Remastered",
                "Vol. 2",
                "Endless Summer"
            ])
        )
    })
}

fn album(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(8, 16);
    let ext = *rng.weighted(&[("m4a", 3), ("mp3", 2), ("flac", 1)]);
    let mut out = files(rng, bytes, n, 0.3, |rng, i| {
        format!(
            "{:02} {}.{ext}",
            i + 1,
            rng.pick(&[
                "Intro",
                "Night Drive",
                "Sunset",
                "Echoes",
                "Waves",
                "Glow",
                "Outro",
                "Static"
            ])
        )
    });
    out.push(file("cover.jpg", 300 * KB));
    out
}

fn documents(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[40, 60]);
    let mut out = files(rng, parts[0], (60, 200), 1.6, |rng, _| {
        document_name(rng, OFFICE_EXTS)
    });
    out.extend(subdirs(
        rng,
        parts[1],
        (5, 12),
        1.2,
        doc_folder,
        0,
        |rng, _| {
            (*rng.pick(&[
                "Taxes",
                "Work",
                "School",
                "House",
                "Travel",
                "Receipts",
                "Contracts",
                "Health",
                "Car",
                "Kids",
            ]))
            .to_string()
        },
    ));
    out
}

fn doc_folder(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (5, 80), 1.5, |rng, _| {
        document_name(rng, OFFICE_EXTS)
    })
}

fn downloads(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (80, 250), 2.0, |rng, _| match rng.below(4) {
        0 => format!(
            "{}-{}.{}.dmg",
            rng.pick(APPS).replace(' ', ""),
            rng.range(1, 30),
            rng.range(0, 9)
        ),
        1 => format!(
            "{}.zip",
            rng.pick(&["photos", "export", "project", "assets", "backup", "invoices"])
        ),
        2 => document_name(rng, OFFICE_EXTS),
        _ => photo_name("IMG_", rng.range_usize(0, 9999), "jpeg"),
    })
}

fn desktop(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (20, 70), 1.4, |rng, i| {
        if rng.chance(0.5) {
            format!("Screenshot {i}.png")
        } else {
            document_name(rng, OFFICE_EXTS)
        }
    })
}

fn developer(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range(5, 10) as u32;
    dev::projects(rng, bytes, n)
}

fn trash(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (20, 90), 1.8, |rng, _| {
        if rng.chance(0.5) {
            document_name(rng, OFFICE_EXTS)
        } else {
            format!("{}.dmg", rng.pick(APPS))
        }
    })
}
