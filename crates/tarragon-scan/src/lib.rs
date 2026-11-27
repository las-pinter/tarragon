//! Folder scanning and change detection for tarragon.
//!
//! The engine scans one folder at a time. The scan is non-recursive and it
//! indexes only the ten supported image extensions. Each kept file is stat'ed
//! for its size and mtime.
//!
//! The scan does not read pixel dimensions. The stored `width` and `height`
//! stay zero until a render fills them.
//!
//! The optional watcher reports a change in the open folder after a 500 ms
//! debounce. It watches only while a folder is open. A second watcher holds a
//! changing set of folders, so the sidebar counts follow changes; `count`
//! gives the number of supported images directly in one folder.
//!
//! This crate performs I/O. Among the tarragon crates it depends on
//! `tarragon-core` only.

pub mod count;
pub mod extensions;
pub mod scan;
pub mod watch;
pub mod watch_set;
