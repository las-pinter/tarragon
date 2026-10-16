//! PSD and PSB parsing, merged preview, and layer compositor.
//!
//! This crate is pure CPU work. It does not depend on the GUI or the database.
//!
//! The parser reads the Photoshop binary format directly. It does not use a
//! `psd` crate (Q9). The spike in [`parse`] reads the file header and the
//! first layer record. The full parser plan (section layout, the PSB 8-byte
//! length rule, tiled read, the 8192 px cap, and the 120 s deadline) is in the
//! crate `README.md`.

pub mod composite;
pub mod error;
pub mod merged;
pub mod parse;

pub use error::PsdError;
