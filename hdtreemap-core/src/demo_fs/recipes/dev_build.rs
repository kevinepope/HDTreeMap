//! Build and tooling folders for the developer recipes (`dev.rs`): Rust `target/`
//! directories (`.rlib`, `.rmeta`, `.o`, incremental caches), `.git` directories with pack
//! files and loose objects, Python virtualenvs with their data and notebooks, and
//! toolchain-like library trees (`.rustup`, `.cargo`, `/usr/lib`).

use super::dev::{empty, CRATES, PROJECT_WORDS};
use super::{dir, file, files, layout, named_files, shares, split, subdirs, unique, Entry, KB};
use crate::demo_fs::rng::Rng;
use std::collections::HashSet;

const PY_PACKAGES: &[&str] = &[
    "numpy",
    "pandas",
    "scipy",
    "matplotlib",
    "torch",
    "torchvision",
    "tensorflow",
    "scikit_learn",
    "jupyterlab",
    "ipykernel",
    "requests",
    "urllib3",
    "certifi",
    "idna",
    "charset_normalizer",
    "pillow",
    "opencv_python",
    "transformers",
    "tokenizers",
    "datasets",
    "pyarrow",
    "sqlalchemy",
    "flask",
    "django",
    "fastapi",
    "pydantic",
    "uvicorn",
    "boto3",
    "botocore",
    "setuptools",
    "pip",
    "wheel",
    "six",
    "attrs",
    "pytest",
    "black",
    "mypy",
];

/// Rust `target/`: debug and release profiles.
pub fn rust_target(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[78, 21, 1]);
    vec![
        dir("debug", parts[0], rust_profile, 0),
        dir("release", parts[1], rust_profile, 1),
        file(".rustc_info.json", parts[2] / 2 + 1),
        file("CACHEDIR.TAG", 177),
    ]
}

fn rust_profile(rng: &mut Rng, bytes: u64, release: u32) -> Vec<Entry> {
    let crates = rng.range_usize(60, 260);
    let incremental = if release == 1 { 0 } else { 30 };
    let parts = shares(bytes, &[55, 8, incremental, 2, 6]);
    let mut out = vec![
        dir("deps", parts[0], rust_deps, crates as u32),
        dir("build", parts[1], rust_build, (crates / 5) as u32),
        dir(".fingerprint", parts[3], rust_fingerprint, crates as u32),
    ];
    if incremental > 0 {
        out.push(dir("incremental", parts[2], rust_incremental, 0));
    }
    let bin = rng.pick(PROJECT_WORDS).to_string();
    out.push(file(bin.clone(), parts[4] * 9 / 10));
    out.push(file(format!("{bin}.d"), parts[4] - parts[4] * 9 / 10));
    out
}

fn rust_deps(rng: &mut Rng, bytes: u64, crates: u32) -> Vec<Entry> {
    let sizes = split(rng, bytes, crates as usize, 1.3);
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(crates as usize * 3);
    for size in sizes {
        let name = rng.pick(CRATES).replace('-', "_");
        let hash = rng.hex(16);
        let stem = unique(&mut seen, format!("lib{name}-{hash}"));
        let parts = shares(size, &[70, 28, 2]);
        out.push(file(format!("{stem}.rlib"), parts[0]));
        out.push(file(format!("{stem}.rmeta"), parts[1]));
        out.push(file(format!("{}.d", &stem[3..]), parts[2]));
    }
    out
}

fn rust_build(rng: &mut Rng, bytes: u64, count: u32) -> Vec<Entry> {
    subdirs(
        rng,
        bytes,
        count.max(1) as usize,
        1.5,
        rust_build_script,
        0,
        |rng, _| format!("{}-{}", rng.pick(CRATES), rng.hex(16)),
    )
}

fn rust_build_script(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[1, 1, 1, 97]);
    let mut out = vec![
        file("output", parts[0]),
        file("stderr", parts[1]),
        file("root-output", parts[2]),
    ];
    if rng.chance(0.5) {
        out.push(dir("out", parts[3], object_files, 0));
    } else {
        out.push(file(
            format!("build_script_build-{}", rng.hex(16)),
            parts[3],
        ));
    }
    out
}

fn object_files(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (2, 40), 1.2, |rng, _| {
        let ext = rng.weighted(&[("o", 6), ("a", 1), ("rs", 2), ("h", 1)]);
        format!(
            "{}-{}.{ext}",
            rng.pick(CRATES).replace('-', "_"),
            rng.hex(8)
        )
    })
}

