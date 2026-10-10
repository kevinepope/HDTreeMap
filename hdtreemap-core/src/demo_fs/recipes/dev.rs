//! Developer-folder recipes shared by the simulated drives: source projects and their
//! `node_modules` (thousands of small `.js` / `.json` / `.map` / `.d.ts` files) and sources.
//! The build and tooling folders a project carries (Rust `target/`, `.git`, Python `.venv`,
//! toolchains) are in `dev_build.rs` and re-exported here.

use super::dev_build::{data_dir, notebooks, venv};
pub use super::dev_build::{git_dir, rust_target, toolchain};
use super::{dir, file, files, shares, split, subdirs, unique, Entry, KB};
use crate::demo_fs::rng::Rng;
use std::collections::HashSet;

/// Project flavours (`arg` of [`project`]).
pub const JS: u32 = 0;
pub const RUST: u32 = 1;
pub const PYTHON: u32 = 2;

pub(super) const PROJECT_WORDS: &[&str] = &[
    "atlas",
    "beacon",
    "cobalt",
    "delta",
    "ember",
    "falcon",
    "glacier",
    "harbor",
    "ion",
    "juniper",
    "kestrel",
    "lumen",
    "meridian",
    "nimbus",
    "orbit",
    "pylon",
    "quartz",
    "relay",
    "summit",
    "tundra",
    "umbra",
    "vertex",
    "willow",
    "xenon",
    "yarrow",
    "zephyr",
    "api",
    "web",
    "dashboard",
    "cli",
    "sdk",
    "service",
    "worker",
    "admin",
    "mobile",
    "engine",
    "parser",
    "renderer",
    "bot",
    "infra",
];

const NPM_PACKAGES: &[&str] = &[
    "react",
    "react-dom",
    "lodash",
    "typescript",
    "webpack",
    "babel-loader",
    "eslint",
    "prettier",
    "vite",
    "rollup",
    "esbuild",
    "postcss",
    "tailwindcss",
    "autoprefixer",
    "express",
    "axios",
    "chalk",
    "commander",
    "debug",
    "ms",
    "semver",
    "uuid",
    "yargs",
    "glob",
    "minimatch",
    "rimraf",
    "mkdirp",
    "fs-extra",
    "graceful-fs",
    "source-map",
    "source-map-js",
    "acorn",
    "terser",
    "browserslist",
    "caniuse-lite",
    "electron-to-chromium",
    "core-js",
    "regenerator-runtime",
    "tslib",
    "rxjs",
    "zone.js",
    "jest",
    "jest-circus",
    "jest-snapshot",
    "pretty-format",
    "ansi-styles",
    "supports-color",
    "color-convert",
    "color-name",
    "escape-string-regexp",
    "picomatch",
    "micromatch",
    "braces",
    "fill-range",
    "to-regex-range",
    "is-number",
    "fast-glob",
    "chokidar",
    "readdirp",
    "anymatch",
    "picocolors",
    "nanoid",
    "postcss-value-parser",
    "cssnano",
    "sass",
    "less",
    "styled-components",
    "emotion",
    "next",
    "vue",
    "svelte",
    "three",
    "d3",
    "moment",
    "date-fns",
    "dayjs",
    "zod",
    "yup",
    "formik",
    "redux",
    "react-redux",
    "immer",
    "mobx",
    "graphql",
    "apollo-client",
    "prisma",
    "sequelize",
    "mongoose",
    "pg",
    "mysql2",
    "sqlite3",
    "ws",
    "socket.io",
    "jsonwebtoken",
    "bcrypt",
    "dotenv",
    "cors",
    "body-parser",
    "cookie-parser",
    "multer",
    "sharp",
    "jimp",
    "puppeteer",
    "playwright-core",
    "cypress",
    "mocha",
    "chai",
    "sinon",
    "nyc",
    "istanbul-lib-coverage",
    "husky",
    "lint-staged",
    "concurrently",
    "nodemon",
    "ts-node",
    "tsx",
    "esbuild-register",
    "workbox-core",
    "firebase",
    "aws-sdk",
    "@aws-sdk/client-s3",
    "googleapis",
    "stripe",
    "openai",
    "marked",
    "highlight.js",
];

const NPM_SCOPES: &[&str] = &[
    "@types",
    "@babel",
    "@jest",
    "@eslint",
    "@typescript-eslint",
    "@rollup",
    "@esbuild",
    "@vitejs",
    "@emotion",
    "@mui",
    "@radix-ui",
    "@tanstack",
    "@sentry",
    "@swc",
];

