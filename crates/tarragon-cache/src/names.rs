//! Cache names that a Windows path can hold.
//!
//! Windows refuses some characters, trailing dots and spaces, and the device
//! names. The same rule runs on every platform so the cache layout is the same
//! everywhere.

/// The device names that Windows reserves as base names.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The FNV-1a 64-bit hash. It is written out here because the standard
/// hashers may change between Rust releases and two builds must agree.
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

/// Return `name` with every Windows-illegal character and control character
/// replaced by `_`, trailing dots and spaces removed, and a `_` prefix when the
/// part before the first dot is a reserved device name.
///
/// An empty result becomes `_`. Distinct names can give the same result, so use
/// [`portable`] for a file name.
#[must_use]
pub(crate) fn sanitize(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || u32::from(c) < 0x20 {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = replaced.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        return "_".to_owned();
    }
    let base = trimmed.split('.').next().unwrap_or_default();
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(base)) {
        format!("_{trimmed}")
    } else {
        trimmed.to_owned()
    }
}

/// Return `name` when it is already portable, else the sanitized text, a `-`,
/// and 8 lower-case hex digits of the hash of the original name.
///
/// The hash keeps two names that clean to the same text apart.
#[must_use]
pub(crate) fn portable(name: &str) -> String {
    let clean = sanitize(name);
    if clean == name {
        return clean;
    }
    let hash = fnv1a64(name.as_bytes()) & 0xffff_ffff;
    format!("{clean}-{hash:08x}")
}

#[cfg(test)]
mod tests;