fn rust_fingerprint(rng: &mut Rng, bytes: u64, crates: u32) -> Vec<Entry> {
    let n = (crates as usize / 3).max(1);
    subdirs(rng, bytes, n, 0.5, fingerprint_files, 0, |rng, _| {
        format!("{}-{}", rng.pick(CRATES), rng.hex(16))
    })
}

fn fingerprint_files(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let lib = rng.pick(CRATES).replace('-', "_");
    named_files(
        bytes,
        &[
            (&format!("lib-{lib}"), 1),
            (&format!("lib-{lib}.json"), 3),
            ("dep-lib", 2),
            ("invoked.timestamp", 1),
        ],
    )
}

fn rust_incremental(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(3, 12);
    subdirs(rng, bytes, n, 1.0, incremental_session, 0, |rng, _| {
        format!("{}-{}", rng.pick(PROJECT_WORDS), rng.hex(13))
    })
}

fn incremental_session(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir(
        format!("s-{}-{}-working", rng.hex(10), rng.hex(14)),
        bytes,
        incremental_objects,
        0,
    )]
}

fn incremental_objects(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[80, 12, 8]);
    let mut out = files(rng, parts[0], (40, 400), 1.0, |rng, _| {
        format!("{}.o", rng.hex(25))
    });
    out.push(file("dep-graph.bin", parts[1]));
    out.push(file("query-cache.bin", parts[2]));
    out.push(file("work-products.bin", 4 * KB));
    out
}

/// A `.git` directory: pack files, loose objects, refs, hooks.
pub fn git_dir(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[94, 4, 1, 1]);
    vec![
        dir("objects", parts[0], git_objects, 0),
        file("index", parts[1]),
        dir("hooks", parts[2], git_hooks, 0),
        dir("refs", parts[3], git_refs, 0),
        file("HEAD", 23),
        file("config", 400),
        file("description", 73),
        file("packed-refs", rng.range(1, 40) * KB),
        dir("logs", rng.range(4, 200) * KB, git_logs, 0),
    ]
}

fn git_objects(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let loose_dirs = rng.range_usize(0, 120);
    let parts = shares(bytes, &[90, if loose_dirs == 0 { 0 } else { 10 }]);
    let mut out = vec![
        dir("pack", parts[0], git_packs, 0),
        dir("info", 0, empty, 0),
    ];
    // Distinct two-digit fan-out folders: a partial shuffle of 00..ff.
    let mut fan_out: Vec<u8> = (0..=255).collect();
    let sizes = split(rng, parts[1], loose_dirs, 0.6);
    for (i, size) in sizes.into_iter().enumerate() {
        let j = rng.range_usize(i, 255);
        fan_out.swap(i, j);
        out.push(dir(format!("{:02x}", fan_out[i]), size, git_loose, 0));
    }
    out
}

fn git_packs(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let packs = rng.range_usize(1, 4);
    let mut out = Vec::new();
    for size in split(rng, bytes, packs, 1.5) {
        let hash = rng.hex(40);
        out.push(file(format!("pack-{hash}.pack"), size - size / 40));
        out.push(file(format!("pack-{hash}.idx"), size / 40));
        if rng.chance(0.5) {
            out.push(file(format!("pack-{hash}.rev"), 0));
        }
    }
    out
}

fn git_loose(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (1, 12), 1.0, |rng, _| rng.hex(38))
}

fn git_hooks(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let hooks = [
        "applypatch-msg",
        "commit-msg",
        "fsmonitor-watchman",
        "post-update",
        "pre-applypatch",
        "pre-commit",
        "pre-merge-commit",
        "pre-push",
        "pre-rebase",
        "pre-receive",
        "prepare-commit-msg",
        "push-to-checkout",
        "update",
    ];
    let names: Vec<String> = hooks.iter().map(|h| format!("{h}.sample")).collect();
    let parts: Vec<(&str, u64)> = names.iter().map(|n| (n.as_str(), 1)).collect();
    named_files(bytes, &parts)
}

fn git_refs(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    layout(
        bytes,
        &[
            ("heads", 2, git_ref_files, 0),
            ("remotes", 2, git_ref_files, 1),
            ("tags", 1, git_ref_files, 2),
        ],
    )
}

fn git_ref_files(rng: &mut Rng, bytes: u64, kind: u32) -> Vec<Entry> {
    let n = rng.range_usize(1, 8);
    files(rng, bytes, n, 0.0, |rng, i| match kind {
        2 => format!("v{}.{}.{}", rng.range(0, 3), rng.range(0, 20), i),
        _ => (*rng.pick(&[
            "main",
            "develop",
            "feature-login",
            "fix-crash",
            "release",
            "wip",
            "spike",
        ]))
        .to_string(),
    })
}

