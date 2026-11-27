//! egui widgets and theme tokens for the tarragon interface.
//!
//! The crate draws with egui and never depends on eframe. The app crate
//! owns the eframe window and the repaint loop.

mod backdrop;
pub(crate) mod busy;
pub mod crash_card;
pub mod filter_bar;
mod fonts;
pub mod gallery;
pub mod grid;
pub mod layout;
mod popup;
pub mod prefs;
pub mod preview;
pub mod shell;
pub mod sidebar;
pub mod status;
pub mod swatch;
pub mod tag_manager;
#[cfg(test)]
pub(crate) mod test_support;
pub mod texture_budget;
pub mod theme;
pub mod tokens;
pub mod widgets;

pub use fonts::{FallbackFont, add_fallbacks, definitions, install};
pub use shell::Shell;
pub use theme::apply_theme;
