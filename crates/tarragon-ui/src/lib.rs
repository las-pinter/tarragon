//! egui widgets and theme tokens for the tarragon interface.
//!
//! The crate depends on egui only. It never depends on eframe. The app crate
//! owns the eframe window and the repaint loop.

mod fonts;
pub mod gallery;
pub mod grid;
pub mod layout;
pub mod shell;
pub mod sidebar;
pub mod texture_budget;
pub mod theme;
pub mod tokens;
pub mod widgets;

pub use fonts::{definitions, install};
pub use shell::Shell;
pub use theme::apply_theme;
