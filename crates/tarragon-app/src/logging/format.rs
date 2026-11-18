//! The line shape of the log file and stderr.

use std::fmt;

use time::UtcOffset;
use tracing::{Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

use super::ring::{MessageVisitor, level_name};
use super::stamp;

/// Formats `YYYY-MM-DD HH:MM:SS+HH:MM [LEVEL] message`. A DEBUG line also
/// shows the target before the message. The time uses the stored offset.
#[derive(Debug, Clone, Copy)]
pub(super) struct LineFormat {
    /// The offset used for the time of each line.
    pub offset: UtcOffset,
}

impl<S, N> FormatEvent<S, N> for LineFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        // Events from the `log` bridge carry their real target here.
        let normalized = event.normalized_metadata();
        let meta = normalized.as_ref().unwrap_or_else(|| event.metadata());
        let stamp = stamp::with_offset(stamp::now(self.offset));
        write!(writer, "{stamp} [{}] ", level_name(*meta.level()))?;
        if *meta.level() == Level::DEBUG {
            write!(writer, "{}: ", meta.target())?;
        }
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        writer.write_str(&visitor.finish())?;
        writeln!(writer)
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use time::UtcOffset;
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::layer::SubscriberExt;

    use super::LineFormat;

    /// A writer that collects bytes in memory.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("lock").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Sink {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn capture(run: impl FnOnce()) -> String {
        let sink = Sink::default();
        let layer = tracing_subscriber::fmt::layer()
            .event_format(LineFormat {
                offset: UtcOffset::UTC,
            })
            .with_writer(sink.clone());
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), run);
        let bytes = sink.0.lock().expect("lock").clone();
        String::from_utf8(bytes).expect("utf-8")
    }

    #[test]
    fn info_line_has_time_level_and_message_only() {
        let out = capture(|| tracing::info!(target: "demo", "hello é"));
        assert!(out.ends_with(" [INFO] hello é\n"), "{out:?}");
        assert!(!out.contains("demo"));
        assert_eq!(out.as_bytes()[4], b'-');
        assert_eq!(&out[19..25], "+00:00");
    }

    #[test]
    fn debug_line_shows_the_target() {
        let out = capture(|| tracing::debug!(target: "demo::part", n = 2, "step"));
        assert!(out.ends_with(" [DEBUG] demo::part: step n=2\n"), "{out:?}");
    }

    #[test]
    fn warn_line_uses_the_warning_name() {
        let out = capture(|| tracing::warn!("careful"));
        assert!(out.ends_with(" [WARNING] careful\n"), "{out:?}");
    }

    #[test]
    fn bridged_log_record_shows_its_real_target_and_no_log_fields() {
        let _ = tracing_log::LogTracer::init();
        let out = capture(|| log::warn!(target: "wgpu_hal", "slow"));
        assert!(out.ends_with(" [WARNING] slow\n"), "{out:?}");
    }
}
