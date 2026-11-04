use super::{fnv1a64, portable, sanitize};

#[test]
fn fnv1a64_matches_known_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn portable_suffix_is_the_low_32_bits_of_the_hash() {
    assert_eq!(portable("a:b.png"), "a_b.png-197f1765");
}

#[test]
fn each_illegal_character_becomes_underscore() {
    for c in ['<', '>', ':', '"', '/', '\\', '|', '?', '*'] {
        assert_eq!(sanitize(&format!("a{c}b")), "a_b", "char {c:?}");
    }
}

#[test]
fn control_characters_become_underscore() {
    assert_eq!(sanitize("a\u{1}b"), "a_b");
    assert_eq!(sanitize("a\u{1f}b"), "a_b");
    assert_eq!(sanitize("a\0b"), "a_b");
    // 0x20 is a plain space and stays inside a name.
    assert_eq!(sanitize("a b"), "a b");
}

#[test]
fn trailing_dots_and_spaces_are_removed() {
    assert_eq!(sanitize("a. "), "a");
    assert_eq!(sanitize("a .. "), "a");
    assert_eq!(sanitize("a.b"), "a.b");
    assert_eq!(sanitize(".hidden"), ".hidden");
}

#[test]
fn empty_result_becomes_underscore() {
    assert_eq!(sanitize("..."), "_");
    assert_eq!(sanitize(" . "), "_");
    assert_eq!(sanitize(""), "_");
}

#[test]
fn reserved_device_names_get_a_prefix() {
    for name in ["con", "CON.txt", "Lpt9.png", "com1", "nUl", "aux.tar.gz"] {
        assert_eq!(sanitize(name), format!("_{name}"), "name {name:?}");
    }
}

#[test]
fn reserved_check_runs_after_trailing_removal() {
    assert_eq!(sanitize("con. "), "_con");
}

#[test]
fn non_reserved_look_alikes_stay_as_they_are() {
    for name in ["console", "com10", "a.con", "com0", "lpt", "com"] {
        assert_eq!(sanitize(name), name, "name {name:?}");
    }
}

#[test]
fn unchanged_names_stay_byte_identical() {
    for name in [
        "art.png",
        "photo.png.jpg",
        "a b.png",
        "日本語.png",
        "é-ü_ß.jpg",
    ] {
        assert_eq!(portable(name), name);
    }
}

#[test]
fn changed_names_get_a_hash_suffix() {
    let name = portable("a:b.png");
    assert!(name.starts_with("a_b.png-"));
    assert_eq!(name.len(), "a_b.png-".len() + 8);
    assert_ne!(portable("con"), "_con");
    assert!(portable("con").starts_with("_con-"));
    assert!(portable("...").starts_with("_-"));
}

#[test]
fn names_that_collapse_stay_distinct() {
    let colon = portable("a:b.png");
    let star = portable("a*b.png");
    let plain = portable("a_b.png");
    assert_ne!(colon, plain);
    assert_ne!(colon, star);
    assert_ne!(star, plain);
    assert_ne!(portable("con"), portable("_con"));
}

#[test]
fn multi_byte_text_is_not_split() {
    assert_eq!(sanitize("日本:語.png"), "日本_語.png");
    assert_eq!(sanitize("é\u{1}ü. "), "é_ü");
    assert_eq!(sanitize("😀*😀"), "😀_😀");
    let name = portable("日本:語.png");
    assert!(name.starts_with("日本_語.png-"));
}
