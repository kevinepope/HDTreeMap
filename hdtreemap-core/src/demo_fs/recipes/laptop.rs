//! "Nearly Full Laptop SSD": a macOS laptop with 9 GB free. The space hogs to find are a
//! couple of giant virtual-machine images, Xcode's DerivedData and simulator runtimes, and
//! three old iPhone backups, around an ordinary OS, apps and user folders. `Mail` is
//! unreadable without Full Disk Access, as on a real Mac. This file holds the system side
//! (apps, frameworks, `/Library`, `/private`, `/usr`); `laptop_home.rs` the user's home.

use super::dev;
use super::laptop_home;
use super::{dir, file, files, layout, locked, named_files, shares, subdirs, Entry, KB};
use crate::demo_fs::rng::Rng;

pub(super) const APPS: &[&str] = &[
    "Safari",
    "Google Chrome",
    "Firefox",
    "Slack",
    "Zoom",
    "Microsoft Word",
    "Microsoft Excel",
    "Microsoft PowerPoint",
    "Microsoft Teams",
    "Visual Studio Code",
    "Docker",
    "Spotify",
    "Figma",
    "Notion",
    "Discord",
    "1Password",
    "Adobe Photoshop 2024",
    "Adobe Lightroom Classic",
    "Final Cut Pro",
    "Logic Pro",
    "GarageBand",
    "iMovie",
    "Keynote",
    "Pages",
    "Numbers",
    "Blender",
    "Steam",
    "Parallels Desktop",
    "UTM",
    "Postman",
    "Sublime Text",
    "iTerm",
    "Rectangle",
    "VLC",
    "Transmission",
    "The Unarchiver",
    "Raycast",
    "Obsidian",
];

pub(super) const FRAMEWORKS: &[&str] = &[
    "AppKit",
    "Foundation",
    "CoreData",
    "CoreGraphics",
    "CoreImage",
    "CoreML",
    "Metal",
    "MetalKit",
    "AVFoundation",
    "AudioToolbox",
    "WebKit",
    "SwiftUI",
    "Combine",
    "CloudKit",
    "Contacts",
    "EventKit",
    "GameKit",
    "HealthKit",
    "MapKit",
    "PDFKit",
    "Photos",
    "QuartzCore",
    "Security",
    "StoreKit",
    "Vision",
    "NaturalLanguage",
    "Speech",
    "SpriteKit",
    "SceneKit",
    "RealityKit",
    "ARKit",
    "CoreBluetooth",
    "CoreLocation",
    "CoreMedia",
    "CoreText",
    "Accelerate",
    "IOKit",
    "Network",
    "UniformTypeIdentifiers",
    "UserNotifications",
];

/// The drive's top level.
pub fn root(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("Applications", 125, applications, 0),
            ("Library", 32, system_library, 0),
            ("System", 28, system, 0),
            ("Users", 765, laptop_home::users, 0),
            ("private", 42, private, 0),
            ("usr", 8, usr, 0),
        ],
    );
    out.push(locked(".Spotlight-V100"));
    out.push(locked(".fseventsd"));
    out
}

fn applications(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    // Xcode alone is a big slice; the other apps share the rest.
    let parts = shares(bytes, &[30, 70]);
    let mut out = vec![dir("Xcode.app", parts[0], app_bundle, 8)];
    let n = rng.range_usize(24, APPS.len());
    out.extend(subdirs(rng, parts[1], n, 1.1, app_bundle, 1, |rng, i| {
        if i < APPS.len() {
            format!("{}.app", APPS[i])
        } else {
            format!("{}.app", rng.pick(APPS))
        }
    }));
    out
}

/// A `.app` bundle; `scale` multiplies its file count.
fn app_bundle(_rng: &mut Rng, bytes: u64, scale: u32) -> Vec<Entry> {
    vec![dir("Contents", bytes, app_contents, scale)]
}

fn app_contents(rng: &mut Rng, bytes: u64, scale: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[22, 40, 38]);
    vec![
        file("Info.plist", 4 * KB),
        file("PkgInfo", 8),
        dir("MacOS", parts[0], app_binaries, 0),
        dir("Resources", parts[1], app_resources, scale),
        dir("Frameworks", parts[2], app_frameworks, scale),
        dir("_CodeSignature", 0, empty_sig, 0),
    ]
    .into_iter()
    .chain(
        rng.chance(0.4)
            .then(|| file("embedded.provisionprofile", 12 * KB)),
    )
    .collect()
}

