use eframe::egui::{Pos2, Rect, Vec2};

use super::*;

#[test]
fn stored_geometry_sets_size_and_position() {
    let view = viewport_for("T", Some([10, -20, 900, 600]));
    assert_eq!(view.title.as_deref(), Some("T"));
    assert_eq!(view.inner_size, Some(Vec2::new(900.0, 600.0)));
    assert_eq!(view.position, Some(Pos2::new(10.0, -20.0)));
}

#[test]
fn no_geometry_gives_the_default_size_and_no_position() {
    let view = viewport_for("T", None);
    assert_eq!(view.inner_size, Some(Vec2::new(1280.0, 800.0)));
    assert_eq!(view.position, None);
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
}

#[test]
fn geometry_reads_position_from_outer_and_size_from_inner() {
    let info = ViewportInfo {
        outer_rect: Some(rect(5.0, 6.0, 810.0, 640.0)),
        inner_rect: Some(rect(7.0, 30.0, 800.0, 600.0)),
        ..Default::default()
    };
    assert_eq!(geometry_from(&info, None, [0, 0]), Some([5, 6, 800, 600]));
}

#[test]
fn missing_outer_rect_keeps_the_old_position() {
    let info = ViewportInfo {
        inner_rect: Some(rect(0.0, 0.0, 700.0, 500.0)),
        ..Default::default()
    };
    let old = Some([40, 50, 1, 1]);
    assert_eq!(geometry_from(&info, old, [0, 0]), Some([40, 50, 700, 500]));
}

#[test]
fn no_position_ever_known_gives_no_geometry() {
    let info = ViewportInfo {
        inner_rect: Some(rect(0.0, 0.0, 700.0, 500.0)),
        ..Default::default()
    };
    assert_eq!(geometry_from(&info, None, [0, 0]), None);
}

#[test]
fn minimized_window_keeps_the_old_geometry() {
    let info = ViewportInfo {
        minimized: Some(true),
        outer_rect: Some(rect(-32000.0, -32000.0, 100.0, 100.0)),
        inner_rect: Some(rect(-32000.0, -32000.0, 100.0, 100.0)),
        ..Default::default()
    };
    let old = Some([1, 2, 3, 4]);
    assert_eq!(geometry_from(&info, old, [0, 0]), old);
    assert_eq!(geometry_from(&ViewportInfo::default(), None, [0, 0]), None);
}

/// The time of the first frame in the tests.
const START: Duration = Duration::from_secs(10);

/// The time `value` milliseconds after `START`.
fn ms(value: u64) -> Duration {
    START + Duration::from_millis(value)
}

/// A report of a window manager that put the window `step` below the asked
/// position and shows no frame offset, as inner equals outer.
fn reported(view: &ViewportBuilder, step: f32) -> ViewportInfo {
    let asked = view.position.unwrap_or_default();
    let size = view.inner_size.unwrap_or_default();
    let rect = Rect::from_min_size(asked + Vec2::new(0.0, step), size);
    ViewportInfo {
        outer_rect: Some(rect),
        inner_rect: Some(rect),
        ..Default::default()
    }
}

#[test]
fn late_steps_of_the_window_manager_keep_the_geometry_over_restarts() {
    let stored = [217, 219, 2011, 1105];
    let mut geometry = Some(stored);
    for _ in 0..3 {
        let view = viewport_for("T", geometry);
        let mut tracker = Tracker::new(geometry);
        for (time, step) in [(0, 0.0), (200, 14.0), (400, 28.0), (600, 28.0)] {
            geometry = tracker.track(&reported(&view, step), geometry, ms(time), false);
        }
        assert!(tracker.settling());
        assert_eq!(tracker.at_exit(geometry), Some(stored));
    }
}

#[test]
fn a_move_after_the_settle_time_is_kept_without_the_offset() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    let mut geometry = stored;
    for (time, step) in std::iter::once((0, 0.0)).chain((1..=13).map(|i| (i * 200, 28.0))) {
        geometry = tracker.track(&reported(&view, step), geometry, ms(time), false);
    }
    assert!(!tracker.settling());
    let moved = ViewportInfo {
        outer_rect: Some(rect(300.0, 400.0, 800.0, 600.0)),
        inner_rect: Some(rect(300.0, 400.0, 800.0, 600.0)),
        ..Default::default()
    };
    geometry = tracker.track(&moved, geometry, ms(3000), false);
    assert_eq!(tracker.at_exit(geometry), Some([300, 372, 800, 600]));
}

