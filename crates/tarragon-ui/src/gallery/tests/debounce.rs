//! Tests for the quiet-window timer.

use super::*;

#[test]
fn a_debouncer_without_arm_is_never_ready() {
    let mut debouncer = Debouncer::default();
    assert!(!debouncer.is_ready(Instant::now()));
    assert_eq!(debouncer.remaining(Instant::now()), None);
}

#[test]
fn the_search_window_is_exactly_300_ms() {
    assert_eq!(SEARCH_DEBOUNCE, Duration::from_millis(300));
}

#[test]
fn the_debouncer_waits_for_the_quiet_window() {
    let start = Instant::now();
    let mut debouncer = Debouncer::default();
    debouncer.arm(start);
    assert!(!debouncer.is_ready(start));
    assert!(!debouncer.is_ready(start + Duration::from_millis(299)));
    assert!(debouncer.is_ready(start + SEARCH_DEBOUNCE));
    assert!(!debouncer.is_ready(start + SEARCH_DEBOUNCE), "ready clears");
}

#[test]
fn the_debouncer_reports_the_time_left() {
    let start = Instant::now();
    let mut debouncer = Debouncer::default();
    debouncer.arm(start);
    assert_eq!(debouncer.remaining(start), Some(SEARCH_DEBOUNCE));
    assert_eq!(
        debouncer.remaining(start + Duration::from_millis(200)),
        Some(Duration::from_millis(100))
    );
    assert_eq!(
        debouncer.remaining(start + SEARCH_DEBOUNCE),
        Some(Duration::ZERO),
        "a deadline that passed leaves no time, but the wake is still owed"
    );
}

#[test]
fn typing_rearms_the_quiet_window() {
    let start = Instant::now();
    let mut debouncer = Debouncer::default();
    debouncer.arm(start);
    debouncer.arm(start + Duration::from_millis(200));
    assert!(!debouncer.is_ready(start + Duration::from_millis(450)));
    assert!(debouncer.is_ready(start + Duration::from_millis(500)));
}
