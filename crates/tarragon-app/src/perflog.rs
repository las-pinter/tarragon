//! The opt-in perf log: a text file of start-up measures and, on request, a
//! frame run. Without the log variable nothing here reads or writes anything.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::framelog::{FrameRecorder, FrameReport, Phase};

/// The frame run waits this long after the load before it records.
const SETTLE: Duration = Duration::from_secs(5);
/// The recorder skips the frames of this span.
const WARMUP: Duration = Duration::from_secs(5);
/// The recorded span of the frame run.
const RUN: Duration = Duration::from_secs(30);
/// A valid run needs a window at least this wide, in points.
const MIN_WIDTH: f32 = 1280.0;
/// A valid run needs a window at least this high, in points.
const MIN_HEIGHT: f32 = 800.0;
/// A valid run needs at least this many live textures at its end.
const MIN_TEXTURES: usize = 900;

/// The first moment of `main`, the fallback for the cold start.
static MAIN_START: OnceLock<Instant> = OnceLock::new();

/// Remember the first moment of `main`. A second call changes nothing.
pub fn mark_main_start() {
    let _ = MAIN_START.set(Instant::now());
}

/// The log file and the frame-run flag from the two variables. An empty log
/// value counts as unset. The frame run is meaningful only with a log file.
pub fn from_vars(log: Option<OsString>, frame_run: bool) -> (Option<PathBuf>, bool) {
    (log.filter(|v| !v.is_empty()).map(PathBuf::from), frame_run)
}

/// Read `TARRAGON_PERF_LOG` and `TARRAGON_FRAME_RUN` once.
pub fn from_env() -> (Option<PathBuf>, bool) {
    from_vars(
        std::env::var_os("TARRAGON_PERF_LOG"),
        std::env::var_os("TARRAGON_FRAME_RUN").is_some(),
    )
}

/// What `ui` tells the log about one frame.
pub struct Frame {
    /// The end of the frame.
    pub now: Instant,
    /// The window size in points.
    pub window: [f32; 2],
    /// The renderer that runs: `wgpu` or `glow`.
    pub renderer: &'static str,
    /// True in the first frame whose grid shows the folder's tiles.
    pub shown: bool,
    /// True in the frame where every tile is settled.
    pub loaded: bool,
    /// The textures the grid budget holds now.
    pub textures: usize,
    /// The vertical scroll offset of the grid in points.
    pub scroll_y: f32,
}

/// What the app must do after a frame.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing.
    Idle,
    /// Draw a frame after this long.
    Wait(Duration),
    /// Draw the next frame at once.
    Repaint,
    /// The run is over; close the window.
    Close,
}

/// Writes the perf log file.
pub struct PerfLog {
    /// The file. It is created or cut at the first write.
    path: PathBuf,
    /// Whether the file exists now, so later writes append. The first write
    /// is the header.
    created: bool,
    /// Whether the frame run is on.
    frame_run: bool,
    /// The moment the grid first showed its tiles; the cold start ends here.
    shown_at: Option<Instant>,
    /// The moment every tile settled.
    loaded_at: Option<Instant>,
    /// Takes the frames of the run.
    recorder: FrameRecorder,
    /// Whether the end of the run is written.
    finished: bool,
    /// The largest grid offset seen while the run scrolled, in points.
    scroll_max: f32,
}

impl PerfLog {
    /// A log for `path`, or none when no file was asked for.
    pub fn new(path: Option<PathBuf>, frame_run: bool) -> Option<Self> {
        Some(Self {
            path: path?,
            created: false,
            frame_run,
            shown_at: None,
            loaded_at: None,
            recorder: FrameRecorder::new(WARMUP, RUN),
            finished: false,
            scroll_max: 0.0,
        })
    }

    /// Take one finished frame: write what is due and say what comes next.
    pub fn frame(&mut self, frame: &Frame) -> Step {
        if !self.created {
            let session = session_text(
                env("XDG_SESSION_TYPE").as_deref(),
                env("WAYLAND_DISPLAY").as_deref(),
                env("DISPLAY").as_deref(),
            );
            self.write(&header_lines(session, frame.renderer, frame.window));
        }
        // A load without a shown report (a failed open) still ends the cold start.
        if (frame.shown || frame.loaded) && self.shown_at.is_none() {
            self.shown_at = Some(frame.now);
            let (cold_ms, basis) = cold_start(frame.now);
            self.write(&cold_lines(cold_ms, basis));
        }
        if frame.loaded && self.loaded_at.is_none() {
            self.loaded_at = Some(frame.now);
            self.write(&loaded_lines(peak_rss_kib(), frame.textures));
        }
        let Some(loaded_at) = self.loaded_at.filter(|_| self.frame_run && !self.finished) else {
            return Step::Idle;
        };
        let settled = loaded_at + SETTLE;
        if frame.now < settled {
            return Step::Wait(settled - frame.now);
        }
        self.scroll_max = self.scroll_max.max(frame.scroll_y);
        if self.recorder.frame(frame.now) != Phase::Done {
            return Step::Repaint;
        }
        self.finished = true;
        let report = self.recorder.report();
        let span = report.as_ref().map_or(Duration::ZERO, |r| r.span);
        let valid = run_valid(frame.window, span, frame.textures);
        self.write(&end_lines(
            report.as_ref(),
            self.scroll_max,
            frame.textures,
            valid,
        ));
        Step::Close
    }

