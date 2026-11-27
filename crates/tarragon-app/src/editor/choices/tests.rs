use std::path::Path;

use super::*;
use crate::editor::tests::db_with;

#[test]
fn the_program_gives_the_app_name() {
    assert_eq!(app_name("krita {file}").as_deref(), Some("Krita"));
    assert_eq!(
        app_name("\"/opt/gimp/bin/gimp-2.10\" {file}").as_deref(),
        Some("Gimp-2.10")
    );
    assert_eq!(app_name("Krita.EXE {file}").as_deref(), Some("Krita"));
    assert_eq!(app_name("   ").as_deref(), None);
}

#[test]
fn with_no_row_the_default_app_is_the_only_choice() {
    let (_dir, db) = db_with(&[]);
    let found = choices(&db, Path::new("/a/x.png")).expect("choices");
    assert_eq!(found, [Choice::Default]);
    assert_eq!(found[0].label(), DEFAULT_APP);
}

#[test]
fn the_extension_row_comes_first_then_the_fallback_then_the_default() {
    let (_dir, db) = db_with(&[("psd", "krita {file}"), ("*", "gimp {file}")]);
    let found = choices(&db, Path::new("/a/x.PSD")).expect("choices");
    let labels: Vec<&str> = found.iter().map(Choice::label).collect();
    assert_eq!(labels, ["Krita", "Gimp", DEFAULT_APP]);
}

#[test]
fn a_fallback_with_the_same_template_shows_once() {
    let (_dir, db) = db_with(&[("png", "krita {file}"), ("*", "krita {file}")]);
    let found = choices(&db, Path::new("/a/x.png")).expect("choices");
    assert_eq!(found.len(), 2);
}

#[cfg(unix)]
#[test]
fn a_template_choice_runs_its_template() {
    let (dir, _db) = db_with(&[]);
    let out = dir.0.join("out.txt");
    let choice = Choice::Template {
        name: "Sh".into(),
        template: format!("sh -c \"echo $0 > {}\" {{file}}", out.display()),
    };
    open_with_path(Path::new("/a/x.png"), &choice, None).expect("open");
    let text = crate::editor::tests::wait_for_text(&out);
    assert_eq!(text.as_deref(), Some("/a/x.png\n"));
}

#[cfg(target_os = "linux")]
#[test]
fn the_default_choice_runs_the_system_opener() {
    use std::os::unix::fs::PermissionsExt;

    let (dir, _db) = db_with(&[]);
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
    open_with_path(
        Path::new("/a/x.png"),
        &Choice::Default,
        Some(bin.as_os_str()),
    )
    .expect("open");
    let text = crate::editor::tests::wait_for_text(&out);
    assert_eq!(text.as_deref(), Some("/a/x.png\n"));
}
