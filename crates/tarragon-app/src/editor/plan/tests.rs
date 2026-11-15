use std::ffi::OsString;

use super::*;

/// A route on a list key; `kind` picks one file or many per start.
fn list(key: &str, kind: ListKind) -> Route {
    Route::List {
        key: key.to_owned(),
        line: ExecLine {
            program: key.to_owned(),
            before: Vec::new(),
            after: Vec::new(),
            list: kind,
            uri: false,
        },
    }
}

/// A one-file route that runs `program` with no arguments.
fn single(program: &str) -> Route {
    Route::Single(Launch {
        program: OsString::from(program),
        args: Vec::new(),
    })
}

/// A path from text, to keep the test lines short.
fn p(name: &str) -> PathBuf {
    PathBuf::from(name)
}

/// The files of each start, in start order.
fn paths(starts: &[Planned]) -> Vec<Vec<PathBuf>> {
    starts.iter().map(|start| start.paths.clone()).collect()
}

#[test]
fn two_keys_give_two_starts_with_selection_order() {
    let starts = plan(vec![
        (p("/a/1.png"), list("gimp", ListKind::Many)),
        (p("/a/x.psd"), list("krita", ListKind::Many)),
        (p("/a/2.png"), list("gimp", ListKind::Many)),
    ]);
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0].paths, vec![p("/a/1.png"), p("/a/2.png")]);
    assert_eq!(starts[0].launch.program, OsString::from("gimp"));
    assert_eq!(
        starts[0].launch.args,
        vec![OsString::from("/a/1.png"), OsString::from("/a/2.png")]
    );
    assert_eq!(starts[1].paths, vec![p("/a/x.psd")]);
}

#[test]
fn interleaved_keys_keep_first_seen_order() {
    let starts = plan(vec![
        (p("/1"), list("a", ListKind::Many)),
        (p("/2"), list("b", ListKind::Many)),
        (p("/3"), list("a", ListKind::Many)),
    ]);
    assert_eq!(paths(&starts), vec![vec![p("/1"), p("/3")], vec![p("/2")]]);
}

#[test]
fn a_repeated_path_is_planned_once() {
    let starts = plan(vec![
        (p("/1"), list("a", ListKind::Many)),
        (p("/1"), list("a", ListKind::Many)),
        (p("/2"), single("x")),
        (p("/2"), single("x")),
    ]);
    assert_eq!(paths(&starts), vec![vec![p("/1")], vec![p("/2")]]);
}

#[test]
fn many_short_paths_split_by_count() {
    let routes = (0..1200)
        .map(|n| (p(&format!("/f{n}")), list("a", ListKind::Many)))
        .collect();
    let starts = plan(routes);
    let sizes: Vec<usize> = starts.iter().map(|start| start.paths.len()).collect();
    assert_eq!(sizes, vec![500, 500, 200]);
    assert_eq!(starts[1].paths[0], p("/f500"));
}

#[test]
fn long_paths_split_by_bytes() {
    let routes = (0..10)
        .map(|n| {
            let name = format!("/{n}{}", "x".repeat(19_998));
            (p(&name), list("a", ListKind::Many))
        })
        .collect();
    let starts = plan(routes);
    assert_eq!(starts.len(), 2);
    assert!(starts.iter().all(|start| start.paths.len() <= 5));
    assert_eq!(
        starts.iter().map(|start| start.paths.len()).sum::<usize>(),
        10
    );
}

#[test]
fn a_path_over_the_byte_cap_is_not_split() {
    let long = format!("/{}", "x".repeat(MAX_PATH_BYTES + 5));
    let starts = plan(vec![
        (p(&long), list("a", ListKind::Many)),
        (p("/b"), list("a", ListKind::Many)),
    ]);
    assert_eq!(paths(&starts), vec![vec![p(&long)], vec![p("/b")]]);
}

#[test]
fn one_file_line_gives_a_start_per_path() {
    let starts = plan(vec![
        (p("/1"), list("a", ListKind::One)),
        (p("/2"), list("a", ListKind::One)),
        (p("/3"), list("a", ListKind::One)),
    ]);
    assert_eq!(
        paths(&starts),
        vec![vec![p("/1")], vec![p("/2")], vec![p("/3")]]
    );
}

#[test]
fn single_routes_stay_in_place() {
    let starts = plan(vec![
        (p("/1"), single("x")),
        (p("/2"), list("a", ListKind::Many)),
        (p("/3"), single("y")),
        (p("/4"), list("a", ListKind::Many)),
    ]);
    assert_eq!(
        paths(&starts),
        vec![vec![p("/1")], vec![p("/2"), p("/4")], vec![p("/3")]]
    );
    assert_eq!(starts[0].launch.program, OsString::from("x"));
    assert_eq!(starts[2].launch.program, OsString::from("y"));
}