#[test]
fn user_input_freezes_the_offset() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    let mut geometry = tracker.track(&reported(&view, 28.0), stored, ms(0), false);
    geometry = tracker.track(&reported(&view, 28.0), geometry, ms(300), true);
    assert!(!tracker.settling());
    let moved = ViewportInfo {
        outer_rect: Some(rect(150.0, 200.0, 800.0, 600.0)),
        ..Default::default()
    };
    assert_eq!(
        tracker.track(&moved, geometry, ms(400), false),
        Some([150, 172, 800, 600])
    );
}

#[test]
fn an_exit_while_settling_saves_the_asked_position() {
    let stored = Some([217, 219, 2011, 1105]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    let geometry = tracker.track(&reported(&view, 14.0), stored, ms(0), false);
    assert_eq!(geometry, Some([217, 219, 2011, 1105]));
    let halfway = ViewportInfo {
        outer_rect: Some(rect(217.0, 300.0, 2011.0, 1105.0)),
        inner_rect: Some(rect(217.0, 300.0, 2011.0, 1105.0)),
        ..Default::default()
    };
    let geometry = tracker.track(&halfway, geometry, ms(200), false);
    assert_eq!(geometry, Some([217, 219, 2011, 1105]));
    assert_eq!(tracker.at_exit(geometry), stored);
}

#[test]
fn exactly_the_settle_time_freezes() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    tracker.track(&reported(&view, 28.0), stored, ms(0), false);
    for time in (1..=9).map(|i| i * 200) {
        tracker.track(&reported(&view, 28.0), stored, ms(time), false);
    }
    tracker.track(&reported(&view, 28.0), stored, ms(1999), false);
    assert!(tracker.settling());
    tracker.track(&reported(&view, 28.0), stored, ms(2000), false);
    assert!(!tracker.settling());
}

#[test]
fn minimized_frames_do_not_use_up_the_settle_time() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    let minimized = ViewportInfo {
        minimized: Some(true),
        ..reported(&view, 0.0)
    };
    for time in [0, 1000, 5000] {
        tracker.track(&minimized, stored, ms(time), false);
    }
    assert!(tracker.settling());
    tracker.track(&reported(&view, 0.0), stored, ms(5100), false);
    tracker.track(&reported(&view, 28.0), stored, ms(5300), false);
    assert!(tracker.settling());
    let mut geometry = stored;
    for time in (1..=9).map(|i| 5300 + i * 200) {
        geometry = tracker.track(&reported(&view, 28.0), stored, ms(time), false);
    }
    assert!(!tracker.settling());
    assert_eq!(geometry, stored);
}

#[test]
fn a_long_gap_between_frames_does_not_end_the_settle_time() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    tracker.track(&reported(&view, 0.0), stored, ms(0), false);
    let geometry = tracker.track(&reported(&view, 28.0), stored, ms(3000), false);
    assert!(tracker.settling());
    assert_eq!(geometry, stored);
    assert_eq!(tracker.shift, [0, 28]);
}

#[test]
fn user_input_on_the_first_frame_freezes_at_zero() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    let geometry = tracker.track(&reported(&view, 28.0), stored, ms(0), true);
    assert!(!tracker.settling());
    assert_eq!(geometry, Some([100, 128, 800, 600]));
}

#[test]
fn a_negative_offset_is_taken_like_a_positive_one() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    let geometry = tracker.track(&reported(&view, -28.0), stored, ms(0), false);
    assert_eq!(geometry, stored);
    let far = reported(&view, -101.0);
    assert_eq!(
        tracker.track(&far, geometry, ms(100), false),
        Some([100, 27, 800, 600])
    );
}

#[test]
fn a_far_offset_is_not_taken_as_a_frame() {
    let mut tracker = Tracker::new(Some([500, 500, 800, 600]));
    let info = ViewportInfo {
        outer_rect: Some(rect(0.0, 0.0, 800.0, 600.0)),
        inner_rect: Some(rect(0.0, 0.0, 800.0, 600.0)),
        ..Default::default()
    };
    assert_eq!(
        tracker.track(&info, None, ms(0), false),
        Some([0, 0, 800, 600])
    );
}

