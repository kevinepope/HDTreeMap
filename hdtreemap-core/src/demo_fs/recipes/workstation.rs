//! "Developer Workstation": a Linux box full of source projects, each with its
//! `node_modules`, Rust `target/` or Python `.venv` plus `.git`; Docker's overlay layers,
//! toolchains (`.rustup`, `.cargo`, `.npm`), datasets and VM images.

use super::dev;
use super::{
    dir, document_name, file, files, layout, locked, named_files, subdirs, Entry, KB, OFFICE_EXTS,
};
use crate::demo_fs::rng::Rng;

/// The drive's top level.
pub fn root(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("home", 740, home_root, 0),
            ("var", 200, var, 0),
            ("usr", 35, dev::toolchain, 6),
            ("opt", 25, dev::toolchain, 3),
        ],
    );
    out.push(locked("lost+found"));
    out.push(locked("root"));
    out
}

fn home_root(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(bytes, &[("dev", 97, home, 0), ("guest", 3, guest, 0)])
}

fn guest(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (5, 25), 1.2, |rng, _| {
        document_name(rng, OFFICE_EXTS)
    })
}

fn home(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("projects", 600, projects, 0),
            ("datasets", 150, datasets, 0),
            ("vm-images", 60, vm_images, 0),
            (".cache", 50, dot_cache, 0),
            (".rustup", 30, dev::toolchain, 5),
            (".cargo", 30, cargo_home, 0),
            (".npm", 25, npm_cache, 0),
            ("Downloads", 40, downloads, 0),
            ("Documents", 15, guest, 0),
        ],
    );
    out.push(file(".bash_history", 220 * KB));
    out.push(file(".bashrc", 4 * KB));
    out.push(file(".gitconfig", KB));
    out.push(locked(".ssh"));
    out
}

fn projects(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    // Grouped by client / area, as people do once there are dozens.
    let groups = ["work", "oss", "experiments", "archive", "clients"];
    let n = groups.len();
    let mut i = 0;
    subdirs(rng, bytes, n, 0.6, project_group, 0, |_, _| {
        let name = groups[i % n].to_string();
        i += 1;
        name
    })
}

fn project_group(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range(6, 12) as u32;
    dev::projects(rng, bytes, n)
}

fn datasets(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(6, 14);
    subdirs(rng, bytes, n, 1.2, dataset, 0, |rng, _| {
        format!(
            "{}-{}",
            rng.pick(&[
                "imagenet",
                "common-crawl",
                "openwebtext",
                "laion",
                "coco",
                "librispeech",
                "kitti",
                "taxi-trips",
                "weather"
            ]),
            rng.pick(&["subset", "2023", "v2", "clean", "raw", "shards"])
        )
    })
}

fn dataset(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(10, 120);
    let ext = *rng.weighted(&[
        ("parquet", 4),
        ("tar", 2),
        ("jsonl.zst", 2),
        ("arrow", 1),
        ("tfrecord", 1),
    ]);
    let mut out = files(rng, bytes, n, 0.4, |_, i| format!("shard-{i:05}.{ext}"));
    out.push(file("README.md", 4 * KB));
    out
}

fn vm_images(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (3, 6), 0.8, |rng, _| {
        format!(
            "{}.{}",
            rng.pick(&[
                "ubuntu-24.04",
                "debian-12",
                "windows-server-2022",
                "fedora-40",
                "freebsd-14"
            ]),
            rng.pick(&["qcow2", "vmdk", "img"])
        )
    })
}

fn dot_cache(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("pip", 30, cache_blobs, 0),
            ("yarn", 25, cache_blobs, 1),
            ("huggingface", 35, hf_cache, 0),
            ("go-build", 10, cache_blobs, 2),
        ],
    )
}

fn cache_blobs(rng: &mut Rng, bytes: u64, kind: u32) -> Vec<Entry> {
    subdirs(rng, bytes, 64, 0.5, blob_bucket, kind, |_, i| {
        format!("{i:02x}")
    })
}

fn blob_bucket(rng: &mut Rng, bytes: u64, kind: u32) -> Vec<Entry> {
    files(rng, bytes, (10, 60), 1.4, |rng, _| match kind {
        1 => format!(
            "npm-{}-{}.tgz",
            rng.pick(&["react", "lodash", "webpack", "babel", "eslint"]),
            rng.hex(12)
        ),
        2 => format!("{}-d", rng.hex(64)),
        _ => rng.hex(56),
    })
}

fn hf_cache(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(4, 10);
    subdirs(rng, bytes, n, 1.0, hf_model, 0, |rng, _| {
        format!(
            "models--{}--{}",
            rng.pick(&[
                "meta-llama",
                "mistralai",
                "openai",
                "google",
                "stabilityai",
                "bert"
            ]),
            rng.pick(&["base", "large", "7b", "13b", "xl", "small"])
        )
    })
}

fn hf_model(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let shards = rng.range_usize(1, 8);
    let mut out = files(rng, bytes, shards, 0.1, |_, i| {
        format!("model-{:05}-of-{shards:05}.safetensors", i + 1)
    });
    out.push(file("config.json", KB));
    out.push(file("tokenizer.json", 2 * 1024 * KB));
    out
}

fn cargo_home(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("registry", 85, registry, 0),
            ("git", 15, dev::toolchain, 2),
        ],
    )
}

fn registry(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("src", 70, dev::toolchain, 12),
            ("cache", 25, crate_cache, 0),
            ("index", 5, cache_blobs, 2),
        ],
    )
}

fn crate_cache(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir(
        format!("index.crates.io-{}", rng.hex(16)),
        bytes,
        crate_files,
        0,
    )]
}

