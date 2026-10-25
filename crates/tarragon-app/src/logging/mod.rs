//! Logging: a rotating log file, stderr, an in-memory ring, and a panic hook.

pub mod crash;
mod format;
pub mod ring;
mod rotate;
mod stamp;

use std::path::Path;

use time::UtcOffset;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Registry, reload};

pub use crash::install_panic_hook;
pub use stamp::local_offset;

use format::LineFormat;
use ring::{LogRing, RingLayer};
use rotate::RotatingWriter;

/// Tests that panic on purpose or set the panic hook take this lock, so one
/// does not hit the hook of the other.
#[cfg(test)]
pub(crate) static PANIC_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Dependency targets held at WARN in every mode. A target matches by prefix,
/// so `wgpu` also covers `wgpu_core` and `egui` also covers `egui_wgpu`. The
/// `log` bridge reports the same target names.
const NOISY_TARGETS: [&str; 6] = ["wgpu", "naga", "winit", "eframe", "egui", "epaint"];

/// Build the level filter: INFO or DEBUG for our code, WARN for noisy crates.
fn level_filter(debug: bool) -> Targets {
    let level = if debug { Level::DEBUG } else { Level::INFO };
    NOISY_TARGETS
        .iter()
        .fold(Targets::new().with_default(level), |filter, target| {
            filter.with_target(*target, Level::WARN)
        })
}

/// Changes the log level while the program runs.
#[derive(Debug, Clone)]
pub struct LogControl {
    /// The handle to the live filter.
    handle: reload::Handle<Targets, Registry>,
}

impl LogControl {
    /// Make a filter layer that starts at INFO and the control that changes it.
    pub fn new_filter() -> (reload::Layer<Targets, Registry>, Self) {
        let (layer, handle) = reload::Layer::new(level_filter(false));
        (layer, Self { handle })
    }

    /// Switch between DEBUG (`true`) and INFO (`false`).
    ///
    /// The level stays as it was when the logger is gone or the reload lock
    /// is poisoned.
    pub fn set_debug(&self, debug: bool) {
        let _ = self.handle.modify(|filter| *filter = level_filter(debug));
    }
}

/// What [`init`] sets up. Keep it alive until the program ends: dropping it
/// flushes the log file.
#[derive(Debug)]
pub struct Logging {
    /// The in-memory lines.
    #[expect(dead_code, reason = "read by the code that shows the lines")]
    pub ring: LogRing,
    /// The level switch.
    pub control: LogControl,
    /// Whether this call installed the global logger. When false, nothing
    /// reaches stderr through logging.
    pub installed: bool,
    /// Flushes the file writer on drop.
    _guard: Option<WorkerGuard>,
}

/// Open the rotating log file.
///
/// Returns the writer and a rotation problem found while opening, if any. The
/// error is a message ready to log.
fn open_file_writer(path: &Path) -> Result<(RotatingWriter, Option<String>), String> {
    let mut writer = RotatingWriter::open(path)
        .map_err(|error| format!("cannot open the log file {}: {error}", path.display()))?;
    let warning = writer.take_warning();
    Ok((writer, warning))
}

