//! Runs `tests/smoke/smoke.sh` with stub tools, so no real corpus, scan,
//! `sudo`, or window is involved. The stubs sit in a fake bundle folder. The
//! script reads the CPU tables and the drop command from its test overrides.
//! The app rows are tested in `smoke_script_app.rs`.

// The script needs Linux tools such as `free`, `lscpu` and `taskset`.
#![cfg(target_os = "linux")]

#[allow(
    dead_code,
    reason = "shared with the app-row tests; each file uses part of it"
)]
#[path = "support/smoke_bundle.rs"]
mod smoke_bundle;

use smoke_bundle::{BENCH_EXIT_FAIL, BENCH_NO_TIME, BENCH_PASS, BENCH_RESULT_FAIL, Bundle, text};
use std::fs;
use std::process::Output;

#[test]
fn check_only_prints_pin_and_memory_without_corpus() {
    let b = Bundle::new("check-only", BENCH_PASS);
    let corpus = b.corpus();
    let out = b.run(
        "cores4",
        "false",
        &["--check-only", corpus.to_str().unwrap()],
    );
    let s = text(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("INFO memory"), "{s}");
    assert!(s.contains("INFO pin cpus 0,2,4,6 (P-core)"), "{s}");
    assert!(!s.contains("COLD"), "{s}");
    assert!(!corpus.exists(), "check-only made a corpus");
    assert!(!b.root.join("taskset.log").exists());
}

#[test]
fn fewer_than_four_cores_fails_pin() {
    let b = Bundle::new("two-cores", BENCH_PASS);
    let out = b.run("cores2", "true", &["--check-only"]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("FAIL pin"), "{s}");
}

#[test]
fn failed_drop_gives_partly_cold_and_fails_scan() {
    let b = Bundle::new("partly", BENCH_PASS);
    let out = b.run("cores4", "false", &[b.corpus().to_str().unwrap()]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("INFO cache PARTLY COLD"), "{s}");
    assert!(s.contains("FAIL scan_cold 1.500 s"), "{s}");
    assert!(s.contains("SUMMARY pass=3 fail=2"), "{s}");
}

#[test]
fn cold_drop_and_passing_bench_exits_zero_under_the_pin() {
    let b = Bundle::new("cold", BENCH_PASS);
    let out = b.run("cores4", "true", &[b.corpus().to_str().unwrap()]);
    let s = text(&out);
    assert!(out.status.success(), "{s}");
    assert!(
        s.contains("COLD STEP: sudo will ask for your password"),
        "{s}"
    );
    assert!(s.contains("INFO cache COLD"), "{s}");
    assert!(s.contains("PASS scan_cold 1.500 s"), "{s}");
    let log = fs::read_to_string(b.root.join("taskset.log")).expect("taskset ran");
    assert!(log.starts_with("-c 0,2,4,6 "), "{log}");
}

/// Run a cold scan against a bench body and return the output text.
fn cold_scan(name: &str, bench: &str) -> (Output, String) {
    let b = Bundle::new(name, bench);
    let out = b.run("cores4", "true", &[b.corpus().to_str().unwrap()]);
    let s = text(&out);
    (out, s)
}

#[test]
fn bench_result_fail_with_exit_zero_fails_scan() {
    let (out, s) = cold_scan("result-fail", BENCH_RESULT_FAIL);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("FAIL scan_cold 1.500 s"), "{s}");
    assert!(s.contains("INFO cache COLD"), "{s}");
}

#[test]
fn bench_nonzero_exit_with_pass_result_fails_scan() {
    let (out, s) = cold_scan("exit-fail", BENCH_EXIT_FAIL);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("FAIL scan_cold 1.500 s"), "{s}");
}

#[test]
fn bench_without_time_line_fails_scan() {
    let (out, s) = cold_scan("no-time", BENCH_NO_TIME);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("FAIL scan_cold bench gave no time"), "{s}");
}

#[test]
fn corpus_gen_and_scan_bench_get_the_right_arguments() {
    let b = Bundle::new("args", BENCH_PASS);
    let corpus = b.corpus();
    let dir = corpus.to_str().unwrap();
    let out = b.run_env("cores4", "true", &[dir], &[("TARRAGON_SMOKE_FILES", "7")]);
    assert!(out.status.success(), "{}", text(&out));
    let gen_log = fs::read_to_string(b.root.join("corpus_gen.log")).expect("corpus_gen ran");
    let psd = b.root.join("fixtures/rgb_8bit.psd");
    assert_eq!(
        gen_log.trim_end(),
        format!(
            "--out {dir} --files 7 --psd-file {} --psd-percent 10",
            psd.display()
        )
    );
    let scan_log = fs::read_to_string(b.root.join("scan_bench.log")).expect("scan_bench ran");
    assert_eq!(scan_log.trim_end(), format!("--folder {dir}"));
}

