//! Shared domain types, errors, filter state, and sorting rules.
//!
//! This crate stays pure. It does no I/O, no database work, and no GUI work.
//! Every other tarragon crate may depend on it. It depends on no local crate.
//!
//! `filter_state::FilterState`, `tier::Tier`, and `color::ColorBucket` repeat
//! their module names on purpose. Turn off the repetition lint for the whole
//! crate.
#![allow(clippy::module_name_repetitions)]

pub mod color;
pub mod entity;
pub mod error;
pub mod filter_state;
pub mod format;
pub mod paths;
pub mod render_scope;
pub mod sorting;
pub mod tier;

#[cfg(test)]
mod tests {
    #[test]
    fn package_version_has_three_numeric_parts() {
        let version: &str = env!("CARGO_PKG_VERSION");
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 3, "version is not x.y.z: {version}");
        for part in parts {
            assert!(
                part.parse::<u32>().is_ok(),
                "version part is not numeric: {part}"
            );
        }
    }
}
