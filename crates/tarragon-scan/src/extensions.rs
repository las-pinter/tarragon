//! The ten indexed image extensions.
//!
//! The set lives in `tarragon-core` so the scanner and the render registry
//! share one definition. This module re-exports it for scan callers.

pub use tarragon_core::format::{SUPPORTED_EXTENSIONS, is_supported};
