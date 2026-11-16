//! Runs the app rows of `tests/smoke/smoke.sh` with a stub app that writes a
//! canned perf log, so no real window, scan, or `sudo` is involved.

// The script needs Linux tools such as `free`, `lscpu` and `taskset`.
#![cfg(target_os = "linux")]

#[allow(
    dead_code,
    reason = "shared with the other script tests; each file uses part of it"
)]
#[path = "support/smoke_bundle.rs"]
mod smoke_bundle;

use smoke_bundle::{BENCH_PASS, Bundle, good_frame, good_load, stub, text};
use std::fs;
use std::process::Output;

/// Run a full pass with a passing drop and no display. The corpus is the
/// bundle's own folder.
fn full_run(b: &Bundle) -> (Output, String) {
    let out = b.run("cores4", "true", &[b.corpus().to_str().unwrap()]);
    let s = text(&out);
    (out, s)
}

/// Run with a changed cold start log and return the text.
fn with_load(name: &str, log: &str) -> (Output, String) {
    let b = Bundle::new(name, BENCH_PASS);
    b.set_load(log);
    full_run(&b)
}

/// Run with a changed frame log and return the text.
fn with_frame(name: &str, log: &str) -> (Output, String) {
    let b = Bundle::new(name, BENCH_PASS);
    b.set_frame(log);
    full_run(&b)
}

#[test]
fn good_logs_pass_every_app_row_and_write_the_files() {
    let b = Bundle::new("app-good", BENCH_PASS);
    let (out, s) = full_run(&b);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("PASS cold_start 1200 ms (basis spawn)"), "{s}");
    assert!(s.contains("PASS peak_rss 900000 KiB"), "{s}");
    assert!(s.contains("PASS p99_wayland p99 16.00 ms"), "{s}");
    assert!(
        s.contains("INFO p99_xwayland skipped, DISPLAY is not set"),
        "{s}"
    );
    assert!(s.contains("SUMMARY pass=5 fail=0"), "{s}");
    assert!(s.contains("  PASS p99_wayland\n"), "{s}");
    assert_eq!(
        s.matches("FRAME RUN: keep the window visible and do not touch the mouse for 40 s")
            .count(),
        1,
        "{s}"
    );
    assert_eq!(
        fs::read_to_string(b.root.join("smoke-result.txt")).unwrap(),
        s
    );
    assert_eq!(
        fs::read_to_string(b.root.join("perf-cold_start.txt")).unwrap(),
        good_load()
    );
    assert_eq!(
        fs::read_to_string(b.root.join("perf-p99_wayland.txt")).unwrap(),
        good_frame()
    );
}

#[test]
fn each_app_run_gets_the_pin_the_right_variables_and_a_new_data_folder() {
    let b = Bundle::new("app-env", BENCH_PASS);
    let (out, s) = full_run(&b);
    assert!(out.status.success(), "{s}");
    let taskset = fs::read_to_string(b.root.join("taskset.log")).unwrap();
    let app_runs = taskset
        .lines()
        .filter(|l| l.contains("timeout --foreground 300"))
        .count();
    assert_eq!(app_runs, 2, "{taskset}");
    assert!(
        taskset.lines().all(|l| l.starts_with("-c 0,2,4,6 ")),
        "{taskset}"
    );
    let log = fs::read_to_string(b.root.join("app.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(
        lines[0].contains("close=1") && lines[0].contains("frame=unset"),
        "{log}"
    );
    assert!(
        lines[1].contains("frame=1") && lines[1].contains("close=unset"),
        "{log}"
    );
    let data = |l: &str| {
        l.split(" data=")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .to_owned()
    };
    assert_ne!(data(lines[0]), data(lines[1]), "{log}");
    for l in &lines {
        assert!(
            l.starts_with(&format!("args={} ", b.corpus().display())),
            "{log}"
        );
        assert!(
            !std::path::Path::new(&data(l)).exists(),
            "data folder left: {log}"
        );
    }
}

#[test]
fn the_caches_drop_again_right_before_the_cold_start_run() {
    let b = Bundle::new("app-drops", BENCH_PASS);
    let drop_cmd = format!("echo d >> {}/drops", b.root.display());
    let out = b.run("cores4", &drop_cmd, &[b.corpus().to_str().unwrap()]);
    let s = text(&out);
    assert!(out.status.success(), "{s}");
    assert_eq!(s.matches("COLD STEP").count(), 2, "{s}");
    let log = fs::read_to_string(b.root.join("app.log")).unwrap();
    assert!(log.lines().next().unwrap().ends_with("drops=2"), "{log}");
}

#[test]
fn a_cache_that_is_partly_cold_fails_the_cold_start_row() {
    let b = Bundle::new("app-partly", BENCH_PASS);
    let out = b.run("cores4", "false", &[b.corpus().to_str().unwrap()]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains("FAIL cold_start 1200 ms, cache PARTLY COLD"),
        "{s}"
    );
    assert!(s.contains("PASS peak_rss"), "{s}");
}

#[test]
fn cold_start_of_3200_ms_fails() {
    let (out, s) = with_load("app-cold", &good_load().replace("1200", "3200"));
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains("FAIL cold_start 3200 ms, the bar is under 3000 ms"),
        "{s}"
    );
}