fn git_logs(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![file("HEAD", bytes)]
}

/// A Python virtualenv: `bin/` plus `lib/python3.x/site-packages/<packages>`.
pub(super) fn venv(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[1, 99]);
    vec![
        dir("bin", parts[0], venv_bin, 0),
        dir("lib", parts[1], venv_lib, 0),
        file("pyvenv.cfg", 300),
        file(".gitignore", 70),
    ]
}

fn venv_bin(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    files(rng, bytes, (6, 30), 0.8, |rng, _| {
        (*rng.pick(&[
            "python",
            "python3",
            "pip",
            "pip3",
            "activate",
            "activate.fish",
            "jupyter",
            "pytest",
            "black",
            "uvicorn",
            "f2py",
            "normalizer",
            "tqdm",
        ]))
        .to_string()
    })
}

fn venv_lib(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let version = rng.range(9, 13);
    vec![dir(
        format!("python3.{version}"),
        bytes,
        site_packages_parent,
        0,
    )]
}

fn site_packages_parent(_rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    vec![dir("site-packages", bytes, site_packages, 0)]
}

fn site_packages(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(15, PY_PACKAGES.len());
    subdirs(rng, bytes, n, 1.6, py_package, 0, |rng, _| {
        rng.pick(PY_PACKAGES).to_string()
    })
}

fn py_package(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(4, 60);
    let mut out = files(rng, bytes, n, 1.6, |rng, _| {
        let ext = rng.weighted(&[
            ("py", 10),
            ("pyc", 6),
            ("so", 2),
            ("pyi", 2),
            ("dylib", 1),
            ("json", 1),
        ]);
        format!(
            "{}.{ext}",
            rng.pick(&[
                "__init__",
                "core",
                "_internal",
                "utils",
                "base",
                "api",
                "io",
                "linalg",
                "fft",
                "random",
                "compat",
                "_version"
            ])
        )
    });
    if rng.chance(0.5) {
        out.push(dir("__pycache__", rng.range(20, 900) * KB, pycache, 0));
    }
    out
}

fn pycache(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(2, 20);
    files(rng, bytes, n, 0.7, |rng, _| {
        format!(
            "{}.cpython-3{}.pyc",
            rng.pick(&["__init__", "core", "utils", "base", "api", "io"]),
            rng.range(9, 12)
        )
    })
}

pub(super) fn data_dir(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(5, 40);
    files(rng, bytes, n, 1.8, |rng, i| {
        let ext = rng.weighted(&[
            ("csv", 4),
            ("parquet", 4),
            ("json", 2),
            ("npy", 2),
            ("h5", 1),
            ("pkl", 1),
            ("zip", 1),
        ]);
        format!(
            "{}_{:03}.{ext}",
            rng.pick(&["train", "test", "val", "raw", "features", "labels", "events", "sensors"]),
            i
        )
    })
}

pub(super) fn notebooks(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(3, 20);
    files(rng, bytes, n, 1.2, |rng, i| {
        format!(
            "{:02}-{}.ipynb",
            i,
            rng.pick(&[
                "exploration",
                "cleaning",
                "baseline",
                "training",
                "results",
                "plots",
                "scratch"
            ])
        )
    })
}

/// A toolchain-like tree (`.rustup`, `.cargo/registry`, `/usr/lib`): many folders of
/// libraries and headers. `arg` scales the folder count.
pub fn toolchain(rng: &mut Rng, bytes: u64, scale: u32) -> Vec<Entry> {
    let n = rng.range_usize(4, 10) * scale.max(1) as usize;
    subdirs(rng, bytes, n, 1.2, library_folder, 0, |rng, _| {
        format!(
            "{}-{}.{}.{}",
            rng.pick(CRATES),
            rng.range(0, 4),
            rng.range(0, 40),
            rng.range(0, 12)
        )
    })
}

fn library_folder(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(5, 60);
    files(rng, bytes, n, 1.4, |rng, _| {
        let ext = rng.weighted(&[
            ("rs", 6),
            ("so", 1),
            ("a", 1),
            ("h", 2),
            ("toml", 1),
            ("md", 1),
            ("json", 1),
        ]);
        format!(
            "{}.{ext}",
            rng.pick(&[
                "lib", "mod", "util", "error", "types", "build", "macros", "impls", "tests",
                "README"
            ])
        )
    })
}
