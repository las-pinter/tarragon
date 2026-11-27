//! Parts of the filter bar.

mod action;
mod bar;
pub mod chip;
pub mod colors;
pub mod folders;
mod list;
pub mod menu;
pub mod tags;

pub use action::{CLEAR_TEXT, FOLDERS_TEXT, TAGS_TEXT};
pub use bar::show;
pub use list::Shown;
