//! Group routed files into process starts.
//!
//! The code is pure: it starts no process.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::exec::{ExecLine, ListKind};
use super::{Launch, Route};

/// The most paths one start takes.
const MAX_PATHS: usize = 500;
/// The most bytes of path text one start takes, counted on the raw paths and
/// not on the built arguments. Linux limits the whole argument list, so a byte
/// cap is safer than a count alone.
const MAX_PATH_BYTES: usize = 100_000;

/// One process start and the files it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    /// The program and arguments to run.
    pub launch: Launch,
    /// The files this start opens, so a failed start can count them.
    pub paths: Vec<PathBuf>,
}

/// The files of one template, in selection order.
struct Group {
    /// The parsed line of the first file of the group.
    line: ExecLine,
    /// The files that share the template.
    paths: Vec<PathBuf>,
}

/// Where a start (or a run of starts) sits in the result.
enum Slot {
    /// A file that has its own start.
    Single(PathBuf, Launch),
    /// The starts of the group at this index.
    Group(usize),
}

/// Turn one route per file into the list of process starts.
///
/// Files with the same list key form one group. A `Single` route is one start.
/// Each start sits where its first file was first seen, so groups and single
/// starts keep their relative order, and the starts of one group stay together.
/// A file that comes twice is planned once.
///
/// A group becomes one start, or several when it holds too many files or too
/// much path text. On a one-file line it is one start per file.
pub fn plan(routes: Vec<(PathBuf, Route)>) -> Vec<Planned> {
    let mut seen = HashSet::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut slots = Vec::new();
    for (path, route) in routes {
        if !seen.insert(path.clone()) {
            continue;
        }
        match route {
            Route::Single(launch) => slots.push(Slot::Single(path, launch)),
            Route::List { key, line } => {
                if let Some(&at) = index.get(&key) {
                    groups[at].paths.push(path);
                } else {
                    index.insert(key, groups.len());
                    slots.push(Slot::Group(groups.len()));
                    groups.push(Group {
                        line,
                        paths: vec![path],
                    });
                }
            }
        }
    }
    let mut out = Vec::new();
    for slot in slots {
        match slot {
            Slot::Single(path, launch) => out.push(Planned {
                launch,
                paths: vec![path],
            }),
            Slot::Group(at) => plan_group(&groups[at], &mut out),
        }
    }
    out
}

/// Add the starts of one group to `out`.
fn plan_group(group: &Group, out: &mut Vec<Planned>) {
    let chunks = match group.line.list {
        ListKind::One => group.paths.chunks(1).map(<[PathBuf]>::to_vec).collect(),
        ListKind::Many => chunk(&group.paths),
    };
    for paths in chunks {
        out.push(Planned {
            launch: group.line.launch(&paths),
            paths,
        });
    }
}

/// Split `paths` into runs within the count and byte caps. A path is never
/// split, so a single long path still gets a run of its own.
fn chunk(paths: &[PathBuf]) -> Vec<Vec<PathBuf>> {
    let mut chunks = Vec::new();
    let mut current: Vec<PathBuf> = Vec::new();
    let mut bytes = 0;
    for path in paths {
        let size = path.as_os_str().len();
        if !current.is_empty() && (current.len() + 1 > MAX_PATHS || bytes + size > MAX_PATH_BYTES) {
            chunks.push(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes += size;
        current.push(path.clone());
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests;