#[test]
fn rss_of_2200000_kib_fails() {
    let (out, s) = with_load("app-rss", &good_load().replace("900000", "2200000"));
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("FAIL peak_rss 2200000 KiB"), "{s}");
    assert!(s.contains("PASS cold_start"), "{s}");
}

#[test]
fn rss_that_is_not_a_number_fails() {
    let (out, s) = with_load("app-rss-na", &good_load().replace("900000", "na"));
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("FAIL peak_rss no peak_rss_kib number"), "{s}");
}

#[test]
fn p99_of_40_ms_fails() {
    let log = good_frame()
        .replace("p99_ms: 16.00", "p99_ms: 40.00")
        .replace("frame_verdict: PASS", "frame_verdict: FAIL");
    let (out, s) = with_frame("app-p99", &log);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains("FAIL p99_wayland frame_verdict FAIL (p99 40.00 ms"),
        "{s}"
    );
}

#[test]
fn max_of_150_ms_fails() {
    let log = good_frame()
        .replace("max_ms: 40.00", "max_ms: 150.00")
        .replace("frame_verdict: PASS", "frame_verdict: FAIL");
    let (out, s) = with_frame("app-max", &log);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains("FAIL p99_wayland frame_verdict FAIL (p99 16.00 ms, max 150.00 ms"),
        "{s}"
    );
}

#[test]
fn an_invalid_run_fails_with_the_reason() {
    let log = good_frame().replace("run_valid: yes", "run_valid: no");
    let (out, s) = with_frame("app-invalid", &log);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains("FAIL p99_wayland run is not valid (run_valid no)"),
        "{s}"
    );
}

#[test]
fn an_x11_session_fails_the_wayland_row_with_a_clear_text() {
    let log = good_frame().replace("session: wayland", "session: x11");
    let (out, s) = with_frame("app-x11", &log);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains("FAIL p99_wayland session is x11, it must be wayland"),
        "{s}"
    );
    assert!(s.contains("PASS cold_start"), "{s}");
}

#[test]
fn an_app_that_writes_no_log_fails_the_rows_with_no_perf_log() {
    let b = Bundle::new("app-nolog", BENCH_PASS);
    stub(&b.root.join("bin/tarragon"), "exit 0\n");
    let (out, s) = full_run(&b);
    assert_eq!(out.status.code(), Some(1), "{s}");
    for row in ["cold_start", "peak_rss", "p99_wayland"] {
        assert!(
            s.contains(&format!("FAIL {row} no perf log (app exit 0")),
            "{row}: {s}"
        );
    }
}

#[test]
fn an_app_that_hangs_is_stopped_by_the_time_limit() {
    let b = Bundle::new("app-hang", BENCH_PASS);
    stub(&b.root.join("bin/tarragon"), "exec sleep 30\n");
    let out = b.run_env(
        "cores4",
        "true",
        &[b.corpus().to_str().unwrap()],
        &[("TARRAGON_SMOKE_APP_TIMEOUT", "1")],
    );
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains("FAIL cold_start no perf log (app exit 124, timed out after 1 s)"),
        "{s}"
    );
    assert!(s.contains("FAIL p99_wayland no perf log"), "{s}");
}