/// Install the global logger and the `log` bridge.
///
/// Logs go to stderr and to the ring. They also go to `log_file` when it is
/// given and opens; an existing file rolls over first. Times use `offset`. The
/// level starts at INFO. A file that cannot open is reported once, and logging
/// goes on without it.
///
/// The file writer runs on its own thread. The appender crate panics if the
/// system cannot start that thread; no other path here panics.
pub fn init(log_file: Option<&Path>, offset: UtcOffset) -> Logging {
    let (filter, control) = LogControl::new_filter();
    let ring = LogRing::new();

    let mut problems = Vec::new();
    let mut guard = None;
    let file_writer = log_file.and_then(|path| match open_file_writer(path) {
        Ok((writer, warning)) => {
            problems.extend(warning);
            let (non_blocking, worker) = tracing_appender::non_blocking(writer);
            guard = Some(worker);
            Some(non_blocking)
        }
        Err(message) => {
            problems.push(format!("{message}; logging to stderr only"));
            None
        }
    });
    let format = LineFormat { offset };
    let file_layer = file_writer.map(|writer| {
        tracing_subscriber::fmt::layer()
            .event_format(format)
            .with_writer(writer)
    });
    let stderr_layer = tracing_subscriber::fmt::layer()
        .event_format(format)
        .with_writer(std::io::stderr);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(RingLayer::new(ring.clone(), offset))
        .with(file_layer)
        .with(stderr_layer);
    let installed = tracing::subscriber::set_global_default(subscriber).is_ok();
    if installed {
        // The bridge passes every `log` record to tracing, where the level
        // filter drops the noisy crates below WARN. An earlier `log` logger
        // is kept.
        let _ = tracing_log::LogTracer::init();
    }

    for message in problems {
        tracing::warn!("{message}");
    }
    Logging {
        ring,
        control,
        installed,
        _guard: guard,
    }
}

#[cfg(test)]
mod tests {
    use time::UtcOffset;
    use tracing_subscriber::layer::SubscriberExt;

    use super::ring::{LogRing, RingLayer};
    use super::{LogControl, open_file_writer};

    fn texts(ring: &LogRing) -> Vec<String> {
        ring.snapshot().into_iter().map(|e| e.text).collect()
    }

    /// Run `body` under a subscriber that has the level filter and the ring.
    fn with_ring(body: impl FnOnce(&LogControl)) -> Vec<String> {
        let (filter, control) = LogControl::new_filter();
        let ring = LogRing::new();
        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(RingLayer::new(ring.clone(), UtcOffset::UTC));
        tracing::subscriber::with_default(subscriber, || body(&control));
        texts(&ring)
    }

    #[test]
    fn set_debug_toggles_debug_lines_on_and_off() {
        let lines = with_ring(|control| {
            tracing::debug!(target: "demo", "first");
            tracing::info!(target: "demo", "info one");
            control.set_debug(true);
            tracing::debug!(target: "demo", "second");
            control.set_debug(false);
            tracing::debug!(target: "demo", "third");
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].ends_with("INFO     demo: info one"));
        assert!(lines[1].ends_with("DEBUG    demo: second"));
    }

    #[test]
    fn noisy_targets_stay_at_warn_in_debug_mode() {
        let lines = with_ring(|control| {
            control.set_debug(true);
            tracing::debug!(target: "wgpu_core::device", "chatty");
            tracing::info!(target: "winit", "chatty");
            tracing::info!(target: "egui_wgpu::renderer", "chatty");
            tracing::warn!(target: "wgpu", "kept");
            tracing::debug!(target: "tarragon::scan", "kept too");
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("wgpu: kept"));
    }

    #[test]
    fn log_records_reach_the_ring_with_their_own_target() {
        // The bridge is process-wide; a second install is a harmless error.
        let _ = tracing_log::LogTracer::init();
        let lines = with_ring(|control| {
            log::warn!(target: "wgpu_hal::vulkan", "from a noisy crate");
            log::info!(target: "winit::platform", "dropped");
            log::debug!(target: "app::part", "dropped while debug is off");
            control.set_debug(true);
            log::debug!(target: "app::part", "kept with debug on");
            log::debug!(target: "naga::front", "dropped in debug mode");
        });
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].ends_with("WARNING  wgpu_hal::vulkan: from a noisy crate"));
        assert!(lines[1].ends_with("DEBUG    app::part: kept with debug on"));
    }

    #[test]
    fn open_file_writer_reports_a_bad_path() {
        let bad = std::env::temp_dir()
            .join("tarragon-no-such-folder")
            .join("deeper")
            .join("tarragon.log");
        let message = open_file_writer(&bad).expect_err("must fail");
        assert!(message.starts_with("cannot open the log file"), "{message}");
        assert!(message.contains("tarragon.log"), "{message}");
    }
}