pub(super) const CRATES: &[&str] = &[
    "serde",
    "serde_json",
    "serde_derive",
    "tokio",
    "tokio-macros",
    "hyper",
    "reqwest",
    "rand",
    "rand_core",
    "rand_chacha",
    "regex",
    "regex-syntax",
    "aho-corasick",
    "memchr",
    "syn",
    "quote",
    "proc-macro2",
    "unicode-ident",
    "libc",
    "cfg-if",
    "log",
    "env_logger",
    "anyhow",
    "thiserror",
    "clap",
    "clap_derive",
    "clap_lex",
    "bitflags",
    "bytes",
    "futures",
    "futures-util",
    "futures-core",
    "mio",
    "socket2",
    "parking_lot",
    "lock_api",
    "smallvec",
    "once_cell",
    "itoa",
    "ryu",
    "hashbrown",
    "indexmap",
    "crossbeam-utils",
    "rayon",
    "rayon-core",
    "wgpu",
    "wgpu-core",
    "naga",
    "winit",
    "image",
    "png",
    "flate2",
    "miniz_oxide",
    "sha2",
    "digest",
    "generic-array",
    "typenum",
    "chrono",
    "time",
    "uuid",
    "url",
    "idna",
    "percent-encoding",
    "tracing",
    "tracing-core",
    "tracing-subscriber",
    "sqlx",
    "diesel",
    "axum",
    "tower",
    "tower-http",
    "http",
    "h2",
    "rustls",
    "ring",
    "webpki",
    "base64",
];

/// `count` projects of mixed flavours sharing `bytes`.
pub fn projects(rng: &mut Rng, bytes: u64, count: u32) -> Vec<Entry> {
    let sizes = split(rng, bytes, count as usize, 1.0);
    let mut seen = HashSet::new();
    sizes
        .into_iter()
        .map(|size| {
            let name = match rng.below(3) {
                0 => rng.pick(PROJECT_WORDS).to_string(),
                1 => format!("{}-{}", rng.pick(PROJECT_WORDS), rng.pick(PROJECT_WORDS)),
                _ => format!("{}{}", rng.pick(PROJECT_WORDS), rng.range(2, 9)),
            };
            let flavour = *rng.weighted(&[(JS, 5), (RUST, 3), (PYTHON, 2)]);
            dir(unique(&mut seen, name), size, project, flavour)
        })
        .collect()
}

/// One source project (`arg`: [`JS`], [`RUST`] or [`PYTHON`]).
pub fn project(rng: &mut Rng, bytes: u64, flavour: u32) -> Vec<Entry> {
    let mut out = match flavour {
        RUST => {
            let parts = shares(bytes, &[80, 9, 4, 1]);
            vec![
                dir("target", parts[0], rust_target, 0),
                dir(".git", parts[1], git_dir, 0),
                dir("src", parts[2], source_dir, 1),
                file("Cargo.lock", parts[3] / 2),
                file("Cargo.toml", 2 * KB),
                file("README.md", parts[3] - parts[3] / 2),
            ]
        }
        PYTHON => {
            let parts = shares(bytes, &[45, 30, 10, 8, 2]);
            vec![
                dir(".venv", parts[0], venv, 0),
                dir("data", parts[1], data_dir, 0),
                dir(".git", parts[2], git_dir, 0),
                dir("notebooks", parts[3], notebooks, 0),
                dir("src", parts[4], source_dir, 2),
                file("requirements.txt", KB),
                file("pyproject.toml", 2 * KB),
            ]
        }
        _ => {
            let parts = shares(bytes, &[72, 12, 6, 6, 4]);
            vec![
                dir(
                    "node_modules",
                    parts[0],
                    node_modules,
                    rng.range(150, 900) as u32,
                ),
                dir(".git", parts[1], git_dir, 0),
                dir("src", parts[2], source_dir, 0),
                dir("dist", parts[3], dist_dir, 0),
                file("package-lock.json", parts[4]),
                file("package.json", 3 * KB),
                file("tsconfig.json", KB),
                file("README.md", 6 * KB),
                file(".gitignore", 200),
            ]
        }
    };
    if rng.chance(0.3) {
        out.push(file(".DS_Store", 6 * KB));
    }
    out
}

/// `node_modules` with about `packages` packages, some under `@scope` folders.
pub fn node_modules(rng: &mut Rng, bytes: u64, packages: u32) -> Vec<Entry> {
    let scopes = rng.range_usize(2, 6);
    let scoped_share = bytes / 6;
    let mut out = subdirs(
        rng,
        bytes - scoped_share,
        packages as usize,
        1.4,
        npm_package,
        0,
        |rng, i| {
            let base = rng.pick(NPM_PACKAGES);
            if i < NPM_PACKAGES.len() {
                base.to_string()
            } else {
                format!(
                    "{base}-{}",
                    rng.pick(&["utils", "core", "plugin", "loader", "compat", "lite"])
                )
            }
        },
    );
    out.extend(subdirs(
        rng,
        scoped_share,
        scopes,
        0.8,
        npm_scope,
        0,
        |rng, _| rng.pick(NPM_SCOPES).to_string(),
    ));
    out.push(dir(".bin", 0, empty, 0));
    out.push(file(".package-lock.json", rng.range(200, 900) * KB));
    out
}

