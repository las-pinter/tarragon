//! Driven scrolling of the grid: a setter for the offset and a repeating path.

use std::time::Duration;

use super::Gallery;

/// The speed of the driven scroll, in points per second. The pass of about
/// 20 s for 10 000 tiles assumes the two-column layout at a 1280 point
/// window. The speed is fixed, so texture churn per second is the same on
/// every layout.
const SCROLL_SPEED: f32 = 60_000.0;

/// The offset in points after `elapsed` of a driven scroll.
///
/// The offset starts at 0, grows at a constant speed to the end of the
/// content (`content` less `view`), and returns at the same speed. The path
/// repeats. Content that fits the view gives 0.
#[must_use]
pub fn scroll_offset(elapsed: Duration, content: f32, view: f32) -> f32 {
    let travel = content - view;
    if travel.is_nan() || travel <= 0.0 {
        return 0.0;
    }
    let along = (elapsed.as_secs_f32() * SCROLL_SPEED) % (2.0 * travel);
    if along <= travel {
        along
    } else {
        2.0 * travel - along
    }
}

impl Gallery {
    /// Scroll the grid to `y` points in the next frame. The offset applies
    /// once; a grid that gets no call keeps the offset it holds.
    pub fn set_scroll_offset(&mut self, y: f32) {
        self.scroll_pin = Some(y);
    }

    /// The vertical offset of the grid after the last frame, in points.
    #[must_use]
    pub fn scroll_y(&self) -> f32 {
        self.scroll_info.offset
    }

    /// The content and view heights of the grid after the last frame, in
    /// points.
    #[must_use]
    pub fn scroll_extent(&self) -> (f32, f32) {
        (self.scroll_info.content, self.scroll_info.view)
    }
}
