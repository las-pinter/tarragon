//! The panic guard for render calls.
//!
//! A renderer reads files that other programs wrote. A bug in a decoder can
//! panic on a bad file. The guard turns that panic into a normal error, so one
//! bad file does not stop the worker thread or the app.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use tarragon_core::error::RenderError;
use tarragon_core::render_scope::RenderGuardScope;

/// The message used when the panic payload is not text.
const NON_STRING_PAYLOAD: &str = "non-string panic payload";

/// Run `render` and turn a panic into [`RenderError::Panicked`].
///
/// While `render` runs, the thread is marked as inside the guard. The panic
/// hook reads the mark and skips the crash file, because the app goes on.
///
/// # Errors
///
/// Returns the error of `render` unchanged. Returns [`RenderError::Panicked`]
/// with `path` and the panic message when `render` panics.
pub fn run_guarded<T>(
    path: &Path,
    render: impl FnOnce() -> Result<T, RenderError>,
) -> Result<T, RenderError> {
    let scope = RenderGuardScope::enter();
    // The closure and its captures are dropped after a panic and nothing
    // reads them again, so a broken invariant cannot be seen.
    let outcome = catch_unwind(AssertUnwindSafe(render));
    drop(scope);
    outcome.unwrap_or_else(|payload| {
        Err(RenderError::Panicked {
            path: path.to_path_buf(),
            message: payload_message(payload.as_ref()),
        })
    })
}

/// The text of a panic payload, or a fixed text when it is not a string.
fn payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        NON_STRING_PAYLOAD.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use tarragon_core::error::RenderError;
    use tarragon_core::render_scope::{RenderGuardScope, inside_render_guard};

    use super::run_guarded;

    fn panicked(result: Result<u32, RenderError>) -> (PathBuf, String) {
        match result {
            Err(RenderError::Panicked { path, message }) => (path, message),
            other => panic!("expected Panicked, got {other:?}"),
        }
    }

    #[test]
    fn a_str_panic_becomes_panicked() {
        let result = run_guarded(Path::new("a.png"), || panic!("str boom"));
        assert_eq!(
            panicked(result),
            (PathBuf::from("a.png"), "str boom".to_owned())
        );
    }

    #[test]
    fn a_string_panic_becomes_panicked() {
        let result = run_guarded(Path::new("b.png"), || panic!("{} boom", "string"));
        assert_eq!(
            panicked(result),
            (PathBuf::from("b.png"), "string boom".to_owned())
        );
    }

    #[test]
    fn a_non_string_panic_gets_a_fixed_message() {
        let result = run_guarded(Path::new("c.png"), || std::panic::panic_any(42_u32));
        assert_eq!(
            panicked(result),
            (
                PathBuf::from("c.png"),
                "non-string panic payload".to_owned()
            )
        );
    }

    #[test]
    fn ok_and_err_pass_through() {
        assert_eq!(run_guarded(Path::new("d"), || Ok(7_u32)).expect("ok"), 7);
        let result: Result<u32, RenderError> = run_guarded(Path::new("d"), || {
            Err(RenderError::UnsupportedFormat(".x".to_owned()))
        });
        assert!(matches!(result, Err(RenderError::UnsupportedFormat(ext)) if ext == ".x"));
    }

    #[test]
    fn the_mark_is_set_inside_and_cleared_after() {
        assert!(!inside_render_guard());
        let seen = run_guarded(Path::new("e"), || Ok(inside_render_guard())).expect("ok");
        assert!(seen);
        assert!(!inside_render_guard());
    }

    #[test]
    fn the_mark_is_cleared_after_a_panic() {
        let seen = std::cell::Cell::new(false);
        let result = run_guarded(Path::new("f"), || {
            seen.set(inside_render_guard());
            panic!("late")
        });
        let _: (PathBuf, String) = panicked(result);
        assert!(seen.get());
        assert!(!inside_render_guard());
    }

    #[test]
    fn nested_use_restores_the_previous_value() {
        let _outer = RenderGuardScope::enter();
        let result: Result<u32, RenderError> = run_guarded(Path::new("g"), || panic!("inner"));
        let _: (PathBuf, String) = panicked(result);
        assert!(inside_render_guard(), "the outer mark must survive");
    }
}
