//! Window geometry and the raise-the-window commands.

use std::time::Duration;

use eframe::egui::{UserAttentionType, ViewportBuilder, ViewportCommand, ViewportInfo};

/// The inner size used when no geometry is stored, in points.
const DEFAULT_SIZE: [f32; 2] = tarragon_ui::tokens::WINDOW_SIZE;

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
    /// The offset saved by an earlier run, used once, when none is measured.
    saved: [i64; 2],
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

    /// Use the offset an earlier run saved while none is measured. A saved
    /// offset that is too large is ignored.
    #[must_use]
    pub fn with_saved_shift(mut self, saved: Option<[i64; 2]>) -> Self {
        let fits = |part: &[i64; 2]| part.iter().all(|v| v.abs() <= MAX_SHIFT);
        self.saved = saved.filter(fits).unwrap_or_default();
        self
    }

    /// The offset to keep for the next start, once it is frozen and not zero.
    /// A user input can freeze it in the middle of a step, so it may be a
    /// partial value. The next run that measures a shift overwrites it.
    pub fn measured_shift(&self) -> Option<[i64; 2]> {
        (!self.settling() && self.shift != [0, 0]).then_some(self.shift)
    }

    /// Whether the offset is frozen and this run measured none. Then a saved
    /// offset has been used once and must not be used again, or it would move
    /// the window further at every start on a setup that has no frame offset.
    /// So a second clamped start in a row measures zero with no saved value;
    /// that is accepted. A run with no asked position measured nothing.
    pub fn measured_no_shift(&self) -> bool {
        self.asked.is_some() && !self.settling() && self.shift == [0, 0]
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
        // A measured offset is trusted whole: a window manager that reported
        // one axis reported the frame, so a saved axis must not be mixed in.
        let shift = if self.shift == [0, 0] {
            self.saved
        } else {
            self.shift
        };
        geometry_from(info, old, shift)
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
mod tests;
