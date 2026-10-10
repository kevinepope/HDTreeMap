//! Recipes for the simulated drives: one module per profile, plus the building blocks they
//! share (this file) and the developer-folder recipes (`dev.rs`) several profiles reuse.
//!
//! A recipe is an `engine::Expand` function: given a directory's byte budget it returns the
//! directory's children, spending that budget (give or take a few small fixed-size files
//! such as `package.json`), so each drive's total comes out at the used space
//! `DemoPlatform` advertises. Sizes are drawn from log-normal distributions
//! (a few big files among many small ones, as on real disks), names from word lists and
//! numbering schemes that look like the real thing.

pub mod archive;
pub mod dev;
pub mod dev_build;
pub mod laptop;
pub mod laptop_home;
pub mod media;
pub mod small;
pub mod workstation;

use std::collections::HashSet;

use super::engine::{Entry, Expand};
use super::rng::Rng;

pub const KB: u64 = 1024;
pub const MB: u64 = 1024 * KB;
pub const GB: u64 = 1024 * MB;

pub fn file(name: impl Into<String>, bytes: u64) -> Entry {
    Entry::File {
        name: name.into(),
        bytes,
    }
}

pub fn dir(name: impl Into<String>, bytes: u64, expand: Expand, arg: u32) -> Entry {
    Entry::Dir {
        name: name.into(),
        bytes,
        expand,
        arg,
    }
}

pub fn locked(name: impl Into<String>) -> Entry {
    Entry::Locked { name: name.into() }
}

/// Split `bytes` in proportion to `weights`; the parts add up to exactly `bytes`.
pub fn shares(bytes: u64, weights: &[u64]) -> Vec<u64> {
    let total: u128 = weights.iter().map(|&w| u128::from(w)).sum();
    if total == 0 {
        let mut parts = vec![0; weights.len()];
        if let Some(last) = parts.last_mut() {
            *last = bytes;
        }
        return parts;
    }
    let mut parts: Vec<u64> = weights
        .iter()
        .map(|&w| (u128::from(bytes) * u128::from(w) / total) as u64)
        .collect();
    let spent: u64 = parts.iter().sum();
    // Rounding leftovers go to the largest share.
    if let Some(i) = (0..weights.len()).max_by_key(|&i| weights[i]) {
        parts[i] += bytes - spent;
    }
    parts
}

/// Split `bytes` into `n` random parts with log-normal spread `sigma` (0 = equal parts,
/// 1 = typical files, 2 = a few giants among many small ones). Exact sum.
pub fn split(rng: &mut Rng, bytes: u64, n: usize, sigma: f64) -> Vec<u64> {
    if n == 0 {
        return Vec::new();
    }
    let weights: Vec<u64> = (0..n)
        .map(|_| ((sigma * rng.gaussian()).exp() * 1_000_000.0) as u64 + 1)
        .collect();
    shares(bytes, &weights)
}

/// How many children a helper makes: a fixed number, or one drawn from `lo..=hi`.
#[derive(Clone, Copy, Debug)]
pub enum Count {
    Fixed(usize),
    Between(usize, usize),
}

impl Count {
    fn draw(self, rng: &mut Rng) -> usize {
        match self {
            Count::Fixed(n) => n,
            Count::Between(lo, hi) => rng.range_usize(lo, hi),
        }
    }
}

impl From<usize> for Count {
    fn from(n: usize) -> Self {
        Count::Fixed(n)
    }
}

impl From<(usize, usize)> for Count {
    fn from((lo, hi): (usize, usize)) -> Self {
        Count::Between(lo, hi)
    }
}

/// `count` files sharing `bytes` (spread `sigma`), named by `namer(rng, index)`. Names are
/// made unique within the batch by numbering repeats, as a file manager would.
pub fn files(
    rng: &mut Rng,
    bytes: u64,
    count: impl Into<Count>,
    sigma: f64,
    mut namer: impl FnMut(&mut Rng, usize) -> String,
) -> Vec<Entry> {
    let count = count.into().draw(rng);
    let sizes = split(rng, bytes, count, sigma);
    let mut seen = HashSet::with_capacity(count);
    sizes
        .into_iter()
        .enumerate()
        .map(|(i, size)| {
            let name = unique(&mut seen, namer(rng, i));
            file(name, size)
        })
        .collect()
}

