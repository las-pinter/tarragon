use std::path::{Path, PathBuf};

use super::*;

const ENTRY: &str = "/usr/share/applications/app.desktop";

/// Parse with the name `App`, no icon, and the entry path `ENTRY`.
fn parse(exec: &str) -> Option<ExecLine> {
    ExecLine::parse(exec, "App", None, Path::new(ENTRY))
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn os(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

fn paths(items: &[&str]) -> Vec<PathBuf> {
    items.iter().map(PathBuf::from).collect()
}

#[test]
fn many_files_give_one_launch_with_every_path() {
    let line = parse("gimp %F").expect("parse");
    let launch = line.launch(&paths(&["/a.png", "/b.png", "/c.png"]));
    assert_eq!(launch.program, OsString::from("gimp"));
    assert_eq!(launch.args, os(&["/a.png", "/b.png", "/c.png"]));
    assert_eq!(line.list, ListKind::Many);
    assert!(!line.uri);
}

#[test]
fn single_file_codes_give_the_one_kind() {
    let path = parse("eog %f").expect("parse");
    assert_eq!(path.list, ListKind::One);
    assert!(!path.uri);
    let uri = parse("eog %u").expect("parse");
    assert_eq!(uri.list, ListKind::One);
    assert!(uri.uri);
}

#[test]
fn env_and_quoted_program_keep_their_order() {
    let line = parse(r#"env A=b "/opt/My App/run" --x %U"#).expect("parse");
    assert_eq!(line.program, "env");
    assert_eq!(line.before, strings(&["A=b", "/opt/My App/run", "--x"]));
    assert_eq!(line.after, strings(&[]));
    assert_eq!(line.list, ListKind::Many);
    assert!(line.uri);
}

#[test]
fn flatpak_markers_pass_through_around_the_slot() {
    let line = parse("flatpak run --file-forwarding org.gimp.GIMP @@ %F @@").expect("parse");
    assert_eq!(line.program, "flatpak");
    assert_eq!(
        line.before,
        strings(&["run", "--file-forwarding", "org.gimp.GIMP", "@@"])
    );
    assert_eq!(line.after, strings(&["@@"]));
    let launch = line.launch(&paths(&["/a.png"]));
    assert_eq!(
        launch.args,
        os(&[
            "run",
            "--file-forwarding",
            "org.gimp.GIMP",
            "@@",
            "/a.png",
            "@@"
        ])
    );
}

#[test]
fn icon_name_entry_path_and_percent_expand() {
    let line = ExecLine::parse(
        "app %i --title=%c --from %k --rate=100%% %f",
        "My App",
        Some("app-icon"),
        Path::new(ENTRY),
    )
    .expect("parse");
    assert_eq!(
        line.before,
        strings(&[
            "--icon",
            "app-icon",
            "--title=My App",
            "--from",
            ENTRY,
            "--rate=100%"
        ])
    );
}

#[test]
fn icon_code_gives_nothing_without_an_icon() {
    let none = parse("app %i %f").expect("parse");
    assert_eq!(none.before, strings(&[]));
    let empty = ExecLine::parse("app %i %f", "App", Some(""), Path::new(ENTRY)).expect("parse");
    assert_eq!(empty.before, strings(&[]));
}

#[test]
fn deprecated_codes_give_nothing() {
    let line = parse("app %d %D %n %N %v %m x%d %f").expect("parse");
    assert_eq!(line.before, strings(&["x"]));
}

#[test]
fn only_the_first_file_code_marks_the_slot() {
    let line = parse("app a %u b %F c %f").expect("parse");
    assert_eq!(line.list, ListKind::One);
    assert!(line.uri);
    assert_eq!(line.before, strings(&["a"]));
    assert_eq!(line.after, strings(&["b", "c"]));
}

#[test]
fn uri_kind_percent_encodes_the_path() {
    let line = parse("app %U").expect("parse");
    let launch = line.launch(&paths(&["/a/b c/\u{e9}.png"]));
    assert_eq!(launch.args, os(&["file:///a/b%20c/%C3%A9.png"]));
}

#[test]
fn uri_kind_keeps_a_relative_path_plain() {
    let line = parse("app %u").expect("parse");
    let launch = line.launch(&paths(&["rel/a b.png"]));
    assert_eq!(launch.args, os(&["rel/a b.png"]));
}

#[test]
fn a_line_without_a_file_code_gives_none() {
    assert_eq!(parse("app --flag %i %c"), None);
}

#[test]
fn an_empty_program_gives_none() {
    assert_eq!(parse(""), None);
    assert_eq!(parse("   "), None);
    assert_eq!(parse("%F"), None);
    assert_eq!(parse(r#""" %F"#), None);
}

#[test]
fn quoted_escapes_drop_one_backslash() {
    let line = parse(r#"app "a\"b\`c\$d\\e\xf" %f"#).expect("parse");
    assert_eq!(line.before, strings(&[r#"a"b`c$d\e\xf"#]));
}

#[test]
fn a_backslash_outside_quotes_stays() {
    let line = parse(r"app a\\b %f").expect("parse");
    assert_eq!(line.before, strings(&[r"a\\b"]));
}

#[test]
fn a_quoted_empty_argument_stays() {
    let line = parse(r#"app "" %f"#).expect("parse");
    assert_eq!(line.before, strings(&[""]));
}

#[test]
fn text_from_an_expansion_is_not_scanned_again() {
    let line = ExecLine::parse("app %c %f", "100%d %F", None, Path::new(ENTRY)).expect("parse");
    assert_eq!(line.before, strings(&["100%d %F"]));
}

#[test]
fn an_unusable_line_gives_none() {
    assert_eq!(parse(r#"app %f "x"#), None, "unclosed quote");
    assert_eq!(parse("app %z %f"), None, "unknown code");
    assert_eq!(parse("app %f %"), None, "lone percent");
    assert_eq!(
        parse("app --file=%F"),
        None,
        "file code in a longer argument"
    );
    assert_eq!(
        parse("app --icon=%i %f"),
        None,
        "icon code in a longer argument"
    );
}