#[test]
fn a_stale_perf_log_does_not_count_when_the_app_writes_none() {
    let b = Bundle::new("app-stale", BENCH_PASS);
    fs::write(b.root.join("perf-cold_start.txt"), good_load()).unwrap();
    stub(&b.root.join("bin/tarragon"), "exit 3\n");
    let (_, s) = full_run(&b);
    assert!(
        s.contains("FAIL cold_start no perf log (app exit 3)"),
        "{s}"
    );
}

#[test]
fn the_comparison_run_never_changes_the_exit_code() {
    let b = Bundle::new("app-xway", BENCH_PASS);
    let bad = good_frame()
        .replace("session: wayland", "session: x11")
        .replace("p99_ms: 16.00", "p99_ms: 50.00")
        .replace("frame_verdict: PASS", "frame_verdict: FAIL")
        .replace("run_valid: yes", "run_valid: no");
    b.set_frame_x(&bad);
    let out = b.run_env(
        "cores4",
        "true",
        &[b.corpus().to_str().unwrap()],
        &[("DISPLAY", ":99"), ("WAYLAND_DISPLAY", "wayland-test")],
    );
    let s = text(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("INFO p99_xwayland p99 50.00 ms"), "{s}");
    assert!(s.contains("verdict FAIL"), "{s}");
    assert!(s.contains("PASS p99_wayland"), "{s}");
    assert_eq!(s.matches("FRAME RUN:").count(), 2, "{s}");
    let log = fs::read_to_string(b.root.join("app.log")).unwrap();
    assert!(
        log.lines().nth(1).unwrap().contains("wayland=wayland-test"),
        "{log}"
    );
    assert!(
        log.lines().nth(2).unwrap().contains("wayland=unset"),
        "{log}"
    );
    assert!(b.root.join("perf-p99_xwayland.txt").exists());
}

#[test]
fn the_comparison_run_without_a_log_is_still_only_info() {
    let b = Bundle::new("app-xway-nolog", BENCH_PASS);
    b.stub_app(&b.root.join("app_once"));
    stub(
        &b.root.join("bin/tarragon"),
        &format!(
            "[ -z \"${{WAYLAND_DISPLAY+set}}\" ] && exit 0\n. {}/app_once\n",
            b.root.display()
        ),
    );
    let out = b.run_env(
        "cores4",
        "true",
        &[b.corpus().to_str().unwrap()],
        &[("DISPLAY", ":99"), ("WAYLAND_DISPLAY", "wayland-test")],
    );
    let s = text(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("INFO p99_xwayland no perf log"), "{s}");
}

#[test]
fn the_app_path_can_be_overridden_and_is_listed() {
    let b = Bundle::new("app-override", BENCH_PASS);
    fs::remove_file(b.root.join("bin/tarragon")).unwrap();
    let other = b.root.join("other_app");
    b.stub_app(&other);
    let out = b.run_env(
        "cores4",
        "true",
        &[b.corpus().to_str().unwrap()],
        &[("TARRAGON_SMOKE_APP", other.to_str().unwrap())],
    );
    let s = text(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("INFO override TARRAGON_SMOKE_APP\n"), "{s}");
    assert!(s.contains("PASS cold_start"), "{s}");
}

#[test]
fn psd_bench_numbers_are_info_and_never_change_the_exit_code() {
    let b = Bundle::new("app-psd", BENCH_PASS);
    stub(
        &b.root.join("bin/psd_rss_bench"),
        "echo \"args=$*\"\necho 'peak 5000000 KiB'\nexit 1\n",
    );
    let (out, s) = full_run(&b);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("INFO psd_rss_bench exit 1"), "{s}");
    assert!(
        s.contains(&format!("  args=--source-dir {}\n", b.corpus().display())),
        "{s}"
    );
    assert!(s.contains("  peak 5000000 KiB\n"), "{s}");
}

#[test]
fn a_missing_psd_bench_is_info() {
    let b = Bundle::new("app-nopsd", BENCH_PASS);
    let (out, s) = full_run(&b);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("INFO psd_rss_bench not in the bundle"), "{s}");
}

#[test]
fn a_missing_corpus_fails_every_app_row_too() {
    let b = Bundle::new("app-nocorpus", BENCH_PASS);
    fs::remove_file(b.root.join("bin/corpus_gen")).unwrap();
    let (out, s) = full_run(&b);
    assert_eq!(out.status.code(), Some(1), "{s}");
    for row in ["scan_cold", "cold_start", "peak_rss", "p99_wayland"] {
        assert!(s.contains(&format!("FAIL {row} no corpus")), "{row}: {s}");
    }
    assert!(!b.root.join("app.log").exists());
}