fn crate_files(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (300, 900), 1.2, |rng, i| {
        format!(
            "{}-{}.{}.{i}.crate",
            rng.pick(&["serde", "tokio", "syn", "rand", "regex", "wgpu", "image", "clap"]),
            rng.range(0, 2),
            rng.range(0, 40)
        )
    })
}

fn npm_cache(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[("_cacache", 98, cacache, 0), ("_logs", 2, npm_logs, 0)],
    )
}

fn cacache(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("content-v2", 90, cache_blobs, 0),
            ("index-v5", 10, cache_blobs, 2),
        ],
    )
}

fn npm_logs(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (10, 40), 0.6, |rng, _| {
        format!(
            "{}-{:02}-{:02}T{:02}_{:02}_{:02}_{:03}Z-debug-0.log",
            rng.range(2023, 2025),
            rng.range(1, 12),
            rng.range(1, 28),
            rng.range(0, 23),
            rng.range(0, 59),
            rng.range(0, 59),
            rng.range(0, 999)
        )
    })
}

fn downloads(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (30, 90), 1.8, |rng, _| {
        format!(
            "{}.{}",
            rng.pick(&[
                "node-v20.11.0-linux-x64",
                "go1.22.linux-amd64",
                "cuda_12.4_linux",
                "jetbrains-toolbox",
                "android-studio",
                "zig-linux",
                "postgresql-16",
                "docker-desktop"
            ]),
            rng.pick(&["tar.xz", "tar.gz", "run", "deb", "zip"])
        )
    })
}

fn var(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("lib", 92, var_lib, 0),
            ("log", 5, var_log, 0),
            ("cache", 3, cache_blobs, 2),
        ],
    )
}

fn var_lib(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[("docker", 85, docker, 0), ("postgresql", 15, postgres, 0)],
    )
}

fn docker(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = layout(
        bytes,
        &[
            ("overlay2", 85, overlay2, 0),
            ("volumes", 12, docker_volumes, 0),
            ("image", 3, cache_blobs, 2),
        ],
    );
    out.push(file("engine-id", 36));
    if rng.chance(0.5) {
        out.push(locked("swarm"));
    }
    out
}

fn overlay2(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(120, 260);
    subdirs(rng, bytes, n, 1.4, layer, 0, |rng, _| rng.hex(64))
}

fn layer(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let mut out = vec![dir("diff", bytes, layer_diff, 0)];
    out.push(file("link", 26));
    out.push(file("lower", 200));
    out.push(dir("work", 0, layer_empty, 0));
    out
}

fn layer_empty(_rng: &mut Rng, _bytes: u64, _arg: u32) -> Vec<Entry> {
    Vec::new()
}

fn layer_diff(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let roots = ["usr", "etc", "var", "opt", "app"];
    let n = rng.range_usize(1, roots.len());
    let mut i = 0;
    subdirs(rng, bytes, n, 1.0, layer_folder, 0, |_, _| {
        let name = roots[i % roots.len()].to_string();
        i += 1;
        name
    })
}

fn layer_folder(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(5, 120);
    files(rng, bytes, n, 1.6, |rng, _| {
        let ext = rng.weighted(&[
            ("so", 4),
            ("py", 3),
            ("conf", 1),
            ("js", 2),
            ("json", 1),
            ("", 3),
        ]);
        let stem = rng.pick(&[
            "libssl",
            "libcrypto",
            "python3",
            "node",
            "libstdc++",
            "ld-linux",
            "busybox",
            "nginx",
            "index",
            "settings",
        ]);
        if ext.is_empty() {
            format!("{stem}{}", rng.range(0, 99))
        } else {
            format!("{stem}{}.{ext}", rng.range(0, 99))
        }
    })
}

fn docker_volumes(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(4, 12);
    subdirs(rng, bytes, n, 1.3, volume, 0, |rng, _| {
        format!(
            "{}_{}",
            rng.pick(&["postgres", "redis", "minio", "grafana", "elastic"]),
            rng.pick(&["data", "cache", "logs"])
        )
    })
}

fn volume(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir("_data", bytes, layer_folder, 0)]
}

fn postgres(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir("16", bytes, pg_main, 0)]
}

fn pg_main(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(bytes, &[("main", 1, pg_cluster, 0)])
}

fn pg_cluster(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = super::shares(bytes, &[85, 15]);
    let mut out = vec![
        dir("base", parts[0], pg_base, 0),
        dir("pg_wal", parts[1], pg_wal, 0),
    ];
    out.extend(named_files(
        64 * KB,
        &[
            ("PG_VERSION", 1),
            ("postmaster.opts", 1),
            ("postgresql.auto.conf", 2),
        ],
    ));
    if rng.chance(0.5) {
        out.push(locked("pg_stat_tmp"));
    }
    out
}

fn pg_base(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(3, 8);
    subdirs(rng, bytes, n, 1.5, pg_db, 0, |rng, _| {
        rng.range(16384, 99999).to_string()
    })
}

fn pg_db(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (200, 600), 2.0, |rng, _| {
        rng.range(1000, 99999).to_string()
    })
}

fn pg_wal(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(10, 60);
    files(rng, bytes, n, 0.0, |rng, _| {
        format!(
            "0000000100000{}000000{}",
            rng.hex(3).to_uppercase(),
            rng.hex(2).to_uppercase()
        )
    })
}

fn var_log(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (60, 200), 1.6, |rng, i| {
        let base = rng.pick(&[
            "syslog",
            "kern.log",
            "auth.log",
            "dpkg.log",
            "docker.log",
            "nginx-access.log",
            "journal",
        ]);
        if i % 3 == 0 {
            base.to_string()
        } else {
            format!("{base}.{}.gz", rng.range(1, 30))
        }
    })
}