    /// How long the grid has scrolled itself, or none outside the scroll span.
    ///
    /// The span is the warm-up and the recorded run, after the settle wait.
    /// Without a frame run the answer is always none.
    pub fn scroll_elapsed(&self, now: Instant) -> Option<Duration> {
        let loaded_at = self
            .loaded_at
            .filter(|_| self.frame_run && !self.finished)?;
        now.checked_duration_since(loaded_at + SETTLE)
    }

    /// Write `text`; the first write creates or cuts the file. A failure is
    /// logged and never stops the app.
    fn write(&mut self, text: &str) {
        let result = if self.created {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&self.path)
                .and_then(|mut file| file.write_all(text.as_bytes()))
        } else {
            std::fs::write(&self.path, text)
        };
        self.created = true;
        if let Err(error) = result {
            tracing::warn!("cannot write the perf log {}: {error}", self.path.display());
        }
    }
}

/// An environment value as text, or none when unset or not text.
fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// `wayland` or `x11`. The session type wins; when it is unset or says
/// something else, the display variables decide, Wayland first.
pub fn session_text(xdg: Option<&str>, wayland: Option<&str>, x11: Option<&str>) -> &'static str {
    let set = |v: Option<&str>| v.is_some_and(|v| !v.is_empty());
    match xdg {
        Some("wayland") => "wayland",
        Some("x11") => "x11",
        _ if set(wayland) => "wayland",
        _ if set(x11) => "x11",
        _ => "unknown",
    }
}

/// Whether a frame run counts as a result.
pub fn run_valid(window: [f32; 2], span: Duration, textures: usize) -> bool {
    window[0] >= MIN_WIDTH && window[1] >= MIN_HEIGHT && span >= RUN && textures >= MIN_TEXTURES
}

/// The lines written at the first frame.
fn header_lines(session: &str, renderer: &str, window: [f32; 2]) -> String {
    let mut text = String::from("# perf log\n");
    let _ = writeln!(text, "session: {session}");
    let _ = writeln!(text, "renderer: {renderer}");
    let _ = writeln!(text, "window: {:.0}x{:.0}", window[0], window[1]);
    text
}

/// The lines written when the grid first shows its tiles.
fn cold_lines(cold_ms: u128, basis: &str) -> String {
    format!("cold_start_ms: {cold_ms}\ncold_start_basis: {basis}\n")
}

/// The lines written when every tile is settled.
fn loaded_lines(rss_kib: Option<u64>, textures: usize) -> String {
    let rss = rss_kib.map_or_else(|| "na".to_owned(), |kib| kib.to_string());
    format!("peak_rss_kib: {rss}\nlive_textures: {textures}\n")
}

/// The lines written when the frame run ends.
fn end_lines(
    report: Option<&FrameReport>,
    scroll_max: f32,
    textures: usize,
    valid: bool,
) -> String {
    let mut text = report.map_or_else(|| "frames: too few\n".to_owned(), FrameReport::lines);
    let _ = writeln!(text, "scroll_max_pt: {scroll_max:.0}");
    let _ = writeln!(text, "live_textures_at_end: {textures}");
    let _ = writeln!(text, "run_valid: {}", if valid { "yes" } else { "no" });
    text
}

/// The time since the process started and the basis of the number. The spawn
/// time comes from the kernel; if it cannot be read, the first moment of
/// `main` stands in.
fn cold_start(now: Instant) -> (u128, &'static str) {
    let spawn = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .zip(std::fs::read_to_string("/proc/uptime").ok())
        .and_then(|(stat, uptime)| spawn_age(&stat, &uptime));
    if let Some(age) = spawn {
        return (age.as_millis(), "spawn");
    }
    let main = MAIN_START.get().copied().unwrap_or(now);
    (now.saturating_duration_since(main).as_millis(), "main")
}

/// How long ago the process started, from `/proc/self/stat` and
/// `/proc/uptime`. The start time is in ticks of 10 ms.
fn spawn_age(stat: &str, uptime: &str) -> Option<Duration> {
    // The name in field 2 may hold spaces, so count from the last bracket.
    let after_name = stat.rsplit_once(')')?.1;
    // The first field after the name is field 3; the start time is field 22.
    let ticks: u64 = after_name.split_whitespace().nth(19)?.parse().ok()?;
    let up: f64 = uptime.split_whitespace().next()?.parse().ok()?;
    let up = Duration::try_from_secs_f64(up).ok()?;
    up.checked_sub(Duration::from_millis(ticks.checked_mul(10)?))
}

/// The peak resident memory in KiB, or none off Linux or on a read error.
fn peak_rss_kib() -> Option<u64> {
    vm_hwm_kib(&std::fs::read_to_string("/proc/self/status").ok()?)
}

/// The `VmHWM` value of a `/proc/self/status` text, in KiB.
fn vm_hwm_kib(status: &str) -> Option<u64> {
    let line = status.lines().find_map(|l| l.strip_prefix("VmHWM:"))?;
    line.split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests;
