//! "Huge Archive Server": the stress test. Around 1.5 million files: host backups (few, huge
//! `.tar.zst` / `.bak` files holding most of the bytes), years of daily logs per host, an
//! image dataset with hundreds of thousands of small JPEGs, maildirs with a file per message,
//! users' home folders and bare git mirrors.

use super::dev;
use super::{
    dir, document_name, file, files, layout, locked, subdirs, Entry, KB, MONTHS, OFFICE_EXTS,
};
use crate::demo_fs::rng::Rng;

const HOSTS: &[&str] = &[
    "web01", "web02", "web03", "db01", "db02", "cache01", "mq01", "build01", "build02", "ci01",
    "files01", "auth01", "mail01", "proxy01", "proxy02", "log01", "mon01", "vpn01", "dev01",
    "gpu01", "gpu02", "backup01", "nas01", "edge01", "edge02",
];

const USERS: &[&str] = &[
    "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi", "ivan", "judy", "mallory",
    "niaj", "olivia", "peggy", "rupert", "sybil", "trent", "victor", "walter", "yolanda", "zoe",
    "amir", "bea", "chen", "dmitri", "elena", "farah", "goro", "hana", "igor", "jonas", "kai",
    "lena", "mateo", "nora", "omar", "priya", "quinn", "rosa", "sven",
];

/// The drive's top level.
pub fn root(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("datasets", 110, datasets, 0),
            ("home", 70, homes, 0),
            ("logs", 50, logs, 0),
            ("mail", 35, mail, 0),
            ("git-mirrors", 25, git_mirrors, 0),
            ("vm-images", 10, vm_images, 0),
            // Most of the bytes in a few huge files (the engine interleaves the top-level
            // folders, so the order here does not shape the live map).
            ("backups", 700, backups, 0),
        ],
    );
    out.push(locked("lost+found"));
    out
}

fn backups(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut i = 0;
    subdirs(rng, bytes, HOSTS.len(), 0.9, host_backups, 0, |_, _| {
        let name = HOSTS[i % HOSTS.len()].to_string();
        i += 1;
        name
    })
}

fn host_backups(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("daily", 25, backup_set, 30),
            ("weekly", 35, backup_set, 26),
            ("monthly", 40, backup_set, 24),
        ],
    )
}

fn backup_set(rng: &mut Rng, bytes: u64, count: u32) -> Vec<Entry> {
    let ext = *rng.weighted(&[("tar.zst", 4), ("tar.gz", 2), ("bak", 1), ("img.zst", 1)]);
    files(rng, bytes, count as usize, 0.3, |rng, i| {
        format!(
            "{}-{}-{:02}-{:02}.{ext}",
            rng.pick(&["full", "snapshot", "system"]),
            2024 - (i / 12) as u64,
            12 - i % 12,
            rng.range(1, 28)
        )
    })
}

fn datasets(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("imagenet-subset", 55, image_dataset, 0),
            ("coco-2017", 25, coco, 0),
            ("sensor-logs", 20, sensor_logs, 0),
        ],
    )
}

fn image_dataset(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("train", 92, image_classes, 600),
            ("val", 8, image_classes, 50),
        ],
    )
}

/// 1000 class folders of `per_class` small JPEGs each.
fn image_classes(rng: &mut Rng, bytes: u64, per_class: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 1000, 0.15, image_class, per_class, |_, i| {
        format!("n{:08}", 1_440_764 + i * 9_973)
    })
}

fn image_class(rng: &mut Rng, bytes: u64, per_class: u32) -> Vec<Entry> {
    let n = per_class as usize * rng.range_usize(80, 120) / 100;
    let class = rng.range(1_000_000, 9_999_999);
    files(rng, bytes, n, 0.4, |_, i| format!("n{class:08}_{i}.JPEG"))
}

fn coco(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("train2017", 85, coco_images, 118_287),
            ("val2017", 4, coco_images, 5_000),
            ("annotations", 11, coco_annotations, 0),
        ],
    )
}

fn coco_images(rng: &mut Rng, bytes: u64, n: u32) -> Vec<Entry> {
    let start = rng.range(1, 1000);
    files(rng, bytes, n as usize, 0.3, |_, i| {
        format!("{:012}.jpg", start + i as u64 * 7)
    })
}

fn coco_annotations(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    super::named_files(
        bytes,
        &[
            ("instances_train2017.json", 45),
            ("captions_train2017.json", 10),
            ("person_keypoints_train2017.json", 25),
            ("instances_val2017.json", 2),
        ],
    )
}

fn sensor_logs(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 36, 0.4, sensor_month, 0, |_, i| {
        format!("{}-{:02}", 2022 + i / 12, i % 12 + 1)
    })
}

