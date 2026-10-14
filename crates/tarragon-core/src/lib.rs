//! Shared domain types, paths, errors, filter state, and sorting rules.
//!
//! This crate stays pure. It does no I/O and does not depend on the GUI.

pub mod error;
pub mod filter_state;
pub mod paths;
pub mod sorting;

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