fn empty_sig(_rng: &mut Rng, _bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![file("CodeResources", 90 * KB)]
}

fn app_binaries(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(1, 4);
    files(rng, bytes, n, 0.8, |rng, i| {
        if i == 0 {
            "main".to_string()
        } else {
            format!("{} Helper", rng.pick(APPS))
        }
    })
}

fn app_resources(rng: &mut Rng, bytes: u64, scale: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[70, 30]);
    let n = rng.range_usize(30, 300) * scale.max(1) as usize;
    let mut out = files(rng, parts[0], n, 1.5, |rng, i| {
        let ext = rng.weighted(&[
            ("png", 10),
            ("icns", 1),
            ("nib", 4),
            ("car", 1),
            ("plist", 3),
            ("json", 2),
            ("js", 2),
            ("pak", 1),
            ("dat", 1),
            ("mlmodelc", 1),
            ("strings", 2),
        ]);
        format!(
            "{}{i}.{ext}",
            rng.pick(&[
                "icon", "asset", "image", "toolbar", "panel", "glyph", "sound", "model", "theme"
            ])
        )
    });
    let languages = rng.range_usize(4, 30);
    out.extend(subdirs(rng, parts[1], languages, 0.4, lproj, 0, |_, i| {
        format!("{}.lproj", LANGS[i % LANGS.len()])
    }));
    out
}

const LANGS: &[&str] = &[
    "en", "de", "fr", "es", "it", "ja", "ko", "zh_CN", "zh_TW", "pt_BR", "pt_PT", "nl", "sv", "da",
    "fi", "nb", "pl", "ru", "tr", "uk", "cs", "hu", "el", "he", "ar", "th", "vi", "id", "ms", "hi",
];

fn lproj(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(2, 12);
    files(rng, bytes, n, 0.9, |rng, _| {
        format!(
            "{}.strings",
            rng.pick(&[
                "Localizable",
                "InfoPlist",
                "MainMenu",
                "Preferences",
                "Errors",
                "Onboarding",
                "Help"
            ])
        )
    })
}

fn app_frameworks(rng: &mut Rng, bytes: u64, scale: u32) -> Vec<Entry> {
    let n = rng.range_usize(2, 12) * scale.max(1) as usize;
    subdirs(rng, bytes, n, 1.2, framework, 0, |rng, _| {
        format!("{}.framework", rng.pick(FRAMEWORKS))
    })
}

fn framework(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[75, 25]);
    let mut out = vec![file("Info.plist", 2 * KB)];
    out.push(file(rng.pick(FRAMEWORKS).to_string(), parts[0]));
    let n = rng.range_usize(4, 40);
    out.push(dir("Resources", parts[1], framework_resources, n as u32));
    out
}

fn framework_resources(rng: &mut Rng, bytes: u64, n: u32) -> Vec<Entry> {
    files(rng, bytes, n as usize, 1.2, |rng, i| {
        let ext = rng.weighted(&[
            ("plist", 3),
            ("strings", 3),
            ("png", 4),
            ("car", 1),
            ("metallib", 1),
            ("dylib", 1),
        ]);
        format!("res{i}.{ext}")
    })
}

fn system(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir(
        "Library",
        bytes,
        system_frameworks_root,
        rng.range(1, 2) as u32,
    )]
}

fn system_frameworks_root(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("Frameworks", 40, os_frameworks, 0),
            ("PrivateFrameworks", 35, os_frameworks, 1),
            ("Fonts", 10, fonts, 0),
            ("Extensions", 8, kexts, 0),
            ("CoreServices", 7, os_frameworks, 2),
        ],
    )
}

fn os_frameworks(rng: &mut Rng, bytes: u64, kind: u32) -> Vec<Entry> {
    let n = match kind {
        0 => 120,
        1 => 260,
        _ => 40,
    };
    subdirs(rng, bytes, n, 1.1, framework, 0, |rng, _| {
        let base = rng.pick(FRAMEWORKS);
        match kind {
            1 => format!("{base}Support.framework"),
            2 => format!("{base}Agent.app"),
            _ => format!("{base}.framework"),
        }
    })
}

