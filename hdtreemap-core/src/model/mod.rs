//! The scanned-tree data model (`docs/PLAN.md` §3.1).
//!
//! - [`node`] — the 40-byte [`Node`], [`NodeId`], [`NodeFlags`], [`SizeMode`], [`Sizes`].
//! - [`tree`] — the arena [`Tree`]: names, children, paths, size-mode re-sort,
//!   [`Tree::remove_subtree`], largest folders / files.
//! - [`builder`] — [`TreeBuilder`], which both scan sources feed while scanning.
//! - [`ext`] — [`ExtTable`], totals per extension.

pub mod builder;
pub mod ext;
pub mod node;
pub mod tree;

pub use builder::TreeBuilder;
pub use ext::{ExtRow, ExtTable};
pub use node::{extension_of, Node, NodeFlags, NodeId, SizeMode, Sizes, NO_EXT};
pub use tree::{Children, RemoveError, Tree, NAME_SLAB_LIMIT};

#[cfg(test)]
mod tests;
