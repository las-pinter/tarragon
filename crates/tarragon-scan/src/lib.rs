//! Folder scanning and change detection for tarragon.
//!
//! The engine scans one folder at a time. The scan is non-recursive and it
//! indexes only the ten image extensions from Q26. Each kept file is stat'ed
//! for its size and mtime.
//!
//! The scan does not read pixel dimensions. The stored `width` and `height`
//! stay zero until a render fills them, which matches the Python scan (Q1).
//!
//! The optional watcher reports a change in the open folder after a 500 ms
//! debounce (Q26). It watches only while a folder is open.
//!
//! This crate performs I/O. Among the tarragon crates it depends on
//! `tarragon-core` only (Q16).

pub mod extensions;
pub mod scan;
pub mod watch;
