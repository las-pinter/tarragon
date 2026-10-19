//! Tests for the schema, repositories, filter engine, and settings.
//!
//! Every test uses a temporary database file. WAL needs a file, so the tests
//! do not use an in-memory database.

mod filter;
mod repository;
mod settings;
mod support;
