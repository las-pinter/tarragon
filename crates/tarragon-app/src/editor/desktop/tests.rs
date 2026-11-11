use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::*;

/// A temporary folder that removes itself on drop.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An empty temporary folder.
fn temp_dir() -> TempDir {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("tarragon-app-desktop-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    TempDir(dir)
}

/// Write `text` to `<share>/applications/<name>`, with the folders made.
fn write_entry(share: &Path, name: &str, text: &str) {
    let file = share.join("applications").join(name);
    std::fs::create_dir_all(file.parent().expect("parent")).expect("create dirs");
    std::fs::write(file, text).expect("write entry");
}

/// An `Env` with temp folders only, so no test reads the real data folders.
fn env(home: &Path, dirs: &[&Path]) -> Env {
    Env {
        path: Some(OsString::new()),
        data_home: Some(home.to_owned()),
        data_dirs: dirs.iter().map(|dir| (*dir).to_owned()).collect(),
        timeout: Duration::from_secs(1),
    }
}

/// An entry text with the given lines after the group header.
fn entry(lines: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName=Fake\n{lines}\n")
}

/// One temp folder as `data_home`, and an `Env` for it alone.
fn single() -> (TempDir, Env) {
    let home = temp_dir();
    let env = env(&home.0, &[]);
    (home, env)
}

#[test]
fn entry_loads_and_two_files_give_one_start() {
    let (home, env) = single();
    write_entry(&home.0, "fake.desktop", &entry("Exec=/x/fake %F"));
    let app = load("fake.desktop", &env).expect("loads");
    assert_eq!(app.id, "fake.desktop");
    let launch = app
        .exec
        .launch(&[PathBuf::from("/a/1.png"), PathBuf::from("/a/2.png")]);
    assert_eq!(launch.program, OsString::from("/x/fake"));
    assert_eq!(launch.args.len(), 2);
}

#[test]
fn name_icon_and_entry_path_reach_the_exec_line() {
    let (home, env) = single();
    write_entry(
        &home.0,
        "fake.desktop",
        &entry("Icon=pic\nExec=/x/fake %i %c %k %F"),
    );
    let app = load("fake.desktop", &env).expect("loads");
    let entry_path = home.0.join("applications/fake.desktop");
    assert_eq!(
        app.exec.before,
        vec![
            "--icon".to_owned(),
            "pic".to_owned(),
            "Fake".to_owned(),
            entry_path.to_string_lossy().into_owned(),
        ]
    );
}

#[test]
fn data_home_wins_over_data_dirs() {
    let (home, first, second) = (temp_dir(), temp_dir(), temp_dir());
    write_entry(&second.0, "fake.desktop", &entry("Exec=/x/second %F"));
    write_entry(&first.0, "fake.desktop", &entry("Exec=/x/first %F"));
    write_entry(&home.0, "fake.desktop", &entry("Exec=/x/home %F"));
    let env = env(&home.0, &[&first.0, &second.0]);
    assert_eq!(
        load("fake.desktop", &env).expect("loads").exec.program,
        "/x/home"
    );
    assert_eq!(
        locate("fake.desktop", &env),
        Some(home.0.join("applications/fake.desktop"))
    );
}

#[test]
fn first_data_dir_wins_over_a_later_one() {
    let (home, first, second) = (temp_dir(), temp_dir(), temp_dir());
    write_entry(&second.0, "fake.desktop", &entry("Exec=/x/second %F"));
    write_entry(&first.0, "fake.desktop", &entry("Exec=/x/first %F"));
    let env = env(&home.0, &[&first.0, &second.0]);
    assert_eq!(
        load("fake.desktop", &env).expect("loads").exec.program,
        "/x/first"
    );
}

#[test]
fn later_dir_is_used_when_earlier_ones_lack_the_id() {
    let (home, first, second) = (temp_dir(), temp_dir(), temp_dir());
    write_entry(&second.0, "fake.desktop", &entry("Exec=/x/second %F"));
    let env = env(&home.0, &[&first.0, &second.0]);
    assert_eq!(
        load("fake.desktop", &env).expect("loads").exec.program,
        "/x/second"
    );
}

#[test]
fn dash_id_is_found_in_a_subfolder() {
    let (home, env) = single();
    write_entry(&home.0, "kde/foo.desktop", &entry("Exec=/x/foo %F"));
    assert_eq!(
        locate("kde-foo.desktop", &env),
        Some(home.0.join("applications/kde/foo.desktop"))
    );
    assert_eq!(
        load("kde-foo.desktop", &env).expect("loads").exec.program,
        "/x/foo"
    );
}

#[test]
fn dash_id_finds_the_deeper_and_the_later_dash() {
    let (home, env) = single();
    write_entry(&home.0, "a/b/c-d.desktop", &entry("Exec=/x/deep %F"));
    write_entry(&home.0, "e-f/g.desktop", &entry("Exec=/x/late %F"));
    assert!(locate("a-b-c-d.desktop", &env).is_some());
    assert_eq!(
        locate("e-f-g.desktop", &env),
        Some(home.0.join("applications/e-f/g.desktop"))
    );
}

#[test]
fn plain_file_wins_over_the_dash_subfolder() {
    let (home, env) = single();
    write_entry(&home.0, "kde-foo.desktop", &entry("Exec=/x/plain %F"));
    write_entry(&home.0, "kde/foo.desktop", &entry("Exec=/x/sub %F"));
    assert_eq!(
        load("kde-foo.desktop", &env).expect("loads").exec.program,
        "/x/plain"
    );
}

#[test]
fn bad_ids_find_nothing() {
    let (home, env) = single();
    write_entry(&home.0, "fake.desktop", &entry("Exec=/x/fake %F"));
    std::fs::write(home.0.join("outside.desktop"), entry("Exec=/x/o %F")).expect("write");
    assert_eq!(locate("", &env), None);
    assert_eq!(locate("../outside.desktop", &env), None);
    assert_eq!(locate("sub/fake.desktop", &env), None);
    assert_eq!(locate("missing.desktop", &env), None);
}

#[test]
fn a_folder_is_not_an_entry() {
    let (home, env) = single();
    std::fs::create_dir_all(home.0.join("applications/dir.desktop")).expect("create dir");
    assert_eq!(locate("dir.desktop", &env), None);
}

#[test]
fn unusable_entries_give_none() {
    let (home, env) = single();
    let cases = [
        ("terminal.desktop", entry("Terminal=true\nExec=/x/t %F")),
        (
            "link.desktop",
            "[Desktop Entry]\nType=Link\nName=L\nExec=/x/l %F\n".to_owned(),
        ),
        (
            "notype.desktop",
            "[Desktop Entry]\nName=N\nExec=/x/n %F\n".to_owned(),
        ),
        ("noexec.desktop", entry("Comment=none")),
        ("nocode.desktop", entry("Exec=/x/nocode --flag")),
        ("hidden.desktop", entry("Hidden=true\nExec=/x/h %F")),
    ];
    for (name, text) in &cases {
        write_entry(&home.0, name, text);
        assert!(load(name, &env).is_none(), "{name} must give None");
    }
    assert!(load("absent.desktop", &env).is_none());
}

#[test]
fn false_flags_and_no_display_still_load() {
    let (home, env) = single();
    write_entry(
        &home.0,
        "ok.desktop",
        &entry("Terminal=false\nHidden=false\nNoDisplay=true\nExec=/x/ok %F"),
    );
    assert!(load("ok.desktop", &env).is_some());
}

#[test]
fn other_groups_are_ignored() {
    let (home, env) = single();
    // An action group with its own Exec does not give the entry an Exec.
    let only_action = "[Desktop Entry]\nType=Application\nName=A\n\n\
                       [Desktop Action x]\nExec=/x/action %F\n";
    write_entry(&home.0, "action.desktop", only_action);
    assert!(load("action.desktop", &env).is_none());
    // The group Exec wins whatever the order, and `Type` of another group
    // does not count.
    let both = "[Desktop Action x]\nExec=/x/action %F\nType=Link\n\n\
                [Desktop Entry]\nType=Application\nName=B\nExec=/x/main %F\n\n\
                [Desktop Action y]\nExec=/x/late %F\nTerminal=true\n";
    write_entry(&home.0, "both.desktop", both);
    assert_eq!(
        load("both.desktop", &env).expect("loads").exec.program,
        "/x/main"
    );
}

#[test]
fn comments_locale_keys_and_spaces_are_handled() {
    let (home, env) = single();
    let text = "# top comment\n[Desktop Entry]\r\n# Exec=/x/comment %F\r\n\
                Type = Application\r\nName=Fake\nName[de]=Falsch\n\
                Exec[de]=/x/de %F\nExec = /x/real %F\r\nExec=/x/second %F\n";
    write_entry(&home.0, "fake.desktop", text);
    let app = load("fake.desktop", &env).expect("loads");
    assert_eq!(app.exec.program, "/x/real");
}

#[test]
fn value_escapes_are_undone_before_the_exec_parse() {
    let (home, env) = single();
    write_entry(
        &home.0,
        "esc.desktop",
        &entry(r#"Exec=/x/a "b\sc\\\\d" %F"#),
    );
    let app = load("esc.desktop", &env).expect("loads");
    assert_eq!(app.exec.before, vec![r"b c\d".to_owned()]);
    assert_eq!(unescape(r"a\sb\nc\td\re\\f\qg\"), "a b\nc\td\re\\f\\qg\\");
}

#[test]
fn try_exec_must_name_a_runnable_program() {
    let home = temp_dir();
    let bin = temp_dir();
    let tool = bin.0.join("tool");
    std::fs::write(&tool, "").expect("write tool");
    let plain = bin.0.join("plain");
    std::fs::write(&plain, "").expect("write plain");
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let mut env = env(&home.0, &[]);
    env.path = Some(bin.0.clone().into_os_string());
    let cases = [
        ("tool", true),
        ("plain", false),
        ("missing", false),
        (tool.to_str().expect("utf-8"), true),
        (plain.to_str().expect("utf-8"), false),
        ("/nonexistent/tool", false),
    ];
    for (try_exec, loads) in cases {
        write_entry(
            &home.0,
            "try.desktop",
            &entry(&format!("TryExec={try_exec}\nExec=/x/try %F")),
        );
        assert_eq!(load("try.desktop", &env).is_some(), loads, "{try_exec}");
    }
}

#[test]
fn oversize_and_non_utf8_files_give_none() {
    let (home, env) = single();
    let mut big = entry("Exec=/x/big %F");
    big.push_str(&"#".repeat(usize::try_from(MAX_ENTRY_BYTES).expect("fits")));
    write_entry(&home.0, "big.desktop", &big);
    assert!(load("big.desktop", &env).is_none());
}

#[test]
fn a_valid_entry_with_a_non_utf8_byte_gives_none() {
    let (home, env) = single();
    let file = home.0.join("applications/bin.desktop");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("create dirs");
    // Without the bad byte this entry loads, so only the UTF-8 rule gives None.
    let text = b"[Desktop Entry]\nType=Application\nName=N\nExec=/x/\xff %F\n";
    std::fs::write(&file, text).expect("write");
    assert!(load("bin.desktop", &env).is_none());
    std::fs::write(
        &file,
        b"[Desktop Entry]\nType=Application\nName=N\nExec=/x/ok %F\n",
    )
    .expect("write");
    assert!(load("bin.desktop", &env).is_some());
}

#[test]
fn dot_dot_dash_ids_do_not_leave_the_applications_folder() {
    let (home, env) = single();
    // A valid entry one level above `applications/`.
    std::fs::write(home.0.join("x.desktop"), entry("Exec=/x/up %F")).expect("write");
    write_entry(&home.0, "other.desktop", &entry("Exec=/x/other %F"));
    assert_eq!(locate("..-x.desktop", &env), None);
    assert!(load("..-x.desktop", &env).is_none());
    assert_eq!(locate("..-..-etc-foo.desktop", &env), None);
    assert_eq!(locate("..", &env), None);
    assert_eq!(locate(".", &env), None);
    // A `.` part is also not a folder to step through.
    write_entry(&home.0, "a/b.desktop", &entry("Exec=/x/b %F"));
    assert_eq!(locate("a-.-b.desktop", &env), None);
}

#[test]
fn empty_parts_from_dashes_are_skipped() {
    let (home, env) = single();
    write_entry(&home.0, "x/foo.desktop", &entry("Exec=/x/foo %F"));
    // `x//foo.desktop` names the same file, but an empty part is not allowed.
    assert_eq!(locate("x--foo.desktop", &env), None);
    // A leading dash gives no absolute path and no match.
    assert_eq!(locate("-foo.desktop", &env), None);
    write_entry(&home.0, "-foo.desktop", &entry("Exec=/x/lead %F"));
    assert_eq!(
        locate("-foo.desktop", &env),
        Some(home.0.join("applications/-foo.desktop"))
    );
}

#[test]
fn only_the_first_eight_dashes_can_be_folder_splits() {
    let (home, env) = single();
    // The eighth dash is a split: found.
    write_entry(&home.0, "a-b-c-d-e-f-g-h/i.desktop", &entry("Exec=/x/i %F"));
    assert!(locate("a-b-c-d-e-f-g-h-i.desktop", &env).is_some());
    // Only the ninth dash is a split: not tried.
    write_entry(
        &home.0,
        "a-b-c-d-e-f-g-h-i/j.desktop",
        &entry("Exec=/x/j %F"),
    );
    assert_eq!(locate("a-b-c-d-e-f-g-h-i-j.desktop", &env), None);
}

#[test]
fn first_terminal_and_hidden_values_win() {
    let (home, env) = single();
    write_entry(
        &home.0,
        "first.desktop",
        &entry("Terminal=false\nTerminal=true\nHidden=false\nHidden=true\nExec=/x/f %F"),
    );
    assert!(load("first.desktop", &env).is_some());
    write_entry(
        &home.0,
        "later.desktop",
        &entry("Terminal=true\nTerminal=false\nExec=/x/f %F"),
    );
    assert!(load("later.desktop", &env).is_none());
    write_entry(
        &home.0,
        "hid.desktop",
        &entry("Hidden=true\nHidden=false\nExec=/x/f %F"),
    );
    assert!(load("hid.desktop", &env).is_none());
}
