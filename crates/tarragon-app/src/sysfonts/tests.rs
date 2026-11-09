use std::path::PathBuf;
use std::sync::Arc;

use fontdb::{Database, FaceInfo, ID, Language, Source, Stretch, Style, Weight};

use super::{MAX_CJK, MAX_SYMBOLS, pick};

const ROBOTO: &[u8] = include_bytes!("../../../tarragon-ui/assets/fonts/Roboto-Regular.ttf");

/// A face with a chosen family name, so a test controls which names match.
fn face(family: &str, source: Source) -> FaceInfo {
    FaceInfo {
        id: ID::dummy(),
        source,
        index: 0,
        families: vec![(family.to_owned(), Language::English_UnitedStates)],
        post_script_name: family.to_owned(),
        style: Style::Normal,
        weight: Weight::NORMAL,
        stretch: Stretch::Normal,
        monospaced: false,
    }
}

/// A database with one Roboto face for each family name.
fn database(families: &[&str]) -> Database {
    let mut db = Database::new();
    for family in families {
        let data: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(ROBOTO.to_vec());
        db.push_face_info(face(family, Source::Binary(data)));
    }
    db
}

/// The font folder of the interface crate. It holds three real files.
const FONT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tarragon-ui/assets/fonts");

/// A face that reads from one of the real Roboto files.
fn file_face(family: &str, file: &str, index: u32) -> FaceInfo {
    let mut info = face(family, Source::File(PathBuf::from(FONT_DIR).join(file)));
    info.index = index;
    info
}

#[test]
fn the_caps_are_three_cjk_and_two_symbol_faces() {
    assert_eq!(MAX_CJK, 3);
    assert_eq!(MAX_SYMBOLS, 2);
}

#[test]
fn a_loaded_font_file_is_found_with_its_bytes_and_index() {
    let mut db = Database::new();
    db.load_font_data(ROBOTO.to_vec());

    let found = pick(&db, &["Roboto"], &[]);

    assert_eq!(found.fonts.len(), 1);
    assert_eq!(found.fonts[0].data, ROBOTO);
    assert_eq!(found.fonts[0].index, 0);
    assert_eq!(found.fonts[0].name, "fallback-0-Roboto");
    assert!(found.cjk);
}

#[test]
fn the_face_number_inside_a_collection_is_kept() {
    let mut db = Database::new();
    let data: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(ROBOTO.to_vec());
    let mut info = face("Collected", Source::Binary(data));
    info.index = 2;
    db.push_face_info(info);

    let found = pick(&db, &["Collected"], &[]);

    assert_eq!(found.fonts.len(), 1);
    assert_eq!(found.fonts[0].index, 2);
}

#[test]
fn a_missing_name_is_skipped_and_a_repeated_face_is_kept_once() {
    let mut db = Database::new();
    db.load_font_data(ROBOTO.to_vec());

    let found = pick(&db, &["Missing", "Roboto", "Roboto"], &[]);

    assert_eq!(found.fonts.len(), 1);
}

#[test]
fn an_empty_database_gives_no_fonts_and_no_cjk() {
    let found = pick(&Database::new(), &["Roboto"], &["Roboto"]);

    assert_eq!(found.fonts.len(), 0);
    assert!(!found.cjk);
}

#[test]
fn the_family_match_is_case_sensitive() {
    let mut db = Database::new();
    db.load_font_data(ROBOTO.to_vec());

    let found = pick(&db, &["roboto"], &[]);

    assert_eq!(found.fonts.len(), 0);
}

#[test]
fn the_cjk_list_stops_at_three_faces() {
    let db = database(&["A", "B", "C", "D"]);

    let found = pick(&db, &["A", "B", "C", "D"], &[]);

    let names: Vec<_> = found.fonts.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["fallback-0-A", "fallback-1-B", "fallback-2-C"]);
}

#[test]
fn the_symbol_list_stops_at_two_faces() {
    let db = database(&["S1", "S2", "S3"]);

    let found = pick(&db, &[], &["S1", "S2", "S3"]);

    assert_eq!(found.fonts.len(), 2);
    assert!(!found.cjk);
}

#[test]
fn cjk_faces_come_before_symbol_faces_in_list_order() {
    let db = database(&["S1", "C1", "C2"]);

    let found = pick(&db, &["C2", "C1"], &["S1"]);

    let names: Vec<_> = found.fonts.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["fallback-0-C2", "fallback-1-C1", "fallback-2-S1"]);
    assert!(found.cjk);
}

