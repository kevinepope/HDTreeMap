//! Words for map items: the treemap's tooltip and the status bar's hovered / selected line
//! both describe a node (or a block of merged small items) the same way, from here.

use crate::format::{format_bytes, format_count, format_percent};
use crate::layout::AggregateInfo;
use crate::model::{NodeId, Tree};
use crate::state::Hover;

/// `"1,234 files"` / `"1 file"`.
pub fn files_phrase(count: u64) -> String {
    let noun = if count == 1 { "file" } else { "files" };
    format!("{} {noun}", format_count(count))
}

/// `"312 small items"` for an aggregate block.
fn small_items_phrase(info: AggregateInfo) -> String {
    let noun = if info.count == 1 { "item" } else { "items" };
    format!("{} small {noun}", format_count(u64::from(info.count)))
}

/// The display path of `node`: the full path (the scan root's name is the scanned path).
pub fn display_path(tree: &Tree, node: NodeId) -> String {
    tree.path(node).to_string_lossy().into_owned()
}

/// The hover tooltip: name, size and share of the zoom root, file count for folders, and
/// the full path.
pub fn tooltip(tree: &Tree, zoom_root: NodeId, hover: Hover) -> String {
    let whole = tree.size(zoom_root);
    let zoom_name = if zoom_root == tree.root() {
        display_path(tree, zoom_root)
    } else {
        tree.name(zoom_root).to_string()
    };
    match hover.aggregate {
        Some(info) => format!(
            "{}\n{} \u{b7} {} of {}\nin {}",
            small_items_phrase(info),
            format_bytes(info.bytes),
            format_percent(info.bytes, whole),
            zoom_name,
            display_path(tree, hover.node)
        ),
        None => {
            let node = hover.node;
            let size = tree.size(node);
            let mut text = format!(
                "{}\n{} \u{b7} {} of {}",
                tree.name(node),
                format_bytes(size),
                format_percent(size, whole),
                zoom_name
            );
            if let Some(n) = tree.node(node).filter(|n| n.is_dir()) {
                text.push('\n');
                text.push_str(&files_phrase(u64::from(n.file_count())));
            }
            text.push('\n');
            text.push_str(&display_path(tree, node));
            text
        }
    }
}

/// One status-bar line: `"<path> — 12.0 MB"`, with the item count for folders and
/// aggregates.
pub fn status_line(tree: &Tree, hover: Hover) -> String {
    let path = display_path(tree, hover.node);
    match hover.aggregate {
        Some(info) => format!(
            "{} in {path} \u{2014} {}",
            small_items_phrase(info),
            format_bytes(info.bytes)
        ),
        None => {
            let size = format_bytes(tree.size(hover.node));
            match tree.node(hover.node).filter(|n| n.is_dir()) {
                Some(n) => format!(
                    "{path} \u{2014} {size} in {}",
                    files_phrase(u64::from(n.file_count()))
                ),
                None => format!("{path} \u{2014} {size}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NodeFlags, SizeMode, Sizes, TreeBuilder};

    fn tree() -> (Tree, NodeId, NodeId) {
        let mut b = TreeBuilder::new("/scan");
        let root = b.root();
        let docs = b.add_dir(root, "docs");
        let a = b.add_file(docs, "a.pdf", Sizes::new(3072, 3000), NodeFlags::empty());
        b.add_file(root, "b.bin", Sizes::new(1024, 1000), NodeFlags::empty());
        (b.finish(SizeMode::Allocated), docs, a)
    }

    #[test]
    fn tooltips_name_size_share_and_path() {
        let (tree, docs, a) = tree();
        let file = tooltip(
            &tree,
            tree.root(),
            Hover {
                node: a,
                aggregate: None,
            },
        );
        assert_eq!(
            file,
            "a.pdf\n3.0 KB \u{b7} 75.0% of /scan\n/scan/docs/a.pdf"
        );
        let dir = tooltip(
            &tree,
            docs,
            Hover {
                node: docs,
                aggregate: None,
            },
        );
        assert_eq!(
            dir,
            "docs\n3.0 KB \u{b7} 100.0% of docs\n1 file\n/scan/docs"
        );
        let agg = Hover {
            node: docs,
            aggregate: Some(AggregateInfo {
                count: 12,
                bytes: 2048,
            }),
        };
        assert_eq!(
            tooltip(&tree, tree.root(), agg),
            "12 small items\n2.0 KB \u{b7} 50.0% of /scan\nin /scan/docs"
        );
    }

    #[test]
    fn status_lines_are_one_line() {
        let (tree, docs, a) = tree();
        assert_eq!(
            status_line(
                &tree,
                Hover {
                    node: a,
                    aggregate: None
                }
            ),
            "/scan/docs/a.pdf \u{2014} 3.0 KB"
        );
        assert_eq!(
            status_line(
                &tree,
                Hover {
                    node: docs,
                    aggregate: None
                }
            ),
            "/scan/docs \u{2014} 3.0 KB in 1 file"
        );
        let agg = Hover {
            node: docs,
            aggregate: Some(AggregateInfo {
                count: 1,
                bytes: 10,
            }),
        };
        assert_eq!(
            status_line(&tree, agg),
            "1 small item in /scan/docs \u{2014} 10 B"
        );
        assert_eq!(files_phrase(1_234), "1,234 files");
    }
}