#[test]
fn check_only_runs_no_app_and_writes_no_result_file() {
    let b = Bundle::new("app-check-only", BENCH_PASS);
    let out = b.run("cores4", "false", &["--check-only"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!b.root.join("app.log").exists());
    assert!(!b.root.join("smoke-result.txt").exists());
}

#[test]
fn the_result_file_keeps_the_failing_exit_code() {
    let b = Bundle::new("app-exit", BENCH_PASS);
    let (out, s) = full_run_with_bad_rss(&b);
    assert_eq!(out.status.code(), Some(1), "{s}");
    let saved = fs::read_to_string(b.root.join("smoke-result.txt")).unwrap();
    assert!(saved.contains("SUMMARY pass=4 fail=1"), "{saved}");
    assert!(saved.contains("  FAIL peak_rss\n"), "{saved}");
}

/// A full run where the RSS is over the bar.
fn full_run_with_bad_rss(b: &Bundle) -> (Output, String) {
    b.set_load(&good_load().replace("900000", "2200000"));
    full_run(b)
}

#[test]
fn a_timed_out_load_run_names_the_timeout_in_the_memory_row() {
    let b = Bundle::new("app-load-timeout", BENCH_PASS);
    // The log has a header only, then the app hangs.
    stub(
        &b.root.join("bin/tarragon"),
        "echo '# perf log' > \"$TARRAGON_PERF_LOG\"\nexec sleep 20\n",
    );
    let out = b.run_env(
        "cores4",
        "true",
        &[b.corpus().to_str().unwrap()],
        &[("TARRAGON_SMOKE_APP_TIMEOUT", "1")],
    );
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains(
            "FAIL peak_rss no peak_rss_kib number in the perf log (missing), timed out after 1 s"
        ),
        "{s}"
    );
    assert!(
        s.contains("FAIL cold_start no cold_start_ms in the perf log, timed out after 1 s"),
        "{s}"
    );
}

#[test]
fn a_hung_psd_bench_is_stopped_and_stays_info() {
    let b = Bundle::new("app-psd-hang", BENCH_PASS);
    stub(&b.root.join("bin/psd_rss_bench"), "exec sleep 20\n");
    let start = std::time::Instant::now();
    let out = b.run_env(
        "cores4",
        "true",
        &[b.corpus().to_str().unwrap()],
        &[("TARRAGON_SMOKE_APP_TIMEOUT", "1")],
    );
    let s = text(&out);
    assert!(
        start.elapsed().as_secs() < 15,
        "the script waited for the bench"
    );
    assert!(out.status.success(), "{s}");
    assert!(
        s.contains("INFO psd_rss_bench exit 124, timed out after 1 s"),
        "{s}"
    );
}

#[test]
fn ctrl_c_removes_the_work_folder_and_the_temp_file() {
    use std::os::unix::process::CommandExt;
    let b = Bundle::new("app-signal", BENCH_PASS);
    let tmp = b.root.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    stub(
        &b.root.join("bin/tarragon"),
        &format!("echo up > {}/started\nexec sleep 20\n", b.root.display()),
    );
    let mut child = b
        .command(
            "cores4",
            "true",
            &[b.corpus().to_str().unwrap()],
            &[("TMPDIR", tmp.to_str().unwrap())],
        )
        .stdout(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .expect("the script starts");
    for _ in 0..100 {
        if b.root.join("started").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(b.root.join("started").exists(), "the app did not start");
    // Like Ctrl-C: the signal goes to the whole process group.
    let group = format!("-{}", child.id());
    let sent = std::process::Command::new("kill")
        .args(["-INT", "--", &group])
        .status()
        .expect("kill runs");
    assert!(sent.success());
    child.wait().expect("the script ends");
    // The inner run cleans up a moment after the outer run ends.
    let mut left: Vec<_> = Vec::new();
    for _ in 0..50 {
        left = fs::read_dir(&tmp).unwrap().flatten().collect();
        if left.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(left.is_empty(), "left behind: {left:?}");
}