#[test]
fn a_symbol_face_alone_does_not_set_the_cjk_flag() {
    let db = database(&["S1"]);

    let found = pick(&db, &["Missing"], &["S1"]);

    assert_eq!(found.fonts.len(), 1);
    assert!(!found.cjk);
}

#[test]
fn an_unreadable_file_is_skipped_and_the_next_name_still_counts() {
    let mut db = database(&["Good"]);
    let gone = Source::File(PathBuf::from("/nonexistent-dir/gone.ttf"));
    db.push_face_info(face("Ghost", gone));

    let found = pick(&db, &["Ghost", "Good"], &[]);

    assert_eq!(found.fonts.len(), 1);
    assert_eq!(found.fonts[0].name, "fallback-0-Good");
}

#[test]
fn two_faces_of_one_file_give_one_font() {
    let mut db = Database::new();
    db.push_face_info(file_face("A", "Roboto-Regular.ttf", 0));
    db.push_face_info(file_face("B", "Roboto-Regular.ttf", 1));

    let found = pick(&db, &["A", "B"], &[]);

    assert_eq!(found.fonts.len(), 1);
    assert_eq!(found.fonts[0].name, "fallback-0-A");
}

#[test]
fn two_files_give_two_fonts() {
    let mut db = Database::new();
    db.push_face_info(file_face("A", "Roboto-Regular.ttf", 0));
    db.push_face_info(file_face("B", "Roboto-Medium.ttf", 0));

    let found = pick(&db, &["A", "B"], &[]);

    assert_eq!(found.fonts.len(), 2);
}

#[test]
fn a_face_skipped_for_its_file_uses_no_cap_slot() {
    let mut db = Database::new();
    db.push_face_info(file_face("A", "Roboto-Regular.ttf", 0));
    db.push_face_info(file_face("B", "Roboto-Regular.ttf", 1));
    db.push_face_info(file_face("C", "Roboto-Medium.ttf", 0));
    db.push_face_info(file_face("D", "Roboto-Bold.ttf", 0));

    let found = pick(&db, &["A", "B", "C", "D"], &[]);

    let names: Vec<_> = found.fonts.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["fallback-0-A", "fallback-1-C", "fallback-2-D"]);
}

#[test]
fn a_file_used_by_a_cjk_face_gives_no_symbol_face() {
    let mut db = Database::new();
    db.push_face_info(file_face("Cjk", "Roboto-Regular.ttf", 0));
    db.push_face_info(file_face("Sym", "Roboto-Regular.ttf", 1));

    let found = pick(&db, &["Cjk"], &["Sym"]);

    assert_eq!(found.fonts.len(), 1);
}

#[test]
fn one_name_in_both_lists_gives_one_font() {
    let db = database(&["Both"]);

    let found = pick(&db, &["Both"], &["Both"]);

    assert_eq!(found.fonts.len(), 1);
}

#[test]
fn the_regular_weight_wins_over_bold_in_one_family() {
    let mut db = Database::new();
    let mut bold = face("Fam", Source::Binary(Arc::new(ROBOTO.to_vec())));
    bold.weight = Weight::BOLD;
    bold.index = 7;
    db.push_face_info(bold);
    let mut regular = face("Fam", Source::Binary(Arc::new(ROBOTO.to_vec())));
    regular.index = 3;
    db.push_face_info(regular);

    let found = pick(&db, &["Fam"], &[]);

    assert_eq!(found.fonts.len(), 1);
    assert_eq!(found.fonts[0].index, 3);
}

#[test]
fn the_normal_style_wins_over_italic_in_one_family() {
    let mut db = Database::new();
    let mut italic = face("Fam", Source::Binary(Arc::new(ROBOTO.to_vec())));
    italic.style = Style::Italic;
    italic.index = 5;
    db.push_face_info(italic);
    let mut upright = face("Fam", Source::Binary(Arc::new(ROBOTO.to_vec())));
    upright.index = 1;
    db.push_face_info(upright);

    let found = pick(&db, &["Fam"], &[]);

    assert_eq!(found.fonts.len(), 1);
    assert_eq!(found.fonts[0].index, 1);
}

#[cfg(target_os = "linux")]
#[test]
fn the_linux_symbol_list_finds_both_spellings_of_symbols_two() {
    for family in ["Noto Sans Symbols 2", "Noto Sans Symbols2"] {
        let db = database(&[family]);

        let found = pick(&db, &[], super::SYMBOLS);

        assert_eq!(found.fonts.len(), 1, "no face found for {family}");
    }
}
