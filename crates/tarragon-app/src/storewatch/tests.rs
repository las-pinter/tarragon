//! Tests for the store epoch throttle. Times are fake, so no test sleeps.

use std::time::{Duration, Instant};

use super::StoreWatch;

fn after(start: Instant, millis: u64) -> Instant {
    start + Duration::from_millis(millis)
}

#[test]
fn the_first_change_fires_at_once_and_an_unchanged_epoch_never_fires() {
    let start = Instant::now();
    let mut watch = StoreWatch::default();
    assert!(!watch.poll(0, start));
    assert!(!watch.poll(0, after(start, 10_000)));
    assert!(watch.poll(1, after(start, 1)));
    assert!(!watch.poll(1, after(start, 10_000)));
}

#[test]
fn a_change_inside_the_window_waits_and_fires_at_the_window_end() {
    let start = Instant::now();
    let mut watch = StoreWatch::default();
    assert!(watch.poll(1, start));
    assert!(!watch.poll(2, after(start, 100)));
    assert!(watch.pending(2));
    assert!(!watch.poll(2, after(start, 249)));
    assert!(watch.poll(2, after(start, 250)));
    assert!(!watch.pending(2));
}

#[test]
fn the_wait_is_the_time_left_while_a_change_is_pending() {
    let start = Instant::now();
    let mut watch = StoreWatch::default();
    assert_eq!(watch.wait(0, start), None);
    assert!(watch.poll(1, start));
    assert_eq!(watch.wait(1, after(start, 100)), None);
    assert_eq!(
        watch.wait(2, after(start, 100)),
        Some(Duration::from_millis(150))
    );
    assert_eq!(watch.wait(2, after(start, 400)), Some(Duration::ZERO));
}

#[test]
fn two_changes_inside_one_window_fire_once() {
    let start = Instant::now();
    let mut watch = StoreWatch::default();
    assert!(watch.poll(1, start));
    assert!(!watch.poll(2, after(start, 50)));
    assert!(!watch.poll(3, after(start, 100)));
    assert!(watch.poll(3, after(start, 300)));
    assert!(!watch.poll(3, after(start, 600)));
}
