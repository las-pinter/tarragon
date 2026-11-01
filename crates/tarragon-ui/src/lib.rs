//! egui widgets and theme tokens for the tarragon interface.
//!
//! The crate depends on egui only. It never depends on eframe. The app crate
//! owns the eframe window and the repaint loop.

pub mod filter_bar;
mod fonts;
pub mod gallery;
pub mod grid;
pub mod layout;
mod popup;
pub mod preview;
pub mod shell;
pub mod sidebar;
pub mod swatch;
#[cfg(test)]
pub(crate) mod test_support;
pub mod texture_budget;
pub mod theme;
pub mod tokens;
pub mod widgets;

pub use fonts::{definitions, install};
pub use shell::Shell;
pub use theme::apply_theme;