fn sensor_month(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(28, 31) * 2;
    files(rng, bytes, n, 0.3, |rng, i| {
        format!("day{:02}-{}.parquet", i / 2 + 1, rng.pick(&["am", "pm"]))
    })
}

fn homes(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut i = 0;
    subdirs(rng, bytes, USERS.len(), 1.0, user_home, 0, |_, _| {
        let name = USERS[i % USERS.len()].to_string();
        i += 1;
        name
    })
}

fn user_home(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("Documents", 35, home_documents, 0),
            ("projects", 45, home_projects, 0),
            ("Pictures", 20, home_pictures, 0),
        ],
    );
    out.push(file(".bash_history", rng.range(2, 400) * KB));
    out.push(file(".profile", 807));
    if rng.chance(0.2) {
        out.push(locked(".gnupg"));
    }
    out
}

fn home_documents(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(4, 10);
    subdirs(rng, bytes, n, 1.0, doc_year, 0, |rng, _| {
        rng.range(2015, 2024).to_string()
    })
}

fn doc_year(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (30, 200), 1.4, |rng, _| {
        document_name(rng, OFFICE_EXTS)
    })
}

fn home_projects(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    // Half the users keep a checkout here; the rest only an archive of old work.
    if rng.chance(0.5) {
        dev::projects(rng, bytes, 1)
    } else {
        files(rng, bytes, (3, 20), 1.0, |rng, _| {
            format!(
                "{}-{}.tar.gz",
                rng.pick(&["thesis", "website", "old-api", "scripts", "dotfiles", "game-jam"]),
                rng.range(2012, 2023)
            )
        })
    }
}

fn home_pictures(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (100, 600), 0.6, |rng, i| {
        format!(
            "{}_{i:05}.{}",
            rng.pick(&["IMG", "DSC", "PXL"]),
            rng.pick(&["jpg", "jpg", "png", "heic"])
        )
    })
}

fn logs(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut i = 0;
    subdirs(rng, bytes, HOSTS.len(), 0.8, host_logs, 0, |_, _| {
        let name = HOSTS[i % HOSTS.len()].to_string();
        i += 1;
        name
    })
}

fn host_logs(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 4, 0.3, log_year, 0, |_, i| {
        (2021 + i).to_string()
    })
}

fn log_year(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 12, 0.3, log_month, 0, |_, i| {
        format!("{:02}-{}", i + 1, MONTHS[i].to_lowercase())
    })
}

fn log_month(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let days = rng.range_usize(28, 31);
    let services = ["syslog", "app", "access"];
    files(rng, bytes, days * services.len(), 0.7, |_, i| {
        format!(
            "{}-{:02}.log.gz",
            services[i % services.len()],
            i / services.len() + 1
        )
    })
}

fn mail(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut i = 0;
    subdirs(rng, bytes, USERS.len() * 4, 1.2, maildir, 0, |rng, _| {
        let name = format!(
            "{}{}",
            USERS[i % USERS.len()],
            if i < USERS.len() {
                String::new()
            } else {
                format!(".{}", rng.pick(&["ops", "sales", "hr", "dev", "support"]))
            }
        );
        i += 1;
        name
    })
}

fn maildir(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("cur", 70, messages, 0),
            ("new", 5, messages, 1),
            (".Sent", 15, sent, 0),
            (".Archive", 10, sent, 1),
        ],
    )
}

fn sent(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir("cur", bytes, messages, 0)]
}

/// One file per message, maildir style: `1695381234.M123P456.mail01,S=5321:2,S`.
fn messages(rng: &mut Rng, bytes: u64, new: u32) -> Vec<Entry> {
    let n = if new == 1 {
        rng.range_usize(0, 40)
    } else {
        rng.range_usize(100, 1200)
    };
    let mut t = rng.range(1_500_000_000, 1_700_000_000);
    files(rng, bytes, n, 1.5, |rng, _| {
        t += rng.range(30, 20_000);
        let flags = if new == 1 { "" } else { ":2,S" };
        format!(
            "{t}.M{}P{}.mail01{flags}",
            rng.range(1, 999_999),
            rng.range(100, 99_999)
        )
    })
}

fn git_mirrors(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(250, 400);
    subdirs(rng, bytes, n, 1.5, dev::git_dir, 0, |rng, _| {
        format!(
            "{}-{}.git",
            rng.pick(&[
                "linux",
                "llvm",
                "rust",
                "chromium",
                "postgres",
                "kubernetes",
                "react",
                "tensorflow",
                "ffmpeg",
                "blender",
                "godot",
                "firefox"
            ]),
            rng.hex(4)
        )
    })
}

fn vm_images(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (8, 20), 1.0, |rng, _| {
        format!(
            "{}-{}.qcow2",
            rng.pick(HOSTS),
            rng.pick(&["template", "golden", "snapshot", "rescue"])
        )
    })
}
