//! Tests for the folder dialog runner. No real dialog opens here.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::*;

/// How long a test waits for a worker thread before it fails.
const WAIT: Duration = Duration::from_secs(10);

/// A dialog that counts its calls, waits for a release signal, and then
/// gives a fixed answer.
struct FakeDialog {
    calls: Arc<AtomicUsize>,
    release: Receiver<()>,
    answer: Option<PathBuf>,
}

impl FolderDialog for FakeDialog {
    fn pick(&self, _start: Option<&Path>) -> Option<PathBuf> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.release
            .recv_timeout(WAIT)
            .expect("test releases the dialog");
        self.answer.clone()
    }
}

/// A dialog that panics, to end the worker thread without an answer.
struct PanicDialog;

impl FolderDialog for PanicDialog {
    fn pick(&self, _start: Option<&Path>) -> Option<PathBuf> {
        panic!("the dialog failed on purpose");
    }
}

/// A picker over a fake dialog, with the call counter, the release sender,
/// and the receiver of repaint signals.
fn fake_picker(
    answer: Option<PathBuf>,
) -> (Picker, Arc<AtomicUsize>, mpsc::Sender<()>, Receiver<()>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let (release_tx, release) = mpsc::channel();
    let (repaint_tx, repaint_rx) = mpsc::channel();
    let repaint_tx = Mutex::new(repaint_tx);
    let dialog = FakeDialog {
        calls: Arc::clone(&calls),
        release,
        answer,
    };
    let picker = Picker::with_dialog(Box::new(dialog), move || {
        let _ = repaint_tx.lock().expect("repaint lock").send(());
    });
    (picker, calls, release_tx, repaint_rx)
}

/// Polls until an answer arrives or the wait runs out.
fn wait_for_answer(picker: &mut Picker) -> (Purpose, Option<PathBuf>) {
    let start = Instant::now();
    loop {
        if let Some(answer) = picker.poll() {
            return answer;
        }
        assert!(start.elapsed() < WAIT, "no answer in time");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn poll_returns_the_picked_path_once() {
    let path = PathBuf::from("/some/folder");
    let (mut picker, _calls, release, _repaint) = fake_picker(Some(path.clone()));
    picker.ask(Purpose::OpenFolder, None);
    release.send(()).expect("release");
    assert_eq!(
        wait_for_answer(&mut picker),
        (Purpose::OpenFolder, Some(path))
    );
    assert_eq!(picker.poll(), None);
}

#[test]
fn poll_is_none_before_any_ask() {
    let (mut picker, _calls, _release, _repaint) = fake_picker(None);
    assert_eq!(picker.poll(), None);
}

#[test]
fn poll_is_none_while_the_dialog_is_open() {
    let (mut picker, _calls, release, _repaint) = fake_picker(None);
    picker.ask(Purpose::CacheDir, None);
    assert_eq!(picker.poll(), None);
    release.send(()).expect("release");
    wait_for_answer(&mut picker);
}

#[test]
fn ask_while_a_dialog_is_open_is_ignored() {
    let (mut picker, calls, release, _repaint) = fake_picker(None);
    picker.ask(Purpose::OpenFolder, None);
    picker.ask(Purpose::CacheDir, None);
    release.send(()).expect("release");
    // The first purpose wins, and only one answer arrives.
    assert_eq!(wait_for_answer(&mut picker), (Purpose::OpenFolder, None));
    assert_eq!(picker.poll(), None);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn ask_works_again_after_the_answer() {
    let (mut picker, calls, release, _repaint) = fake_picker(None);
    for purpose in [Purpose::OpenFolder, Purpose::CacheDir] {
        picker.ask(purpose, None);
        release.send(()).expect("release");
        assert_eq!(wait_for_answer(&mut picker), (purpose, None));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn a_cancelled_dialog_answers_none() {
    let (mut picker, _calls, release, _repaint) = fake_picker(None);
    picker.ask(Purpose::CacheDir, None);
    release.send(()).expect("release");
    assert_eq!(wait_for_answer(&mut picker), (Purpose::CacheDir, None));
}

#[test]
fn the_repaint_callback_runs_when_the_answer_arrives() {
    let (mut picker, _calls, release, repaint) = fake_picker(None);
    picker.ask(Purpose::OpenFolder, None);
    assert!(repaint.try_recv().is_err(), "no repaint before the answer");
    release.send(()).expect("release");
    repaint
        .recv_timeout(WAIT)
        .expect("repaint after the answer");
}

#[test]
fn a_dialog_that_dies_answers_none() {
    let mut picker = Picker::with_dialog(Box::new(PanicDialog), || {});
    picker.ask(Purpose::OpenFolder, None);
    assert_eq!(wait_for_answer(&mut picker), (Purpose::OpenFolder, None));
    assert_eq!(picker.poll(), None);
}

#[test]
fn the_fake_pick_value_makes_a_fixed_dialog() {
    let path = PathBuf::from("/fake/choice");
    let mut picker = Picker::from_env_value(Some(path.clone().into_os_string()), || {});
    picker.ask(Purpose::CacheDir, Some(PathBuf::from("/start")));
    assert_eq!(
        wait_for_answer(&mut picker),
        (Purpose::CacheDir, Some(path))
    );
}
