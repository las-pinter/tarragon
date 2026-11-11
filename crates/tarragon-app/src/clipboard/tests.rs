use super::*;

/// A tool that does not exist, so no real program runs.
const MISSING: ToolCommand = ToolCommand {
    program: "tarragon-no-such-tool",
    args: &[],
    package: "some-package",
};

/// A list long enough to fill a pipe, so a write to a stalled tool blocks.
fn long_list() -> Vec<PathBuf> {
    (0..5000)
        .map(|n| PathBuf::from(format!("/home/a/a-long-file-name-{n}.png")))
        .collect()
}

#[test]
fn uri_list_encodes_paths_and_ends_lines_with_crlf() {
    let paths = [
        PathBuf::from("/home/a/my file.png"),
        PathBuf::from("/home/a/café.png"),
    ];

    let text = uri_list(&paths);

    assert_eq!(
        text,
        "file:///home/a/my%20file.png\r\nfile:///home/a/caf%C3%A9.png\r\n"
    );
}

#[test]
fn uri_list_skips_a_path_that_is_not_absolute() {
    let paths = [PathBuf::from("a/b.png"), PathBuf::from("/ok.png")];

    let text = uri_list(&paths);

    assert_eq!(text, "file:///ok.png\r\n");
}

#[test]
fn uri_list_keeps_a_newline_in_a_path_inside_one_line() {
    let path = PathBuf::from("/a/b\nc.png");

    let text = uri_list(std::slice::from_ref(&path));

    assert_eq!(text, "file:///a/b%0Ac.png\r\n");
    assert_eq!(parse_uri_list(text.as_bytes()), [path]);
}

#[test]
fn parse_uri_list_keeps_only_file_entries() {
    let text = b"# a comment\r\n\r\nhttp://example.com/a.png\r\n\
                 file:///home/a/my%20file.png\r\nfile://localhost/home/a/caf%C3%A9.png\r\n";

    let files = parse_uri_list(text);

    assert_eq!(
        files,
        [
            PathBuf::from("/home/a/my file.png"),
            PathBuf::from("/home/a/café.png"),
        ]
    );
}

#[test]
fn parse_uri_list_reads_the_one_slash_form() {
    let files = parse_uri_list(b"file:/home/a/b%20c.png\nfile:relative.png\n");

    assert_eq!(files, [PathBuf::from("/home/a/b c.png")]);
}

#[test]
fn parse_uri_list_skips_remote_host_and_bad_escapes() {
    let text = b"file://server/share/a.png\nfile:///a/%zz.png\nfile:///a/%4\nfile:///a/%00.png\n\
                 file:///ok.png\n";

    let files = parse_uri_list(text);

    assert_eq!(files, [PathBuf::from("/ok.png")]);
}

#[cfg(unix)]
#[test]
fn uri_list_round_trips_a_path_that_is_not_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let path = PathBuf::from(OsString::from_vec(b"/t/\xFF\xFEx y.png".to_vec()));

    let text = uri_list(std::slice::from_ref(&path));

    assert_eq!(text, "file:///t/%FF%FEx%20y.png\r\n");
    assert_eq!(parse_uri_list(text.as_bytes()), [path]);
}

#[test]
fn commands_use_the_wayland_tools_when_the_display_is_set() {
    let wayland = OsStr::new("wayland-0");
    let x11 = OsStr::new(":0");

    let copy = copy_command(Some(wayland), Some(x11)).expect("a command");
    let paste = paste_command(Some(wayland), None).expect("a command");

    assert_eq!(copy.program, "wl-copy");
    assert_eq!(copy.args, ["--type", "text/uri-list"]);
    assert_eq!(copy.package, "wl-clipboard");
    assert_eq!(paste.program, "wl-paste");
    assert_eq!(paste.args, ["--type", "text/uri-list"]);
    assert_eq!(paste.package, "wl-clipboard");
}

#[test]
fn commands_use_xclip_when_only_the_x11_display_is_set() {
    let x11 = OsStr::new(":0");

    let copy = copy_command(None, Some(x11)).expect("a command");
    let paste = paste_command(None, Some(x11)).expect("a command");

    assert_eq!(copy.program, "xclip");
    assert_eq!(
        copy.args,
        ["-selection", "clipboard", "-t", "text/uri-list", "-i"]
    );
    assert_eq!(copy.package, "xclip");
    assert_eq!(paste.program, "xclip");
    assert_eq!(
        paste.args,
        ["-selection", "clipboard", "-o", "-t", "text/uri-list"]
    );
}

#[test]
fn commands_report_no_display_when_neither_display_is_set() {
    assert!(matches!(
        copy_command(None, None),
        Err(ClipboardError::NoDisplay)
    ));
    assert!(matches!(
        paste_command(None, None),
        Err(ClipboardError::NoDisplay)
    ));
}

#[test]
fn copy_with_a_missing_tool_names_the_package() {
    let result = copy_with(&MISSING, &[PathBuf::from("/a.png")], Duration::from_secs(1));

    assert!(matches!(
        result,
        Err(ClipboardError::ToolMissing("some-package"))
    ));
}

#[test]
fn read_with_a_missing_tool_names_the_package() {
    let result = read_with(&MISSING, Duration::from_secs(1));

    assert!(matches!(
        result,
        Err(ClipboardError::ToolMissing("some-package"))
    ));
}

#[cfg(unix)]
#[test]
fn copy_with_a_stalled_tool_times_out() {
    let stalled = ToolCommand {
        program: "sleep",
        args: &["5"],
        package: "coreutils",
    };

    let result = copy_with(&stalled, &long_list(), Duration::from_millis(100));

    assert!(matches!(result, Err(ClipboardError::Timeout)));
}

#[cfg(unix)]
#[test]
fn copy_with_a_failing_tool_reports_the_exit_status_not_the_write_error() {
    let failing = ToolCommand {
        program: "false",
        args: &[],
        package: "coreutils",
    };

    let result = copy_with(&failing, &long_list(), Duration::from_secs(2));

    assert!(matches!(result, Err(ClipboardError::Failed { .. })));
}

#[cfg(unix)]
#[test]
fn read_with_a_silent_tool_times_out() {
    let silent = ToolCommand {
        program: "sleep",
        args: &["5"],
        package: "coreutils",
    };

    let result = read_with(&silent, Duration::from_millis(100));

    assert!(matches!(result, Err(ClipboardError::Timeout)));
}

#[cfg(unix)]
#[test]
fn read_with_a_tool_that_closes_its_output_and_keeps_running_times_out() {
    let lingering = ToolCommand {
        program: "/bin/sh",
        args: &["-c", "exec >&-; exec sleep 5"],
        package: "coreutils",
    };

    let result = read_with(&lingering, Duration::from_millis(200));

    assert!(matches!(result, Err(ClipboardError::Timeout)));
}

#[cfg(unix)]
#[test]
fn read_with_a_failing_tool_gives_an_empty_list() {
    let failing = ToolCommand {
        program: "false",
        args: &[],
        package: "coreutils",
    };

    let result = read_with(&failing, Duration::from_secs(2));

    assert!(matches!(result, Ok(files) if files.is_empty()));
}

#[cfg(unix)]
#[test]
fn read_with_a_tool_that_prints_a_list_returns_the_files() {
    let printing = ToolCommand {
        program: "printf",
        args: &["file:///a/b%%20c.png\\r\\n"],
        package: "coreutils",
    };

    let result = read_with(&printing, Duration::from_secs(2));

    assert!(matches!(result, Ok(files) if files == [PathBuf::from("/a/b c.png")]));
}
