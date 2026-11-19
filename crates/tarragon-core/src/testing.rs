//! Helpers for tests in every tarragon crate.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// A short tag for temp names that differs between processes: the process id
/// and the low 32 bits of the first moment this process asked for the tag, in
/// hex. The tag stays short because some tests put a socket under it.
///
/// A test runner that starts one process per test can reuse a process id.
/// A name made from the id and an in-process counter alone can then meet a
/// folder that an earlier process left behind.
#[must_use]
pub fn run_tag() -> &'static str {
    static TAG: OnceLock<String> = OnceLock::new();
    TAG.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        // Keep the low bits on purpose: they change fastest.
        #[allow(clippy::cast_possible_truncation)]
        let low = nanos as u32;
        format!("{}-{low:08x}", std::process::id())
    })
}

#[cfg(test)]
mod tests {
    use super::run_tag;

    #[test]
    fn the_tag_starts_with_the_pid_and_stays_the_same() {
        let tag = run_tag();
        assert!(
            tag.starts_with(&format!("{}-", std::process::id())),
            "{tag}"
        );
        assert!(
            tag.len() > format!("{}-", std::process::id()).len(),
            "{tag}"
        );
        assert_eq!(run_tag(), tag);
    }
}
