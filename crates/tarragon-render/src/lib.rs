//! Renderers for tarragon: plain images, CLIP files, and Krita documents.
//!
//! This crate is pure CPU work. It does not depend on the GUI or the database.
//!
//! The [`registry`] module owns the format dispatch and the render extension
//! set. A render call returns a `DynamicImage` with the EXIF orientation
//! applied. The UI converts it to RGBA8 with `to_rgba8()` at texture upload.
//! The crate never depends on egui.
//!
//! The `color` module owns the color-tag computation.

pub mod clip;
pub mod color;
pub mod krita;
pub mod plain;
pub mod registry;
pub mod renderer;
