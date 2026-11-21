//! Small helpers for child processes: deadlines, waits, and capped reads.

use std::io::{self, Read};
use std::process::{Child, ExitStatus};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How often to check whether a child has ended.
const POLL_STEP: Duration = Duration::from_millis(10);

/// The time left before `deadline`.
pub(crate) fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// End a child and collect it, so it does not stay a zombie.
pub(crate) fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Wait for `child` to end until `deadline`. `None` means the time ran out.
pub(crate) fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(POLL_STEP);
    }
}

/// Read up to `cap` bytes of `reader` on a thread. The rest is read and thrown
/// away, so the child does not stop on a full pipe.
pub(crate) fn read_capped<R: Read + Send + 'static>(
    name: &str,
    mut reader: R,
    cap: usize,
) -> io::Result<Receiver<io::Result<Vec<u8>>>> {
    let (sender, receiver) = mpsc::channel();
    let limit = u64::try_from(cap).unwrap_or(u64::MAX);
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = (&mut reader)
                .take(limit)
                .read_to_end(&mut bytes)
                .and_then(|_| io::copy(&mut reader, &mut io::sink()))
                .map(|_| bytes);
            let _ = sender.send(result);
        })?;
    Ok(receiver)
}
