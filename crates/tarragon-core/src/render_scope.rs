//! A thread-local mark for code that runs inside the render panic guard.
//!
//! The panic hook runs before `catch_unwind` returns. The hook asks this mark
//! whether the panic will be caught, so it can skip the crash file for a panic
//! that the render guard turns into a normal error. The mark lives here because
//! both the render crate and the app crate already depend on this crate.
//!
//! The mark is per thread. It does not cover a thread that a renderer spawns:
//! a panic on such a worker still writes a crash file, while a panic that is
//! re-raised on the calling thread is still caught.

use std::cell::Cell;

thread_local! {
    static INSIDE_RENDER_GUARD: Cell<bool> = const { Cell::new(false) };
}

/// Keeps the mark set while it lives and puts the old value back on drop.
///
/// The drop also runs while a panic unwinds, so the mark never stays set.
#[derive(Debug)]
#[must_use = "the mark is cleared when this value drops"]
pub struct RenderGuardScope {
    previous: bool,
}

impl RenderGuardScope {
    /// Set the mark for the current thread until the value drops.
    pub fn enter() -> Self {
        let previous = INSIDE_RENDER_GUARD.with(|flag| flag.replace(true));
        Self { previous }
    }
}

impl Drop for RenderGuardScope {
    fn drop(&mut self) {
        INSIDE_RENDER_GUARD.with(|flag| flag.set(self.previous));
    }
}

/// True when the current thread runs inside the render panic guard.
#[must_use]
pub fn inside_render_guard() -> bool {
    INSIDE_RENDER_GUARD.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::{RenderGuardScope, inside_render_guard};

    #[test]
    fn the_mark_is_off_by_default() {
        assert!(!inside_render_guard());
    }

    #[test]
    fn the_mark_is_set_in_scope_and_cleared_after() {
        {
            let _scope = RenderGuardScope::enter();
            assert!(inside_render_guard());
        }
        assert!(!inside_render_guard());
    }

    #[test]
    fn nested_scopes_restore_the_previous_value() {
        let outer = RenderGuardScope::enter();
        {
            let _inner = RenderGuardScope::enter();
            assert!(inside_render_guard());
        }
        assert!(inside_render_guard());
        drop(outer);
        assert!(!inside_render_guard());
    }

    #[test]
    fn the_mark_is_per_thread() {
        let _scope = RenderGuardScope::enter();
        let other = std::thread::spawn(inside_render_guard)
            .join()
            .expect("join");
        assert!(!other);
    }
}
