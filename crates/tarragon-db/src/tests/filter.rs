use std::collections::BTreeSet;
use std::path::PathBuf;

use tarragon_core::color::ColorBucket;
use tarragon_core::entity::{Tag, TagSource};
use tarragon_core::filter_state::{FilterScope, FilterState};

use super::support::{TempDb, file};

/// A filter state with one folder chip.
fn scoped(folder: &str) -> FilterState {
    FilterState {
        folders: BTreeSet::from([PathBuf::from(folder)]),
        ..FilterState::default()
    }
}

/// Attach tags to one path.
fn attach_tags(temp: &TempDb, path: &str, tags: &[&Tag]) {
    let owned: Vec<Tag> = tags.iter().map(|tag| (*tag).clone()).collect();
    temp.db
        .add_tags_to_files_batch(&[path.to_owned()], &owned)
        .expect("attach the tags");
}

/// Populate the filter fixture: two folders, user tags, and color tags.
///
/// The rows are a fixed fixture, so the behavior tests compare equal results.
fn populate_filter_fixture(temp: &TempDb) {
    let files = [
        file("/test/photos", "sunset_beach.png", 1, 100),
        file("/test/photos", "forest_path.jpg", 2, 200),
        file("/test/photos", "blue_ocean.jpg", 3, 300),
        file("/test/photos", "green_valley.png", 4, 150),
        file("/test/photos", "mountain_top.jpg", 5, 250),
        file("/test/photos", "doc.txt", 6, 10),
        file("/test/other", "beach.png", 7, 120),
    ];
    temp.db.write_scan_group(&files).expect("write the fixture");

    let beach = temp.db.ensure_tag("beach", TagSource::User).expect("tag");
    let vacation = temp
        .db
        .ensure_tag("vacation", TagSource::User)
        .expect("tag");
    let nature = temp.db.ensure_tag("nature", TagSource::User).expect("tag");
    let hiking = temp.db.ensure_tag("hiking", TagSource::User).expect("tag");
    let warm = temp
        .db
        .ensure_tag("warm", TagSource::AutoColor)
        .expect("tag");
    let green = temp
        .db
        .ensure_tag("green", TagSource::AutoColor)
        .expect("tag");
    let blue = temp
        .db
        .ensure_tag("blue", TagSource::AutoColor)
        .expect("tag");

    attach_tags(
        temp,
        "/test/photos/sunset_beach.png",
        &[&beach, &vacation, &warm],
    );
    attach_tags(
        temp,
        "/test/photos/forest_path.jpg",
        &[&nature, &hiking, &green],
    );
    attach_tags(
        temp,
        "/test/photos/blue_ocean.jpg",
        &[&beach, &vacation, &blue],
    );
    attach_tags(temp, "/test/photos/green_valley.png", &[&nature, &green]);
    attach_tags(temp, "/test/photos/mountain_top.jpg", &[&hiking, &blue]);
    attach_tags(temp, "/test/other/beach.png", &[&beach, &warm]);
}

/// Run the filter engine and return the stored paths.
fn filter_paths(temp: &TempDb, filters: &FilterState) -> Vec<String> {
    temp.db
        .filter_thumbnails(filters)
        .expect("run the filter query")
        .into_iter()
        .map(|row| row.path)
        .collect()
}

#[test]
fn filter_engine_folder_scope_returns_every_file() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    assert_eq!(filter_paths(&temp, &scoped("/test/photos/")).len(), 6);
}

#[test]
fn filter_engine_global_scope_returns_every_file() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    assert_eq!(filter_paths(&temp, &FilterState::default()).len(), 7);
}

#[test]
fn filter_engine_returns_natural_full_path_order() {
    let temp = TempDb::new();
    for index in [1, 10, 2] {
        temp.db
            .write_scan_group(&[file(
                "/test/natural",
                &format!("photo {index}.png"),
                index,
                10,
            )])
            .expect("write the row");
    }
    assert_eq!(
        filter_paths(&temp, &scoped("/test/natural")),
        [
            "/test/natural/photo 1.png",
            "/test/natural/photo 2.png",
            "/test/natural/photo 10.png"
        ]
    );
}

#[test]
fn filter_engine_filename_filter_matches_a_substring() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        text: "beach".to_owned(),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        ["/test/photos/sunset_beach.png"]
    );
}

#[test]
fn filter_engine_filename_filter_ignores_ascii_case() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        text: "BEACH".to_owned(),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        ["/test/photos/sunset_beach.png"]
    );
}

#[test]
fn filter_engine_filename_filter_escapes_like_wildcards() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    for text in ["%_", "do_"] {
        let filters = FilterState {
            text: text.to_owned(),
            ..scoped("/test/photos")
        };
        assert_eq!(
            filter_paths(&temp, &filters),
            Vec::<String>::new(),
            "the text {text} matched as a LIKE wildcard"
        );
    }
}

#[test]
fn filter_engine_filename_filter_escapes_a_literal_backslash() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    // Without the escape do\c would match doc.txt.
    let filters = FilterState {
        text: "do\\c".to_owned(),
        ..scoped("/test/photos")
    };
    assert_eq!(filter_paths(&temp, &filters), Vec::<String>::new());
}

#[test]
fn filter_engine_filename_filter_does_not_trim_space() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    // The leading space stays, so this must not match sunset_beach.png.
    let filters = FilterState {
        text: " beach".to_owned(),
        ..scoped("/test/photos")
    };
    assert_eq!(filter_paths(&temp, &filters), Vec::<String>::new());
}

