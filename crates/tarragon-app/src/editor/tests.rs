use std::path::PathBuf;

use super::*;

/// A temporary folder that removes itself on drop.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A database with the given `(extension, template)` rows.
fn db_with(rows: &[(&str, &str)]) -> (TempDir, Database) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tarragon-app-editor-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    let dir = TempDir(dir);
    let db = Database::open(dir.0.join("t.db")).expect("open");
    db.initialize().expect("initialize");
    for (extension, template) in rows {
        db.upsert_editor_association(extension, template)
            .expect("upsert");
    }
    (dir, db)
}

fn os(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

#[test]
fn extension_template_keeps_spaces_and_ignores_case() {
    let (_dir, db) = db_with(&[("psd", "krita \"{file}\"")]);
    let launch = resolve(&db, Path::new("/a/b c/x.PSD")).expect("resolve");
    assert_eq!(launch.program, OsString::from("krita"));
    assert_eq!(launch.args, os(&["/a/b c/x.PSD"]));
}

#[test]
fn fallback_template_serves_other_extensions() {
    let (_dir, db) = db_with(&[("*", "gimp --new {file}")]);
    let launch = resolve(&db, Path::new("/a/x.png")).expect("resolve");
    assert_eq!(launch.program, OsString::from("gimp"));
    assert_eq!(launch.args, os(&["--new", "/a/x.png"]));
}

#[test]
fn extension_template_beats_fallback() {
    let (_dir, db) = db_with(&[("*", "gimp {file}"), ("psd", "krita {file}")]);
    let launch = resolve(&db, Path::new("/a/x.psd")).expect("resolve");
    assert_eq!(launch.program, OsString::from("krita"));
}

#[test]
fn template_without_placeholder_gets_path_last() {
    let (_dir, db) = db_with(&[("png", "viewer")]);
    let launch = resolve(&db, Path::new("/a/x.png")).expect("resolve");
    assert_eq!(launch.program, OsString::from("viewer"));
    assert_eq!(launch.args, os(&["/a/x.png"]));
}

#[test]
fn placeholder_inside_an_argument_is_replaced() {
    let (_dir, db) = db_with(&[("png", "ed --open={file} -v")]);
    let launch = resolve(&db, Path::new("/a/x.png")).expect("resolve");
    assert_eq!(launch.args, os(&["--open=/a/x.png", "-v"]));
}

#[cfg(target_os = "linux")]
#[test]
fn no_template_gives_the_system_opener() {
    let (_dir, db) = db_with(&[]);
    let launch = resolve(&db, Path::new("/a/x.png")).expect("resolve");
    assert_eq!(launch.program, OsString::from("xdg-open"));
    assert_eq!(launch.args, os(&["/a/x.png"]));
}

#[cfg(target_os = "linux")]
#[test]
fn blank_template_gives_the_system_opener() {
    let (_dir, db) = db_with(&[("png", "   "), ("*", "")]);
    let launch = resolve(&db, Path::new("/a/x.png")).expect("resolve");
    assert_eq!(launch.program, OsString::from("xdg-open"));
}

#[test]
fn blank_extension_template_falls_through_to_fallback() {
    let (_dir, db) = db_with(&[("png", " "), ("*", "gimp")]);
    let launch = resolve(&db, Path::new("/a/x.png")).expect("resolve");
    assert_eq!(launch.program, OsString::from("gimp"));
}

#[test]
fn file_without_extension_uses_only_the_fallback() {
    let (_dir, db) = db_with(&[("png", "viewer"), ("*", "gimp")]);
    let launch = resolve(&db, Path::new("/a/README")).expect("resolve");
    assert_eq!(launch.program, OsString::from("gimp"));
}

#[test]
fn empty_program_after_split_is_ignored() {
    let (_dir, db) = db_with(&[("png", "\"\" {file}"), ("*", "gimp")]);
    let launch = resolve(&db, Path::new("/a/x.png")).expect("resolve");
    assert_eq!(launch.program, OsString::from("gimp"));
}

#[test]
fn split_handles_quotes_and_white_space() {
    assert_eq!(split_template("  a  \"b c\" d\te "), ["a", "b c", "d", "e"]);
    assert_eq!(split_template("x\"y z\"w"), ["xy zw"]);
    assert_eq!(split_template("a \"\" b"), ["a", "", "b"]);
    assert_eq!(split_template(" \t ").len(), 0);
}

#[test]
fn split_keeps_the_rest_of_an_unclosed_quote() {
    assert_eq!(split_template("a \"b c d"), ["a", "b c d"]);
}

#[test]
fn placeholder_in_the_program_part_does_not_count() {
    let (_dir, db) = db_with(&[("png", "{file}x viewer")]);
    let launch = resolve(&db, Path::new("/a/x.png")).expect("resolve");
    assert_eq!(launch.program, OsString::from("{file}x"));
    assert_eq!(launch.args, os(&["viewer", "/a/x.png"]));
}

#[test]
fn spawn_of_a_missing_program_is_not_found() {
    let launch = Launch {
        program: "tarragon-no-such-editor".into(),
        args: vec![],
    };
    let error = spawn(&launch).expect_err("must fail");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[test]
fn open_of_a_missing_program_names_it() {
    let (_dir, db) = db_with(&[("*", "tarragon-no-such-editor {file}")]);
    let error = open(&db, Path::new("/a/x.png")).expect_err("must fail");
    assert!(matches!(error, OpenError::Spawn { .. }));
    assert!(error.to_string().contains("tarragon-no-such-editor"));
}

/// Wait up to three seconds for `path` to hold a full line.
#[cfg(unix)]
fn wait_for_text(path: &Path) -> Option<String> {
    for _ in 0..60 {
        if let Ok(text) = std::fs::read_to_string(path)
            && text.ends_with('\n')
        {
            return Some(text);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    None
}

#[cfg(unix)]
#[test]
fn spawn_returns_before_the_child_ends() {
    let launch = Launch {
        program: "sh".into(),
        args: os(&["-c", "sleep 1"]),
    };
    let start = std::time::Instant::now();
    spawn(&launch).expect("spawn");
    assert!(start.elapsed() < std::time::Duration::from_millis(500));
    let launch = Launch {
        program: "true".into(),
        args: vec![],
    };
    spawn(&launch).expect("spawn true");
}

#[cfg(unix)]
#[test]
fn open_runs_the_fallback_template() {
    let (dir, db) = db_with(&[]);
    let out = dir.0.join("out.txt");
    let template = format!("sh -c \"echo $0 > {}\" {{file}}", out.display());
    db.upsert_editor_association("*", &template)
        .expect("upsert");
    open(&db, Path::new("/a/x.png")).expect("open");
    assert_eq!(wait_for_text(&out).as_deref(), Some("/a/x.png\n"));
}

#[cfg(target_os = "linux")]
#[test]
fn open_without_template_runs_the_system_opener() {
    use std::os::unix::fs::PermissionsExt;

    let (dir, db) = db_with(&[]);
    let out = dir.0.join("arg.txt");
    let bin = dir.0.join("bin");
    std::fs::create_dir_all(&bin).expect("create bin");
    let script = bin.join("xdg-open");
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho \"$1\" > {}\n", out.display()),
    )
    .expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    open_with_path(&db, Path::new("/a/x.png"), Some(bin.as_os_str())).expect("open");
    assert_eq!(wait_for_text(&out).as_deref(), Some("/a/x.png\n"));
}
