//! Folder scanning and change detection for tarragon.
//!
//! The engine scans one folder at a time. The scan is non-recursive and it
//! indexes only the ten image extensions from Q26. Each kept file is stat'ed
//! for its size and mtime.
//!
//! The scan does not read pixel dimensions. The stored `width` and `height`
//! stay zero until a render fills them, which matches the Python scan (Q1).
//!
//! This crate performs I/O. It depends on `tarragon-core` only (Q16).

pub mod extensions;
pub mod scan;