#[test]
fn filter_engine_whitespace_only_text_is_one_active_filter() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/ws", "with space.png", 1, 10),
            file("/ws", "plain.png", 2, 10),
        ])
        .expect("write the rows");
    let filters = FilterState {
        text: " ".to_owned(),
        ..FilterState::default()
    };
    assert_eq!(filters.active_count(FilterScope::Global), 1);
    assert_eq!(filter_paths(&temp, &filters), ["/ws/with space.png"]);
}

#[test]
fn filter_engine_tags_require_all_selected_tags() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        tags: BTreeSet::from(["beach".to_owned(), "vacation".to_owned()]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        [
            "/test/photos/blue_ocean.jpg",
            "/test/photos/sunset_beach.png"
        ]
    );
    // nature OR hiking also returns green_valley and mountain_top, so this
    // pair proves the AND.
    let separate = FilterState {
        tags: BTreeSet::from(["nature".to_owned(), "hiking".to_owned()]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &separate),
        ["/test/photos/forest_path.jpg"]
    );
}

#[test]
fn filter_engine_tags_with_no_common_file_return_empty() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        tags: BTreeSet::from(["beach".to_owned(), "nature".to_owned()]),
        ..scoped("/test/photos")
    };
    assert_eq!(filter_paths(&temp, &filters), Vec::<String>::new());
}

#[test]
fn filter_engine_color_filter_matches_one_bucket() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        colors: BTreeSet::from([ColorBucket::Green]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        [
            "/test/photos/forest_path.jpg",
            "/test/photos/green_valley.png"
        ]
    );
}

#[test]
fn filter_engine_colors_require_all_selected_buckets() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        colors: BTreeSet::from([ColorBucket::Green, ColorBucket::Blue]),
        ..scoped("/test/photos")
    };
    assert_eq!(filter_paths(&temp, &filters), Vec::<String>::new());
}

#[test]
fn filter_engine_tag_and_color_filters_combine_with_and() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    // Every nature row also has green, so OR and AND agree here.
    let filters = FilterState {
        tags: BTreeSet::from(["nature".to_owned()]),
        colors: BTreeSet::from([ColorBucket::Green]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &filters),
        [
            "/test/photos/forest_path.jpg",
            "/test/photos/green_valley.png"
        ]
    );
    // hiking OR green also returns mountain_top and green_valley, so this
    // pair proves the AND.
    let separate = FilterState {
        tags: BTreeSet::from(["hiking".to_owned()]),
        colors: BTreeSet::from([ColorBucket::Green]),
        ..scoped("/test/photos")
    };
    assert_eq!(
        filter_paths(&temp, &separate),
        ["/test/photos/forest_path.jpg"]
    );
}

#[test]
fn filter_engine_folders_combine_with_or() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    let filters = FilterState {
        folders: BTreeSet::from([PathBuf::from("/test/photos"), PathBuf::from("/test/other")]),
        ..FilterState::default()
    };
    assert_eq!(filter_paths(&temp, &filters).len(), 7);
}

#[test]
fn filter_engine_unknown_folder_returns_empty() {
    let temp = TempDb::new();
    populate_filter_fixture(&temp);
    assert_eq!(
        filter_paths(&temp, &scoped("/nonexistent")),
        Vec::<String>::new()
    );
}

#[test]
fn filter_engine_folder_does_not_match_a_sibling_prefix() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/photos", "img.png", 1, 10),
            file("/photos-vacation", "img.png", 2, 20),
        ])
        .expect("write the rows");
    assert_eq!(filter_paths(&temp, &scoped("/photos")), ["/photos/img.png"]);
}

#[test]
fn filter_engine_folder_escapes_an_underscore() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/my_photos", "a.png", 1, 10),
            file("/myXphotos", "b.png", 2, 10),
        ])
        .expect("write the rows");
    assert_eq!(
        filter_paths(&temp, &scoped("/my_photos")),
        ["/my_photos/a.png"]
    );
}

#[test]
fn filter_engine_folder_escapes_a_percent() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("/my%photos", "a.png", 1, 10),
            file("/myXXphotos", "b.png", 2, 10),
        ])
        .expect("write the rows");
    assert_eq!(
        filter_paths(&temp, &scoped("/my%photos")),
        ["/my%photos/a.png"]
    );
}

#[test]
fn filter_engine_folder_normalizes_backslash_separators() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[
            file("D:/Dropbox/Art/Speedpaintings", "img1.jpg", 1, 10),
            file("D:/Dropbox/Art/Other", "img2.jpg", 2, 10),
        ])
        .expect("write the rows");
    let filters = scoped("D:\\Dropbox\\Art\\Speedpaintings");
    assert_eq!(
        filter_paths(&temp, &filters),
        ["D:/Dropbox/Art/Speedpaintings/img1.jpg"]
    );
}

#[test]
fn filter_engine_has_ten_color_buckets() {
    assert_eq!(ColorBucket::ALL.len(), 10);
    let temp = TempDb::new();
    for bucket in ColorBucket::ALL {
        let name = bucket.as_str();
        temp.db
            .write_scan_group(&[file("/buckets", &format!("{name}.png"), 1, 10)])
            .expect("write the row");
        let tag = temp
            .db
            .ensure_tag(name, TagSource::AutoColor)
            .expect("make the bucket tag");
        temp.db
            .add_tags_to_files_batch(&[format!("/buckets/{name}.png")], &[tag])
            .expect("attach the bucket tag");
    }
    for bucket in ColorBucket::ALL {
        let filters = FilterState {
            colors: BTreeSet::from([bucket]),
            ..FilterState::default()
        };
        assert_eq!(
            filter_paths(&temp, &filters),
            [format!("/buckets/{}.png", bucket.as_str())]
        );
    }
}
