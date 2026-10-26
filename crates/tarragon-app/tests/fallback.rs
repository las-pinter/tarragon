//! Start-up failures of the window, run through the real binary.
//!
//! The display test needs no X server, because nothing listens on `:9999`. The
//! glow test needs `xvfb-run`, so both are ignored by default. Run them with
//! `cargo nextest run --run-ignored only fallback`.

use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use image::{Rgba, RgbaImage};

/// The most time the app gets to start and close.
const LIMIT: Duration = Duration::from_secs(120);

/// A temp folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tarragon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("make the temp folder");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Wait for the child, or kill it after `LIMIT`.
fn wait(child: &mut std::process::Child) -> ExitStatus {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll the app") {
            return status;
        }
        if started.elapsed() > LIMIT {
            let _ = child.kill();
            panic!("the app did not exit within {LIMIT:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A folder with one picture, and an empty data folder next to it.
fn setup(dir: &TempDir) -> (PathBuf, PathBuf) {
    let pics = dir.0.join("pics");
    let data = dir.0.join("data");
    std::fs::create_dir_all(&pics).expect("make the folder");
    std::fs::create_dir_all(&data).expect("make the data folder");
    RgbaImage::from_pixel(40, 30, Rgba([200, 90, 20, 255]))
        .save(pics.join("one.png"))
        .expect("write a png");
    (pics, data)
}

#[test]
#[ignore = "starts the binary"]
fn a_missing_display_is_named_and_does_not_try_glow() {
    let dir = TempDir::new("nodisplay");
    let (pics, data) = setup(&dir);
    let stderr_path = dir.0.join("stderr.txt");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tarragon"))
        .arg(&pics)
        .env("TARRAGON_DB", data.join("tarragon.db"))
        // Nothing listens here, so the real screen is never reached.
        .env("DISPLAY", ":9999")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .stdin(Stdio::null())
        .stderr(File::create(&stderr_path).expect("make the stderr file"))
        .spawn()
        .expect("start the app");
    let status = wait(&mut child);
    let text = std::fs::read_to_string(&stderr_path).unwrap_or_default();
    let log = std::fs::read_to_string(data.join("tarragon.log")).unwrap_or_default();
    let all = format!("{text}\n{log}");

    assert!(!status.success(), "exit status {status}\n{all}");
    assert!(
        all.contains("cannot connect to the display (DISPLAY=:9999, WAYLAND_DISPLAY=unset)"),
        "{all}"
    );
    assert!(!all.contains("trying glow"), "{all}");
    assert!(!all.contains("recreated"), "{all}");
}

#[test]
#[ignore = "needs xvfb-run"]
fn a_wgpu_failure_falls_back_to_glow() {
    let dir = TempDir::new("glow");
    let (pics, data) = setup(&dir);
    // No Vulkan driver is found, so wgpu cannot get an adapter.
    let mut child = Command::new("xvfb-run")
        .arg("-a")
        .arg(env!("CARGO_BIN_EXE_tarragon"))
        .arg(&pics)
        .env("TARRAGON_DB", data.join("tarragon.db"))
        .env("TARRAGON_CLOSE_WHEN_LOADED", "1")
        .env("WGPU_BACKEND", "vulkan")
        .env("VK_DRIVER_FILES", "/nonexistent")
        .env("VK_ICD_FILENAMES", "/nonexistent")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .stdin(Stdio::null())
        .spawn()
        .expect("start xvfb-run");
    let status = wait(&mut child);
    let log = std::fs::read_to_string(data.join("tarragon.log")).unwrap_or_default();

    assert!(status.success(), "exit status {status}\n{log}");
    assert!(log.contains("trying glow"), "{log}");
    assert!(log.contains("1 tiles settled, 0 failed"), "{log}");
}