fn npm_scope(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(2, 25);
    subdirs(rng, bytes, n, 1.0, npm_package, 0, |rng, _| {
        rng.pick(NPM_PACKAGES)
            .trim_start_matches('@')
            .replace('/', "-")
    })
}

/// One npm package: manifest, readme, licence, entry point and a `lib`/`dist` folder.
fn npm_package(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let parts = shares(bytes, &[3, 4, 1, 4, 88]);
    let code_dir = *rng.pick(&["lib", "dist", "build", "cjs", "esm"]);
    let mut out = vec![
        file("package.json", parts[0]),
        file("README.md", parts[1]),
        file("LICENSE", parts[2]),
        file("index.js", parts[3]),
        dir(code_dir, parts[4], npm_code, 0),
    ];
    if rng.chance(0.4) {
        out.push(file("CHANGELOG.md", rng.range(2, 60) * KB));
    }
    if rng.chance(0.25) {
        out.push(file("index.d.ts", rng.range(1, 30) * KB));
    }
    out
}

fn npm_code(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = if rng.chance(0.05) {
        rng.range_usize(80, 260)
    } else {
        rng.range_usize(3, 30)
    };
    files(rng, bytes, n, 1.1, |rng, i| {
        let stem = *rng.pick(&[
            "index",
            "utils",
            "parser",
            "helpers",
            "types",
            "core",
            "compat",
            "runtime",
            "plugin",
            "loader",
            "constants",
            "errors",
            "stream",
            "client",
            "server",
            "config",
            "cache",
        ]);
        let ext = rng.weighted(&[
            ("js", 40),
            ("d.ts", 20),
            ("js.map", 18),
            ("json", 4),
            ("mjs", 8),
            ("cjs", 6),
            ("ts", 4),
        ]);
        format!(
            "{stem}{}.{ext}",
            if i < 17 {
                String::new()
            } else {
                format!("-{i}")
            }
        )
    })
}

pub(super) fn empty(_rng: &mut Rng, _bytes: u64, _arg: u32) -> Vec<Entry> {
    Vec::new()
}

/// `src/`: `.ts`/`.tsx` (arg 0), `.rs` (1) or `.py` (2) sources in a couple of folders.
fn source_dir(rng: &mut Rng, bytes: u64, lang: u32) -> Vec<Entry> {
    let (exts, words): (&[(&str, u32)], &[&str]) = match lang {
        1 => (
            &[("rs", 1)],
            &[
                "main", "lib", "app", "config", "model", "view", "parser", "error", "util",
                "state", "render", "net",
            ],
        ),
        2 => (
            &[("py", 1)],
            &[
                "main", "train", "model", "dataset", "utils", "config", "evaluate", "features",
                "cli", "api",
            ],
        ),
        _ => (
            &[("ts", 5), ("tsx", 4), ("css", 1)],
            &[
                "App", "index", "Header", "Button", "api", "store", "hooks", "utils", "routes",
                "Modal", "Table", "theme",
            ],
        ),
    };
    let parts = shares(bytes, &[60, 40]);
    let mut out = files(rng, parts[0], (4, 20), 0.8, |rng, _| {
        format!("{}.{}", rng.pick(words), rng.weighted(exts))
    });
    let sub = *rng.pick(&["components", "modules", "core", "services", "widgets"]);
    out.push(dir(sub, parts[1], leaf_sources, lang));
    out
}

fn leaf_sources(rng: &mut Rng, bytes: u64, lang: u32) -> Vec<Entry> {
    let ext = match lang {
        1 => "rs",
        2 => "py",
        _ => "tsx",
    };
    files(rng, bytes, (3, 25), 0.8, |rng, _| {
        format!("{}{}.{ext}", rng.pick(PROJECT_WORDS), rng.range(1, 9))
    })
}

fn dist_dir(rng: &mut Rng, bytes: u64, _arg: u32) -> Vec<Entry> {
    let n = rng.range_usize(4, 14);
    files(rng, bytes, n, 1.2, |rng, _| {
        let ext = rng.weighted(&[
            ("js", 4),
            ("js.map", 3),
            ("css", 1),
            ("html", 1),
            ("woff2", 1),
            ("svg", 1),
        ]);
        format!(
            "{}-{}.{ext}",
            rng.pick(&["main", "vendor", "chunk", "runtime", "polyfills"]),
            rng.hex(8)
        )
    })
}
