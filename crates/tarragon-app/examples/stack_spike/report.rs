//! The shared run record, the log file, and the printed summary.

use std::fmt::Write as _;

use crate::settings::Settings;

/// A p99 at or over this value fails the run.
const P99_LIMIT_MS: f64 = 33.0;
/// A frame over this value is a hard fail.
const HARD_LIMIT_MS: f64 = 100.0;

/// Facts about the session and the renderer, read in the first frame.
#[derive(Default, Clone)]
pub struct Facts {
    /// `x11`, `wayland`, or `other`, from the window system handle.
    pub session: String,
    /// The device name and the graphics API.
    pub device: String,
}

/// The record of one run. The window code and the watchdog share it.
#[derive(Default)]
pub struct Run {
    /// Facts about the session and the device.
    pub facts: Facts,
    /// Frame times in milliseconds, in order, from the measure phase.
    pub frames: Vec<f64>,
    /// The sum of the tiles drawn in the measured frames.
    pub tiles_sum: usize,
    /// The largest number of tiles drawn in one frame.
    pub tiles_max: usize,
    /// The number of measured frames in which the window had no focus.
    pub focus_lost: usize,
    /// Why the run is not valid, when it is not.
    pub invalid: Option<String>,
    /// True after the first end of the run. The first end wins.
    pub finished: bool,
    /// True when one thread has started to write the report.
    pub reporting: bool,
}

/// The result of a run. It maps to the printed word and the exit code.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The run is valid and inside the limits.
    Pass,
    /// The run is valid and over a limit.
    Fail,
    /// The run is not valid, or the log could not be written.
    NotValid,
}

impl Verdict {
    /// The word in the log and in the summary.
    fn word(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotValid => "NOT VALID",
        }
    }

    /// The process exit code.
    fn code(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::NotValid => 2,
        }
    }
}

/// Cut a value to two decimals without rounding up. A value under a limit
/// then never shows as the limit, and a value at the limit or over it never
/// shows as less.
fn floor2(value: f64) -> f64 {
    (value * 100.0 + 1e-6).floor() / 100.0
}

/// True when the reason is a hidden window or a stalled one. Only then the
/// hint to keep the window visible fits.
fn window_was_hidden(reason: &str) -> bool {
    ["minimized", "covered", "no frame callback"]
        .iter()
        .any(|word| reason.contains(word))
}

/// Return the value at a percentile, by nearest rank, from sorted values.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = (p / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// Print the report, write the log file, and return the exit code.
/// The code is 2 when the log cannot be written.
///
/// The log is written for a valid run and for a run that is not valid. A run
/// that is not valid keeps the frames that were collected.
#[allow(clippy::cast_precision_loss)]
pub fn report(settings: &Settings, run: &Run) -> u8 {
    let span = run.frames.iter().sum::<f64>().max(0.0) / 1000.0;
    let mut sorted = run.frames.clone();
    sorted.sort_by(f64::total_cmp);
    let invalid = run.invalid.clone().or_else(|| {
        sorted
            .is_empty()
            .then(|| "no frame was recorded".to_owned())
    });
    let stats = (!sorted.is_empty()).then(|| {
        (
            percentile(&sorted, 50.0),
            percentile(&sorted, 95.0),
            percentile(&sorted, 99.0),
            sorted[sorted.len() - 1],
        )
    });
    let tiles_mean = run.tiles_sum as f64 / run.frames.len().max(1) as f64;
    let (verdict, detail) = match (&invalid, stats) {
        (Some(reason), _) => (Verdict::NotValid, Some(reason.clone())),
        (None, Some((.., max))) if floor2(max) > HARD_LIMIT_MS => (
            Verdict::Fail,
            Some("hard fail: a frame is over 100 ms".to_owned()),
        ),
        (None, Some((_, _, p99, _))) if floor2(p99) >= P99_LIMIT_MS => {
            (Verdict::Fail, Some("p99 is 33 ms or more".to_owned()))
        }
        _ => (Verdict::Pass, None),
    };
    let mut result = verdict.word().to_owned();
    if let Some(detail) = detail {
        let _ = write!(result, " ({detail})");
    }

    let mut text = String::new();
    let _ = writeln!(text, "# renderer: {}", settings.renderer.name());
    let _ = writeln!(text, "# session: {}", run.facts.session);
    let _ = writeln!(text, "# device: {}", run.facts.device);
    let _ = writeln!(text, "# textures: {}", settings.textures);
    let _ = writeln!(
        text,
        "# configured_duration_s: {}",
        settings.duration.as_secs_f64()
    );
    let _ = writeln!(text, "# measured_span_s: {span:.2}");
    let _ = writeln!(
        text,
        "# tiles_drawn_per_frame: mean {tiles_mean:.1}, max {}",
        run.tiles_max
    );
    let _ = writeln!(text, "# focus_lost_frames: {}", run.focus_lost);
    let _ = writeln!(text, "# frames: {}", run.frames.len());
    if let Some((p50, p95, p99, max)) = stats {
        let _ = writeln!(
            text,
            "# p50_ms: {p50:.2}\n# p95_ms: {p95:.2}\n# p99_ms: {:.2}\n# max_ms: {:.2}",
            floor2(p99),
            floor2(max)
        );
    }
    let _ = writeln!(text, "# result: {result}");
    let _ = writeln!(text, "frame,ms");
    for (index, ms) in run.frames.iter().enumerate() {
        let _ = writeln!(text, "{index},{ms:.3}");
    }
    let written = std::fs::write(&settings.out, &text);
    if let Err(error) = &written {
        eprintln!("cannot write the log {}: {error}", settings.out);
        eprintln!("the run counts as not valid because the log is lost");
    }

    println!("renderer: {}", settings.renderer.name());
    println!("session:  {}", run.facts.session);
    println!("device:   {}", run.facts.device);
    println!("frames:   {} in {span:.2} s", run.frames.len());
    println!(
        "tiles:    mean {tiles_mean:.1}, max {} drawn per frame of {} live",
        run.tiles_max, settings.textures
    );
    if let Some((p50, p95, p99, max)) = stats {
        println!(
            "p50: {p50:.2} ms  p95: {p95:.2} ms  p99: {:.2} ms  max: {:.2} ms",
            floor2(p99),
            floor2(max)
        );
    }
    if run.focus_lost > 0 {
        println!(
            "warning:  the window had no focus in {} measured frames (recorded only)",
            run.focus_lost
        );
    }
    println!("result:   {result}");
    println!("log:      {}", settings.out);
    if invalid.as_deref().is_some_and(window_was_hidden) {
        eprintln!("keep the window visible and not covered during the run");
    }
    if written.is_err() {
        Verdict::NotValid.code()
    } else {
        verdict.code()
    }
}
