//! Window geometry and the raise-the-window commands.

use std::time::Duration;

use eframe::egui::{UserAttentionType, ViewportBuilder, ViewportCommand, ViewportInfo};

/// The inner size used when no geometry is stored, in points.
const DEFAULT_SIZE: [f32; 2] = [1280.0, 800.0];

/// Window geometry as `[x, y, width, height]`.
pub type Geometry = [i64; 4];

/// Convert a stored value to points. Window sizes are far below the range
/// where an f32 loses whole numbers.
#[expect(clippy::cast_precision_loss, reason = "window sizes are small")]
fn points(value: i64) -> f32 {
    value as f32
}

/// Convert a rectangle part to a stored value, rounded to the nearest whole.
#[expect(clippy::cast_possible_truncation, reason = "window sizes are small")]
fn whole(value: f32) -> i64 {
    value.round() as i64
}

/// The viewport for a start: the stored rectangle, or the default size.
pub fn viewport_for(title: &str, geometry: Option<Geometry>) -> ViewportBuilder {
    let builder = ViewportBuilder::default().with_title(title);
    match geometry {
        Some([x, y, width, height]) => builder
            .with_inner_size([points(width), points(height)])
            .with_position([points(x), points(y)]),
        None => builder.with_inner_size(DEFAULT_SIZE),
    }
}

/// The largest offset between the asked and the reported position that is
/// taken as a frame offset. A larger one is a window that was not placed yet.
const MAX_SHIFT: i64 = 100;

/// The geometry to keep after a frame. A minimized or full-screen window keeps
/// the old value. A missing outer rectangle (Wayland gives none) keeps the old
/// position, and a missing inner rectangle keeps the old size. When no position
/// was ever known, the result is `None`, so no made-up 0,0 is stored.
/// `shift` is how far the reported position lies from the asked one, and it is
/// taken off so that a restart keeps the position.
pub fn geometry_from(
    info: &ViewportInfo,
    old: Option<Geometry>,
    shift: [i64; 2],
) -> Option<Geometry> {
    if info.minimized == Some(true) || info.fullscreen == Some(true) {
        return old;
    }
    let [old_x, old_y, old_width, old_height] = old.unwrap_or_default();
    let (x, y) = match (info.outer_rect, old) {
        (Some(r), _) => (whole(r.min.x) - shift[0], whole(r.min.y) - shift[1]),
        (None, Some(_)) => (old_x, old_y),
        (None, None) => return None,
    };
    let (width, height) = info.inner_rect.map_or((old_width, old_height), |r| {
        (whole(r.width()), whole(r.height()))
    });
    if width <= 0 || height <= 0 {
        return old;
    }
    Some([x, y, width, height])
}

/// How long after the start the window manager may still move the window.
const SETTLE: Duration = Duration::from_secs(2);

/// How often to repaint while the window may still be moved.
pub const SETTLE_REPAINT: Duration = Duration::from_millis(200);

/// The longest gap between two frames that counts as settle time.
const MAX_GAP: Duration = Duration::from_millis(400);

/// Follows the window rectangle across frames.
///
/// Some X11 window managers move the window below its title bar after it was
/// shown, in steps, and the reported position then lies that far from the asked
/// one. The window system reports no frame size. Until the first user input
/// or the settle time, the offset is measured on every frame as reported minus
/// asked, then it is frozen. Time in a minimized or full-screen state does not
/// count as settle time. A move by the window manager inside the settle time
/// counts as offset, so that move is not saved. A close inside that time saves
/// the asked position, as the user cannot have moved the window yet. Where the asked position is
/// unknown, as on a first start or on Wayland, the offset stays zero.
#[derive(Debug, Default)]
pub struct Tracker {
    /// The position given to the viewport.
    asked: Option<[i64; 2]>,
    /// The offset, measured while settling.
    shift: [i64; 2],
    /// The time spent in a normal state so far.
    normal: Duration,
    /// The time of the previous frame, when it was in a normal state.
    previous: Option<Duration>,
    /// Whether the offset is frozen.
    frozen: bool,
    /// The frames seen so far.
    frame: u64,
    /// The last line logged, so only a change is logged.
    last_line: String,
}

impl Tracker {
    /// A tracker for a start with the given stored geometry.
    pub fn new(stored: Option<Geometry>) -> Self {
        Self {
            asked: stored.map(|[x, y, ..]| [x, y]),
            ..Self::default()
        }
    }

    /// Whether the window may still be moved by the window manager.
    pub fn settling(&self) -> bool {
        self.asked.is_some() && !self.frozen
    }

    /// The geometry to keep after a frame. `now` is the time since the start
    /// and `user_input` tells that the user acted in this frame.
    pub fn track(
        &mut self,
        info: &ViewportInfo,
        old: Option<Geometry>,
        now: Duration,
        user_input: bool,
    ) -> Option<Geometry> {
        let was_settling = self.settling();
        let normal = info.minimized != Some(true) && info.fullscreen != Some(true);
        if normal {
            if let Some(previous) = self.previous {
                // A stall, such as a slow start of the GPU, must not end the
                // settle time before the window manager has moved the window.
                self.normal += now.saturating_sub(previous).min(MAX_GAP);
            }
            self.previous = Some(now);
        } else {
            self.previous = None;
        }
        if was_settling {
            if user_input || self.normal >= SETTLE {
                self.frozen = true;
            } else if normal && let (Some(asked), Some(outer)) = (self.asked, info.outer_rect) {
                let shift = [whole(outer.min.x) - asked[0], whole(outer.min.y) - asked[1]];
                if shift.iter().all(|part| part.abs() <= MAX_SHIFT) {
                    self.shift = shift;
                }
            }
        }
        self.frame += 1;
        if was_settling && tracing::enabled!(tracing::Level::DEBUG) {
            if self.frozen {
                tracing::debug!(
                    "window shift frozen at frame {}: {:?}",
                    self.frame,
                    self.shift
                );
            } else {
                let line = format!(
                    "asked={:?} outer={:?} inner={:?} min={:?} full={:?} shift={:?}",
                    self.asked,
                    info.outer_rect.map(|r| (r.min.x, r.min.y)),
                    info.inner_rect.map(|r| (r.min.x, r.min.y)),
                    info.minimized,
                    info.fullscreen,
                    self.shift,
                );
                if line != self.last_line {
                    tracing::debug!("window frame {}: {line}", self.frame);
                    self.last_line = line;
                }
            }
        }
        geometry_from(info, old, self.shift)
    }

    /// The geometry to save at exit. Before the offset is frozen the user
    /// cannot have moved the window, so the asked position is kept.
    pub fn at_exit(&self, current: Option<Geometry>) -> Option<Geometry> {
        match (self.settling(), self.asked, current) {
            (true, Some([x, y]), Some([_, _, width, height])) => Some([x, y, width, height]),
            _ => current,
        }
    }
}

/// The commands that bring the window to the front for an activate request.
/// Wayland ignores focus, so the last command asks for user attention.
pub fn activate_commands(requested: bool) -> Vec<ViewportCommand> {
    if !requested {
        return Vec::new();
    }
    vec![
        ViewportCommand::Minimized(false),
        ViewportCommand::Visible(true),
        ViewportCommand::Focus,
        ViewportCommand::RequestUserAttention(UserAttentionType::Informational),
    ]
}

#[cfg(test)]
mod tests {
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
}
