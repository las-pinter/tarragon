//! Renderers for tarragon: plain images, CLIP files, and Krita documents.
//!
//! This crate is pure CPU work. It does not depend on the GUI or the database.

pub mod clip;
pub mod krita;
pub mod plain;
pub mod registry;
pub mod renderer;