#[test]
fn no_asked_position_gives_no_offset_and_no_settling() {
    let mut tracker = Tracker::new(None);
    let info = ViewportInfo {
        outer_rect: Some(rect(5.0, 6.0, 800.0, 628.0)),
        inner_rect: Some(rect(5.0, 34.0, 800.0, 600.0)),
        ..Default::default()
    };
    let geometry = tracker.track(&info, None, ms(0), false);
    assert_eq!(geometry, Some([5, 6, 800, 600]));
    assert!(!tracker.settling());
    assert_eq!(tracker.at_exit(geometry), geometry);
}

/// Run a clamped start: the window stays where it was asked, so the measured
/// shift is zero. Then the user moves it after the settle time.
fn clamped_start_then_move(saved: Option<[i64; 2]>) -> Option<Geometry> {
    let stored = Some([100, 0, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored).with_saved_shift(saved);
    let mut geometry = stored;
    for time in (0..=10).map(|i| i * 200) {
        geometry = tracker.track(&reported(&view, 0.0), geometry, ms(time), false);
    }
    assert!(!tracker.settling());
    let moved = rect(300.0, 400.0, 800.0, 600.0);
    let info = ViewportInfo {
        outer_rect: Some(moved),
        inner_rect: Some(moved),
        ..Default::default()
    };
    let geometry = tracker.track(&info, geometry, ms(3000), false);
    tracker.at_exit(geometry)
}

#[test]
fn a_saved_shift_is_applied_when_the_measurement_is_zero() {
    assert_eq!(
        clamped_start_then_move(Some([0, 28])),
        Some([300, 372, 800, 600])
    );
}

#[test]
fn a_missing_saved_shift_changes_nothing() {
    assert_eq!(clamped_start_then_move(None), Some([300, 400, 800, 600]));
}

#[test]
fn a_far_saved_shift_is_ignored() {
    assert_eq!(
        clamped_start_then_move(Some([0, 5000])),
        Some([300, 400, 800, 600])
    );
}

#[test]
fn a_measured_shift_wins_over_the_saved_one() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored).with_saved_shift(Some([0, 50]));
    let mut geometry = stored;
    for time in (0..=10).map(|i| i * 200) {
        geometry = tracker.track(&reported(&view, 28.0), geometry, ms(time), false);
    }
    assert_eq!(tracker.measured_shift(), Some([0, 28]));
    let moved = rect(300.0, 400.0, 800.0, 600.0);
    let info = ViewportInfo {
        outer_rect: Some(moved),
        inner_rect: Some(moved),
        ..Default::default()
    };
    let geometry = tracker.track(&info, geometry, ms(3000), false);
    assert_eq!(geometry, Some([300, 372, 800, 600]));
}

#[test]
fn no_shift_is_reported_while_settling_or_when_zero() {
    let stored = Some([100, 100, 800, 600]);
    let view = viewport_for("T", stored);
    let mut tracker = Tracker::new(stored);
    tracker.track(&reported(&view, 28.0), stored, ms(0), false);
    assert_eq!(tracker.measured_shift(), None);
    let mut tracker = Tracker::new(stored);
    for time in (0..=10).map(|i| i * 200) {
        tracker.track(&reported(&view, 0.0), stored, ms(time), false);
    }
    assert_eq!(tracker.measured_shift(), None);
    assert!(tracker.measured_no_shift());
}

#[test]
fn a_run_without_an_asked_position_measured_nothing() {
    let mut tracker = Tracker::new(None).with_saved_shift(Some([0, 28]));
    tracker.track(&ViewportInfo::default(), None, ms(0), false);
    assert!(!tracker.measured_no_shift());
    assert_eq!(tracker.measured_shift(), None);
}

#[test]
fn no_request_gives_no_commands() {
    assert_eq!(activate_commands(false), Vec::<ViewportCommand>::new());
}

#[test]
fn a_request_unminimizes_shows_focuses_then_asks_for_attention() {
    assert_eq!(
        activate_commands(true),
        [
            ViewportCommand::Minimized(false),
            ViewportCommand::Visible(true),
            ViewportCommand::Focus,
            ViewportCommand::RequestUserAttention(UserAttentionType::Informational),
        ]
    );
}