pub(super) fn fonts(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (150, 400), 1.2, |rng, _| {
        let ext = rng.weighted(&[("ttf", 4), ("ttc", 3), ("otf", 2)]);
        format!(
            "{}{}.{ext}",
            rng.pick(&[
                "Helvetica",
                "Avenir",
                "Menlo",
                "SF Pro",
                "Times",
                "Georgia",
                "Futura",
                "Gill Sans",
                "Noto Sans",
                "PingFang",
                "Hiragino",
                "Apple Color Emoji"
            ]),
            rng.pick(&["", " Bold", " Italic", " Light", " Medium", " Condensed"])
        )
    })
}

fn kexts(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    subdirs(rng, bytes, (80, 160), 1.0, kext, 0, |rng, _| {
        format!(
            "{}Family.kext",
            rng.pick(&[
                "IOUSB",
                "IOStorage",
                "IONetworking",
                "IOGraphics",
                "IOAudio",
                "IOBluetooth",
                "AppleHID",
                "AppleThunderbolt",
                "IOPCI",
                "AppleNVMe"
            ])
        )
    })
}

fn kext(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    named_files(
        bytes,
        &[("Info.plist", 1), ("binary", 12), ("version.plist", 1)],
    )
}

fn system_library(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("Application Support", 25, app_support, 0),
            ("Developer", 45, dev::toolchain, 4),
            ("Audio", 20, audio_loops, 0),
            ("Caches", 6, caches, 0),
            ("Fonts", 4, fonts, 0),
        ],
    );
    if rng.chance(0.9) {
        out.push(file(".localized", 0));
    }
    out
}

fn audio_loops(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir("Apple Loops", bytes, loops, rng.range(300, 900) as u32)]
}

fn loops(rng: &mut Rng, bytes: u64, n: u32) -> Vec<Entry> {
    files(rng, bytes, n as usize, 0.7, |rng, i| {
        format!(
            "{} {} {i:03}.caf",
            rng.pick(&["Funk", "Lofi", "Ambient", "Rock", "Jazz", "Synth", "Hip Hop", "House"]),
            rng.pick(&["Beat", "Bass", "Keys", "Pad", "Guitar", "Drums", "Vox"])
        )
    })
}

pub(super) fn app_support(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(10, 30);
    subdirs(rng, bytes, n, 1.5, support_folder, 0, |rng, _| {
        rng.pick(APPS).to_string()
    })
}

pub(super) fn support_folder(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(3, 60);
    files(rng, bytes, n, 1.8, |rng, i| {
        let ext = rng.weighted(&[
            ("db", 3),
            ("sqlite", 3),
            ("json", 3),
            ("plist", 2),
            ("log", 2),
            ("bin", 1),
            ("ldb", 2),
            ("dat", 1),
        ]);
        format!(
            "{}-{i}.{ext}",
            rng.pick(&["data", "index", "cache", "state", "store", "history", "session"])
        )
    })
}

pub(super) fn caches(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(15, 60);
    subdirs(rng, bytes, n, 1.6, cache_folder, 0, |rng, _| {
        format!(
            "com.{}.{}",
            rng.pick(&[
                "apple",
                "google",
                "microsoft",
                "spotify",
                "tinyspeck",
                "docker",
                "figma",
                "jetbrains"
            ]),
            rng.pick(&[
                "Safari",
                "Chrome",
                "helper",
                "updater",
                "ShipIt",
                "CacheStorage",
                "app",
                "agent"
            ])
        )
    })
}

fn cache_folder(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(5, 400);
    files(rng, bytes, n, 1.5, |rng, _| {
        if rng.chance(0.7) {
            rng.hex(16)
        } else {
            format!(
                "{}.{}",
                rng.hex(8),
                rng.pick(&["db", "cache", "tmp", "dat", "json"])
            )
        }
    })
}

fn private(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir("var", bytes, private_var, 0)]
}

fn private_var(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("vm", 70, swap, 0),
            ("folders", 20, caches, 0),
            ("log", 7, laptop_home::logs, 0),
            ("db", 3, support_folder, 0),
        ],
    );
    out.push(locked("audit"));
    out
}

fn swap(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    named_files(
        bytes,
        &[
            ("sleepimage", 16),
            ("swapfile0", 1),
            ("swapfile1", 1),
            ("swapfile2", 1),
            ("swapfile3", 1),
        ],
    )
}

fn usr(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("bin", 15, dev::toolchain, 1),
            ("lib", 45, dev::toolchain, 2),
            ("share", 30, dev::toolchain, 2),
            ("libexec", 10, dev::toolchain, 1),
        ],
    )
}