/// `name`, or `name 2`, `name 3`, ... (before the extension) if already in `seen`.
pub fn unique(seen: &mut HashSet<String>, name: String) -> String {
    if seen.insert(name.clone()) {
        return name;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.clone(), String::new()),
    };
    let mut n = 2;
    loop {
        let candidate = format!("{stem} {n}{ext}");
        if seen.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

/// Fixed children: `(name, weight, expand, arg)` directories sharing `bytes` by weight.
pub fn layout(bytes: u64, parts: &[(&str, u64, Expand, u32)]) -> Vec<Entry> {
    let weights: Vec<u64> = parts.iter().map(|p| p.1).collect();
    parts
        .iter()
        .zip(shares(bytes, &weights))
        .map(|(&(name, _, expand, arg), size)| dir(name, size, expand, arg))
        .collect()
}

/// Fixed files: `(name, weight)` sharing `bytes` by weight.
pub fn named_files(bytes: u64, parts: &[(&str, u64)]) -> Vec<Entry> {
    let weights: Vec<u64> = parts.iter().map(|p| p.1).collect();
    parts
        .iter()
        .zip(shares(bytes, &weights))
        .map(|(&(name, _), size)| file(name, size))
        .collect()
}

/// `count` subdirectories sharing `bytes` (spread `sigma`), each expanded by `expand` with
/// `arg`, named by `namer(rng, index)` (made unique).
pub fn subdirs(
    rng: &mut Rng,
    bytes: u64,
    count: impl Into<Count>,
    sigma: f64,
    expand: Expand,
    arg: u32,
    mut namer: impl FnMut(&mut Rng, usize) -> String,
) -> Vec<Entry> {
    let count = count.into().draw(rng);
    let sizes = split(rng, bytes, count, sigma);
    let mut seen = HashSet::with_capacity(count);
    sizes
        .into_iter()
        .enumerate()
        .map(|(i, size)| {
            let name = unique(&mut seen, namer(rng, i));
            dir(name, size, expand, arg)
        })
        .collect()
}

/// A random document-ish file name: `"Budget 2023.xlsx"`, `"meeting-notes_v2.docx"`.
pub fn document_name(rng: &mut Rng, exts: &[(&str, u32)]) -> String {
    let word = rng.pick(DOC_WORDS);
    let ext = rng.weighted(exts);
    match rng.below(5) {
        0 => format!("{word} {}.{ext}", rng.range(2014, 2025)),
        1 => format!("{word}-{}_v{}.{ext}", rng.pick(DOC_WORDS), rng.range(1, 4)),
        2 => format!("{word} ({}).{ext}", rng.pick(MONTHS)),
        3 => format!("{word}_final.{ext}"),
        _ => format!("{word}.{ext}"),
    }
}

/// A camera-style photo name: `IMG_4821.JPG`.
pub fn photo_name(prefix: &str, index: usize, ext: &str) -> String {
    format!("{prefix}{:04}.{ext}", index % 10_000)
}

pub const MONTHS: &[&str] = &[
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub const DOC_WORDS: &[&str] = &[
    "Report",
    "Invoice",
    "Budget",
    "Notes",
    "Meeting notes",
    "Presentation",
    "Resume",
    "Contract",
    "Summary",
    "Project plan",
    "Draft",
    "Proposal",
    "Receipt",
    "Bank statement",
    "Itinerary",
    "Manual",
    "Thesis",
    "Cover letter",
    "Schedule",
    "Agenda",
    "Tax return",
    "Lease",
    "Insurance",
    "Recipe",
    "Quote",
    "Roadmap",
    "Specs",
    "Minutes",
    "Checklist",
    "Floor plan",
];

/// Office documents by how common they are.
pub const OFFICE_EXTS: &[(&str, u32)] = &[
    ("pdf", 40),
    ("docx", 20),
    ("xlsx", 12),
    ("pptx", 6),
    ("txt", 8),
    ("md", 4),
    ("pages", 3),
    ("numbers", 2),
    ("key", 2),
    ("odt", 1),
    ("csv", 4),
    ("rtf", 1),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_and_splits_spend_exactly_the_budget() {
        assert_eq!(shares(100, &[1, 1, 1]).iter().sum::<u64>(), 100);
        assert_eq!(shares(10, &[0, 0]), vec![0, 10]);
        assert_eq!(shares(7, &[]), Vec::<u64>::new());
        let mut rng = Rng::new(4);
        for sigma in [0.0, 1.0, 2.5] {
            let parts = split(&mut rng, 123_456_789, 37, sigma);
            assert_eq!(parts.len(), 37);
            assert_eq!(parts.iter().sum::<u64>(), 123_456_789);
        }
        assert!(split(&mut rng, 5, 0, 1.0).is_empty());
    }

    #[test]
    fn repeated_names_are_numbered() {
        let mut seen = HashSet::new();
        assert_eq!(unique(&mut seen, "a.pdf".into()), "a.pdf");
        assert_eq!(unique(&mut seen, "a.pdf".into()), "a 2.pdf");
        assert_eq!(unique(&mut seen, "a.pdf".into()), "a 3.pdf");
        assert_eq!(unique(&mut seen, "Makefile".into()), "Makefile");
        assert_eq!(unique(&mut seen, "Makefile".into()), "Makefile 2");
    }
}
