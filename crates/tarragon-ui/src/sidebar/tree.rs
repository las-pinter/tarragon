//! Folder tree model: which rows the sidebar shows, with no drawing or disk access.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// One visible row of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// The nesting level; a root is 0.
    pub depth: usize,
    /// The folder path.
    pub path: PathBuf,
    /// The last path component, or the whole path when it has none.
    pub name: String,
    /// Whether the folder may have children to show.
    pub has_caret: bool,
    /// Whether the children are shown.
    pub expanded: bool,
}

/// The state of one folder.
#[derive(Clone, Debug, Default)]
struct Node {
    /// The child paths in lister order, or none until the folder is listed.
    children: Option<Vec<PathBuf>>,
    /// Whether the user opened the folder.
    expanded: bool,
}

/// The folder tree: roots, listed children, and expanded flags.
#[derive(Clone, Debug, Default)]
pub struct TreeModel {
    /// The roots in sidebar order.
    roots: Vec<PathBuf>,
    /// The state of every folder reachable from a root.
    nodes: HashMap<PathBuf, Node>,
}

impl TreeModel {
    /// Replace the roots; a folder still reachable keeps its state.
    pub fn set_roots(&mut self, roots: &[PathBuf]) {
        self.roots = roots.to_vec();
        for root in roots {
            self.nodes.entry(root.clone()).or_default();
        }
        self.sweep();
    }

    /// Store the listed children of a known folder.
    ///
    /// Only paths strictly below the folder count, so the tree has no cycle,
    /// and a repeated path counts once. A folder that stays reachable keeps
    /// its state, so a refresh does not close open folders. A folder that is
    /// no longer reachable is forgotten.
    pub fn set_children(&mut self, path: &Path, children: Vec<PathBuf>) {
        if !self.nodes.contains_key(path) {
            return;
        }
        let mut kept: Vec<PathBuf> = Vec::new();
        for child in children {
            if child.starts_with(path) && child != path && !kept.contains(&child) {
                kept.push(child);
            }
        }
        for child in &kept {
            self.nodes.entry(child.clone()).or_default();
        }
        if let Some(node) = self.nodes.get_mut(path) {
            node.children = Some(kept);
        }
        self.sweep();
    }

    /// Open or close a known folder.
    pub fn set_expanded(&mut self, path: &Path, expanded: bool) {
        if let Some(node) = self.nodes.get_mut(path) {
            node.expanded = expanded;
        }
    }

    /// Flip a known folder between open and closed.
    pub fn toggle(&mut self, path: &Path) {
        if let Some(node) = self.nodes.get(path) {
            self.set_expanded(path, !node.expanded);
        }
    }

    /// The visible rows in draw order; a closed folder hides its subtree.
    ///
    /// It allocates on each call, so callers should cache the result. A path
    /// under two parents shows twice with one shared expanded flag, so the UI
    /// must not use the path alone as a widget id.
    #[must_use]
    pub fn rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        for root in &self.roots {
            self.push_rows(root, 0, &mut rows);
        }
        rows
    }

    /// Append a folder row, then its children when the folder is open.
    fn push_rows(&self, path: &Path, depth: usize, rows: &mut Vec<TreeRow>) {
        let Some(node) = self.nodes.get(path) else {
            return;
        };
        let name = path.file_name().map_or_else(
            || path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        rows.push(TreeRow {
            depth,
            path: path.to_path_buf(),
            name,
            has_caret: node.children.as_ref().is_none_or(|kids| !kids.is_empty()),
            expanded: node.expanded,
        });
        if node.expanded {
            for child in node.children.iter().flatten() {
                self.push_rows(child, depth + 1, rows);
            }
        }
    }

    /// Drop every folder that no root reaches.
    fn sweep(&mut self) {
        let mut seen: HashSet<&Path> = HashSet::new();
        let mut stack: Vec<&Path> = self.roots.iter().map(PathBuf::as_path).collect();
        while let Some(path) = stack.pop() {
            if !seen.insert(path) {
                continue;
            }
            if let Some(node) = self.nodes.get(path) {
                stack.extend(node.children.iter().flatten().map(PathBuf::as_path));
            }
        }
        let keep: HashSet<PathBuf> = seen.into_iter().map(Path::to_path_buf).collect();
        self.nodes.retain(|path, _| keep.contains(path));
    }
}