#[test]
fn folder_with_the_expected_file_count_is_reused() {
    let b = Bundle::new("reuse", BENCH_PASS);
    let corpus = b.corpus();
    fs::create_dir_all(&corpus).expect("make folder");
    for i in 0..3 {
        fs::write(corpus.join(format!("f{i}.png")), b"x").expect("write file");
    }
    let out = b.run_env(
        "cores4",
        "true",
        &[corpus.to_str().unwrap()],
        &[("TARRAGON_SMOKE_FILES", "3")],
    );
    let s = text(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("INFO corpus reused 3 files"), "{s}");
    assert!(s.contains("COLD STEP"), "{s}");
    assert!(s.contains("PASS scan_cold"), "{s}");
    assert!(
        !b.root.join("corpus_gen.log").exists(),
        "corpus_gen ran on a reused folder"
    );
    assert_eq!(fs::read_dir(&corpus).unwrap().count(), 3);
}

#[test]
fn folder_with_a_wrong_file_count_fails_with_the_real_count() {
    let b = Bundle::new("wrong-count", BENCH_PASS);
    let corpus = b.corpus();
    fs::create_dir_all(&corpus).expect("make folder");
    fs::write(corpus.join("a.png"), b"x").expect("write file");
    let out = b.run_env(
        "cores4",
        "true",
        &[corpus.to_str().unwrap()],
        &[("TARRAGON_SMOKE_FILES", "3")],
    );
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(
        s.contains("has 1 files, expected 3; remove the folder"),
        "{s}"
    );
    assert!(!s.contains("COLD STEP"), "{s}");
    assert!(corpus.join("a.png").exists());
}

#[test]
fn corpus_gen_failure_has_its_own_text() {
    let b = Bundle::new("gen-fails", BENCH_PASS);
    fs::remove_file(b.root.join("bin/corpus_gen")).expect("remove stub");
    let out = b.run("cores4", "true", &[b.corpus().to_str().unwrap()]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("FAIL corpus corpus_gen failed for"), "{s}");
    assert!(s.contains("FAIL scan_cold no corpus"), "{s}");
    assert!(!s.contains("COLD STEP"), "{s}");
}

#[test]
fn set_overrides_are_listed_right_after_the_memory_row() {
    let b = Bundle::new("override", BENCH_PASS);
    let out = b.run_env(
        "cores4",
        "true",
        &["--check-only"],
        &[("TARRAGON_SMOKE_FILES", "5")],
    );
    let s = text(&out);
    assert!(out.status.success(), "{s}");
    let lines: Vec<&str> = s.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == "INFO memory")
        .expect("memory row");
    assert!(
        lines[at + 1].starts_with("INFO override TARRAGON_SMOKE_"),
        "{s}"
    );
    for name in [
        "TARRAGON_SMOKE_BASE",
        "TARRAGON_SMOKE_DROP_CMD",
        "TARRAGON_SMOKE_FILES",
    ] {
        assert!(
            s.contains(&format!("INFO override {name}\n")),
            "{name}: {s}"
        );
    }
}

#[test]
fn unknown_option_and_second_folder_exit_two() {
    let b = Bundle::new("usage", BENCH_PASS);
    for args in [&["--bogus"][..], &["/tmp/one", "/tmp/two"][..]] {
        let out = b.run("cores4", "true", args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("usage:"));
        assert!(!text(&out).contains("INFO memory"));
    }
}

#[test]
fn core_type_is_unknown_without_frequencies() {
    let b = Bundle::new("unknown-type", BENCH_PASS);
    b.set_freq("CPU CORE MAXMHZ\n0 0 -\n1 0 -\n2 1 -\n3 1 -\n4 2 -\n5 2 -\n6 3 -\n7 3 -\n");
    let s = text(&b.run("cores4", "true", &["--check-only"]));
    assert!(s.contains("INFO pin cpus 0,2,4,6 (unknown)"), "{s}");
}

#[test]
fn core_type_is_mixed_when_a_pinned_cpu_is_slow() {
    let b = Bundle::new("mixed-type", BENCH_PASS);
    // Cores 0, 1, 2 are fast. The fourth core is CPU 8, which is slow.
    fs::write(
        b.root.join("cores_mixed"),
        "# CPU,Core\n0,0\n1,1\n2,2\n8,3\n",
    )
    .expect("cores");
    let s = text(&b.run("cores_mixed", "true", &["--check-only"]));
    assert!(s.contains("INFO pin cpus 0,1,2,8 (mixed)"), "{s}");
}

#[test]
fn folder_with_files_is_left_alone_and_skips_the_scan() {
    let b = Bundle::new("keep", BENCH_PASS);
    let corpus = b.corpus();
    fs::create_dir_all(&corpus).expect("make folder");
    fs::write(corpus.join("mine.txt"), b"keep").expect("write file");
    let out = b.run("cores4", "true", &[corpus.to_str().unwrap()]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("FAIL corpus"), "{s}");
    assert!(s.contains("FAIL scan_cold"), "{s}");
    assert!(!s.contains("COLD STEP"), "{s}");
    assert_eq!(fs::read(corpus.join("mine.txt")).unwrap(), b"keep");
}
