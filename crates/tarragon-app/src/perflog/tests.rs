use super::*;

const SEC: Duration = Duration::from_secs(1);

fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tarragon-perflog-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    dir.join("perf.txt")
}

fn frame_at(now: Instant, loaded: bool, textures: usize) -> Frame {
    Frame {
        now,
        window: [1280.0, 800.0],
        renderer: "glow",
        shown: loaded,
        loaded,
        textures,
    }
}

#[test]
fn the_session_type_wins_and_the_display_variables_fill_in() {
    assert_eq!(session_text(Some("wayland"), None, Some(":0")), "wayland");
    assert_eq!(session_text(Some("x11"), Some("wayland-0"), None), "x11");
    assert_eq!(session_text(None, Some("wayland-0"), Some(":0")), "wayland");
    assert_eq!(session_text(None, None, Some(":0")), "x11");
    assert_eq!(session_text(Some("tty"), None, Some(":0")), "x11");
    assert_eq!(session_text(None, Some(""), Some("")), "unknown");
}

#[test]
fn a_run_is_valid_only_when_all_three_limits_hold() {
    let big = [1280.0, 800.0];
    assert!(run_valid(big, 30 * SEC, 900));
    assert!(!run_valid([1279.0, 800.0], 30 * SEC, 900));
    assert!(!run_valid([1280.0, 799.0], 30 * SEC, 900));
    assert!(!run_valid(big, 29 * SEC, 900));
    assert!(!run_valid(big, 30 * SEC, 899));
}

#[test]
fn the_lines_have_the_key_value_format() {
    assert_eq!(
        header_lines("x11", "wgpu", [1200.4, 800.0]),
        "# perf log\nsession: x11\nrenderer: wgpu\nwindow: 1200x800\n"
    );
    assert_eq!(
        cold_lines(950, "spawn"),
        "cold_start_ms: 950\ncold_start_basis: spawn\n"
    );
    assert_eq!(
        loaded_lines(Some(5120), 300),
        "peak_rss_kib: 5120\nlive_textures: 300\n"
    );
    assert!(loaded_lines(None, 0).contains("peak_rss_kib: na\n"));
    assert_eq!(
        end_lines(None, 12, false),
        "frames: too few\nlive_textures_at_end: 12\nrun_valid: no\n"
    );
}

#[test]
fn the_spawn_age_is_uptime_minus_the_start_ticks() {
    // The name holds a space and a bracket; field 22 is 1500 ticks (15 s).
    let stat = "42 (a) b) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 1500 23";
    let age = spawn_age(stat, "20.50 99.00").expect("an age");
    assert_eq!(age, Duration::from_millis(5500));
    assert_eq!(spawn_age("garbage", "20.50 1.0"), None);
    assert_eq!(spawn_age(stat, "10.00 1.0"), None);
}

#[test]
fn the_peak_memory_is_read_from_the_status_text() {
    let status = "Name:\tx\nVmHWM:\t  4321 kB\nVmRSS:\t 100 kB\n";
    assert_eq!(vm_hwm_kib(status), Some(4321));
    assert_eq!(vm_hwm_kib("Name:\tx\n"), None);
}

#[test]
fn no_variable_means_no_log_and_no_frame_run() {
    assert_eq!(from_vars(None, false), (None, false));
    assert_eq!(from_vars(Some(OsString::new()), false), (None, false));
    let (path, run) = from_vars(Some("f.txt".into()), true);
    assert_eq!(path, Some(PathBuf::from("f.txt")));
    assert!(run);
    assert!(PerfLog::new(None, true).is_none());
}

#[test]
fn the_file_is_cut_at_the_first_write_and_then_appended() {
    let path = temp_file("cut");
    std::fs::write(&path, "old content\n").expect("seed the file");
    let mut log = PerfLog::new(Some(path.clone()), false).expect("a log");
    assert_eq!(log.frame(&frame_at(Instant::now(), false, 0)), Step::Idle);
    assert_eq!(log.frame(&frame_at(Instant::now(), true, 7)), Step::Idle);
    let text = std::fs::read_to_string(&path).expect("read the log");
    assert!(text.starts_with("# perf log\nsession: "), "{text}");
    assert!(
        text.contains("renderer: glow\nwindow: 1280x800\n"),
        "{text}"
    );
    assert!(!text.contains("old content"), "{text}");
    for key in ["cold_start_ms: ", "peak_rss_kib: ", "live_textures: 7\n"] {
        assert!(text.contains(key), "{key} missing in {text}");
    }
    assert_eq!(text.matches("# perf log").count(), 1);
}

#[test]
fn a_write_error_does_not_panic() {
    let path = temp_file("bad").join("no-dir").join("perf.txt");
    let mut log = PerfLog::new(Some(path.clone()), false).expect("a log");
    assert_eq!(log.frame(&frame_at(Instant::now(), true, 1)), Step::Idle);
    assert!(!path.exists());
}

#[test]
fn the_frame_run_waits_records_and_closes() {
    let path = temp_file("run");
    let mut log = PerfLog::new(Some(path.clone()), true).expect("a log");
    let t0 = Instant::now();
    assert_eq!(log.frame(&frame_at(t0, true, 950)), Step::Wait(5 * SEC));
    let settled = t0 + 5 * SEC;
    assert_eq!(
        log.frame(&frame_at(t0 + SEC, false, 950)),
        Step::Wait(4 * SEC)
    );
    // 10 ms frames: 5 s of warm-up, then 30 s of run.
    let mut step = Step::Repaint;
    for i in 0..=3600 {
        step = log.frame(&frame_at(
            settled + Duration::from_millis(10 * i),
            false,
            950,
        ));
        if step == Step::Close {
            break;
        }
        assert_eq!(step, Step::Repaint);
    }
    assert_eq!(step, Step::Close);
    let text = std::fs::read_to_string(&path).expect("read the log");
    assert!(text.contains("p99_ms: 10.00\n"), "{text}");
    assert!(
        text.ends_with("live_textures_at_end: 950\nrun_valid: yes\n"),
        "{text}"
    );
    // Later frames write nothing more.
    assert_eq!(
        log.frame(&frame_at(settled + 99 * SEC, false, 950)),
        Step::Idle
    );
    assert_eq!(std::fs::read_to_string(&path).expect("read"), text);
}

#[test]
fn the_cold_start_is_written_when_the_grid_is_shown_before_it_is_loaded() {
    let path = temp_file("shown");
    let mut log = PerfLog::new(Some(path.clone()), false).expect("a log");
    let shown = Frame {
        shown: true,
        loaded: false,
        ..frame_at(Instant::now(), false, 0)
    };
    log.frame(&shown);
    let text = std::fs::read_to_string(&path).expect("read the log");
    assert!(text.contains("cold_start_ms: "), "{text}");
    assert!(!text.contains("peak_rss_kib"), "{text}");
    // The load frame adds its lines and does not write the cold start twice.
    log.frame(&frame_at(Instant::now(), true, 5));
    let text = std::fs::read_to_string(&path).expect("read the log");
    assert_eq!(text.matches("cold_start_ms: ").count(), 1, "{text}");
    assert!(text.contains("peak_rss_kib: "), "{text}");
    assert!(text.contains("live_textures: 5\n"), "{text}");
}
