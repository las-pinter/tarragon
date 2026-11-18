//! Runs the built binary with command-line flags. Every flag returns before
//! the binary reads or writes any data.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tarragon"))
        .args(args)
        .output()
        .expect("the binary starts")
}

#[test]
fn version_flag_prints_name_and_version() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    assert_eq!(out.stderr.len(), 0);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        format!("tarragon {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_flag_prints_usage_and_exits_zero() {
    for flag in ["--help", "-h"] {
        let out = run(&[flag]);
        assert!(out.status.success());
        assert_eq!(out.stderr.len(), 0);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("usage:"));
        assert!(stdout.contains("--version"));
    }
}

#[test]
fn unknown_flag_fails_with_usage_on_stderr() {
    let out = run(&["--bogus"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("usage:"));
    assert!(stderr.contains("--bogus"));
    assert_eq!(out.stdout.len(), 0);
}
