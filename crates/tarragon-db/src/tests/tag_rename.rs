use tarragon_core::entity::{Tag, TagSource};
use tarragon_core::tag_name::TagNameError;

use crate::DbError;

use super::support::TempDb;

/// Make a user tag and attach it to `count` files.
fn used_tag(temp: &TempDb, name: &str, count: usize) -> Tag {
    let tag = temp.db.ensure_tag(name, TagSource::User).expect("tag");
    let paths: Vec<String> = (0..count).map(|i| format!("/refs/{i}.png")).collect();
    temp.db
        .add_tags_to_files_batch(&paths, std::slice::from_ref(&tag))
        .expect("attach");
    tag
}

/// List `(name, usage count)` of every tag.
fn usage(temp: &TempDb) -> Vec<(String, u32)> {
    let all = temp.db.all_tags().expect("all tags");
    all.into_iter()
        .map(|u| (u.tag.name, u.usage_count))
        .collect()
}

#[test]
fn rename_keeps_the_id_and_every_file_link() {
    let temp = TempDb::new();
    let tag = used_tag(&temp, "a", 2);
    let stored = temp.db.rename_tag(tag.id, "b").expect("rename");
    assert_eq!(stored, "b");
    let all = temp.db.all_tags().expect("all tags");
    assert_eq!(all.len(), 1);
    assert_eq!((all[0].tag.id, all[0].tag.name.as_str()), (tag.id, "b"));
    assert_eq!(all[0].usage_count, 2);
    let paths = vec!["/refs/0.png".to_owned(), "/refs/1.png".to_owned()];
    let by_file = temp.db.tags_for_files(&paths).expect("tags");
    for path in &paths {
        assert_eq!(by_file[path].len(), 1);
        assert_eq!(by_file[path][0].name, "b");
    }
}

#[test]
fn rename_onto_a_used_name_is_refused_and_changes_nothing() {
    let temp = TempDb::new();
    let tag = used_tag(&temp, "a", 1);
    temp.db.ensure_tag("b", TagSource::User).expect("user tag");
    temp.db
        .ensure_tag("glow", TagSource::AutoColor)
        .expect("auto tag");
    let before = usage(&temp);
    for name in ["b", "glow"] {
        let err = temp.db.rename_tag(tag.id, name).expect_err("taken");
        assert!(
            matches!(&err, DbError::TagNameTaken(got) if got == name),
            "{err:?}"
        );
    }
    assert_eq!(usage(&temp), before);
}

#[test]
fn rename_compares_names_with_case() {
    let temp = TempDb::new();
    let tag = temp.db.ensure_tag("Cat", TagSource::User).expect("tag");
    assert_eq!(temp.db.rename_tag(tag.id, "cat").expect("free"), "cat");
    let other = temp.db.ensure_tag("Dog", TagSource::User).expect("tag");
    assert_eq!(
        temp.db.rename_tag(other.id, "Cat").expect("free now"),
        "Cat"
    );
}

#[test]
fn rename_refuses_reserved_and_invalid_names() {
    let temp = TempDb::new();
    let tag = used_tag(&temp, "a", 1);
    for name in ["Red", "transparent"] {
        let err = temp.db.rename_tag(tag.id, name).expect_err("reserved");
        assert!(
            matches!(&err, DbError::ReservedTagName(got) if got == name),
            "{err:?}"
        );
    }
    let long = "a".repeat(65);
    for (name, want) in [
        ("", TagNameError::Empty),
        (long.as_str(), TagNameError::TooLong),
    ] {
        let err = temp.db.rename_tag(tag.id, name).expect_err("invalid");
        assert!(matches!(err, DbError::InvalidTagName(got) if got == want));
    }
    assert_eq!(usage(&temp), vec![("a".to_owned(), 1)]);
}

#[test]
fn rename_refuses_built_in_tags_and_unknown_ids() {
    let temp = TempDb::new();
    let blue = temp
        .db
        .ensure_tag("blue", TagSource::AutoColor)
        .expect("tag");
    let clear = temp
        .db
        .ensure_tag("transparent", TagSource::AutoColor)
        .expect("tag");
    for (tag, name) in [(blue, "blue"), (clear, "transparent")] {
        let err = temp.db.rename_tag(tag.id, "mine").expect_err("protected");
        assert!(
            matches!(&err, DbError::ProtectedTag(got) if got == name),
            "{err:?}"
        );
    }
    let err = temp.db.rename_tag(999, "mine").expect_err("unknown");
    assert!(matches!(err, DbError::TagNotFound(999)));
    assert_eq!(tag_names(&temp), ["blue", "transparent"]);
}

/// List every tag name in order.
fn tag_names(temp: &TempDb) -> Vec<String> {
    usage(temp).into_iter().map(|(name, _)| name).collect()
}

#[test]
fn rename_stores_the_trimmed_name_and_accepts_the_current_name() {
    let temp = TempDb::new();
    let tag = used_tag(&temp, "a", 1);
    assert_eq!(temp.db.rename_tag(tag.id, "  b  ").expect("rename"), "b");
    assert_eq!(tag_names(&temp), ["b"]);
    assert_eq!(temp.db.rename_tag(tag.id, " b ").expect("same name"), "b");
    assert_eq!(usage(&temp), vec![("b".to_owned(), 1)]);
}
