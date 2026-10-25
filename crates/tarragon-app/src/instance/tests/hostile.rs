//! Tests with hostile or slow clients, wakers, and a dead wake-up.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use super::super::tcp::TcpTransport;
use super::super::*;
use super::{TempDir, primary};

/// Send raw text to the primary and return what comes back.
fn raw_exchange<T: Transport>(
    layout: &Layout,
    text: impl FnOnce(Option<&str>) -> String,
) -> String {
    let (mut stream, token) = T::connect(layout).expect("connect");
    stream
        .write_all(text(token.as_deref()).as_bytes())
        .expect("send");
    let mut reply = String::new();
    // The listener closes the stream without a reply, so this ends at once.
    let _ = stream.read_to_string(&mut reply);
    reply
}

/// The listener ignores an unknown line and the `bad` line, and still works.
fn ignores_unknown_and_bad_lines<T: Transport>(bad: impl FnOnce(Option<&str>) -> String) {
    let dir = TempDir::new();
    let layout = dir.layout();
    let guard = primary(acquire_with::<T>(&layout));
    assert_eq!(raw_exchange::<T>(&layout, |_| "hello\n".into()), "");
    assert_eq!(raw_exchange::<T>(&layout, bad), "");
    assert!(!guard.take_activate_request());
    // The listener still works after the bad input.
    assert!(matches!(acquire_with::<T>(&layout), Outcome::Forwarded));
    assert!(guard.take_activate_request());
}

#[cfg(unix)]
#[test]
fn unix_unknown_line_gives_no_activation() {
    ignores_unknown_and_bad_lines::<unix::UnixTransport>(|_| "tarragon/1 other\n".into());
}

#[test]
fn tcp_unknown_line_and_wrong_token_give_no_activation() {
    ignores_unknown_and_bad_lines::<TcpTransport>(|_| proto::request_line(Some("wrong")));
}

#[test]
fn hung_client_does_not_block_the_listener() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let guard = primary(acquire_with::<TcpTransport>(&layout));
    // A silent client must not block the listener from serving the next one.
    let (_hung, _) = TcpTransport::connect(&layout).expect("connect");
    assert!(matches!(
        acquire_with::<TcpTransport>(&layout),
        Outcome::Forwarded
    ));
    assert!(guard.take_activate_request());
}

#[test]
fn a_panicking_waker_does_not_end_the_listener() {
    let _serial = crate::logging::PANIC_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = TempDir::new();
    let layout = dir.layout();
    let guard = primary(acquire_with::<TcpTransport>(&layout));
    guard.set_waker(|| panic!("waker panic on purpose"));
    assert!(matches!(
        acquire_with::<TcpTransport>(&layout),
        Outcome::Forwarded
    ));
    assert!(guard.take_activate_request());
    // The listener lives: a second request still works.
    guard.set_waker(|| {});
    assert!(matches!(
        acquire_with::<TcpTransport>(&layout),
        Outcome::Forwarded
    ));
    assert!(guard.take_activate_request());
}

#[test]
fn a_waker_may_replace_itself() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let guard = Arc::new(primary(acquire_with::<TcpTransport>(&layout)));
    let inner = Arc::clone(&guard);
    guard.set_waker(move || inner.set_waker(|| {}));
    assert!(matches!(
        acquire_with::<TcpTransport>(&layout),
        Outcome::Forwarded
    ));
    assert!(guard.take_activate_request());
    // The test ends the cycle: the waker holds the guard.
    guard.set_waker(|| {});
}

/// A client that sends a byte now and then does not hold the listener.
fn dribbling_client_is_cut_off<T: Transport>() {
    let dir = TempDir::new();
    let layout = dir.layout();
    let guard = primary(acquire_with::<T>(&layout));
    let (mut slow, _) = T::connect(&layout).expect("connect");
    let dribble = std::thread::spawn(move || {
        for _ in 0..50 {
            if slow.write_all(b"x").is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    std::thread::sleep(Duration::from_millis(150));
    assert!(matches!(acquire_with::<T>(&layout), Outcome::Forwarded));
    assert!(guard.take_activate_request());
    drop(guard);
    dribble.join().expect("dribble");
}

#[cfg(unix)]
#[test]
fn unix_dribbling_client_is_cut_off() {
    dribbling_client_is_cut_off::<unix::UnixTransport>();
}

#[test]
fn tcp_dribbling_client_is_cut_off() {
    dribbling_client_is_cut_off::<TcpTransport>();
}

#[test]
fn drop_with_a_dead_wake_does_not_hang() {
    let dir = TempDir::new();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let guard = Guard {
        delivery: Delivery::default(),
        stop,
        wake: Box::new(|| false),
        // This thread is not joined, so it ends by itself.
        thread: Some(std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(10));
            }
        })),
        files: Vec::new(),
        _lock: open_lock(&dir.layout().lock).expect("open"),
    };
    let (done, wait) = mpsc::channel();
    std::thread::spawn(move || {
        drop(guard);
        let _ = done.send(());
    });
    wait.recv_timeout(Duration::from_secs(5))
        .expect("drop returned");
}
