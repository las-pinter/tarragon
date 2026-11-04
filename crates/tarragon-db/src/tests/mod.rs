//! Tests for the schema, repositories, filter engine, and settings.
//!
//! Every test uses a temporary database file. WAL needs a file, so the tests
//! do not use an in-memory database.

mod auto_colored;
mod favorites;
mod filter;
mod filter_folder;
mod folder;
mod folder_listing;
mod library;
mod repository;
mod settings;
mod support;
mod tag_rename;
mod window_state;
