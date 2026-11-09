use egui::{Color32, FontDefinitions, FontFamily, FontId, RawInput};

use super::*;

const ALL_FAMILIES: [(&str, &str); 4] = [
    ("Proportional", "roboto-regular"),
    ("Monospace", "roboto-regular"),
    (MEDIUM_FAMILY, "roboto-medium"),
    (BOLD_FAMILY, "roboto-bold"),
];

/// The result of a call that rejects no face.
const NONE: [&str; 0] = [];

fn family(label: &str) -> FontFamily {
    match label {
        "Proportional" => FontFamily::Proportional,
        "Monospace" => FontFamily::Monospace,
        name => FontFamily::Name(name.into()),
    }
}

fn face(name: &str) -> FallbackFont {
    FallbackFont {
        name: name.to_owned(),
        data: REGULAR.to_vec(),
        index: 0,
    }
}

fn frame(ctx: &egui::Context) {
    ctx.run_ui(RawInput::default(), |_| {})
        .drop_without_applying_deltas();
}

fn installed() -> egui::Context {
    let ctx = egui::Context::default();
    install(&ctx);
    frame(&ctx);
    ctx
}

fn current(ctx: &egui::Context) -> FontDefinitions {
    ctx.fonts(|fonts| fonts.definitions().clone())
}

fn latin_shape(ctx: &egui::Context) -> (usize, f32) {
    let galley = ctx.fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            "Tarragon".to_owned(),
            FontId::proportional(14.0),
            Color32::WHITE,
        )
    });
    let glyphs = galley.rows.iter().map(|row| row.row.glyphs.len()).sum();
    (glyphs, galley.rect.width())
}

#[test]
fn the_fallbacks_go_last_in_all_four_families_in_call_order() {
    let ctx = installed();

    let rejected = add_fallbacks(&ctx, vec![face("fb-one"), face("fb-two")]);
    frame(&ctx);

    assert_eq!(rejected, NONE);
    let fonts = current(&ctx);
    for (label, first) in ALL_FAMILIES {
        let list = &fonts.families[&family(label)];
        assert_eq!(list.first().map(String::as_str), Some(first), "{label}");
        assert_eq!(&list[list.len() - 2..], ["fb-one", "fb-two"], "{label}");
    }
}

#[test]
fn an_empty_list_leaves_the_definitions_unchanged() {
    let ctx = installed();
    let before = current(&ctx);

    assert_eq!(add_fallbacks(&ctx, Vec::new()), NONE);
    frame(&ctx);

    let after = current(&ctx);
    assert_eq!(after.families, before.families);
    assert_eq!(
        after.font_data.keys().collect::<HashSet<_>>(),
        before.font_data.keys().collect::<HashSet<_>>()
    );
}

#[test]
fn a_repeated_name_in_one_call_appears_once_in_each_family() {
    let ctx = installed();

    let rejected = add_fallbacks(&ctx, vec![face("fb-one"), face("fb-one")]);
    frame(&ctx);

    assert_eq!(rejected, NONE);
    let fonts = current(&ctx);
    for (label, _) in ALL_FAMILIES {
        let count = fonts.families[&family(label)]
            .iter()
            .filter(|name| *name == "fb-one")
            .count();
        assert_eq!(count, 1, "{label}");
    }
}

#[test]
fn a_latin_string_keeps_its_glyph_count_and_width() {
    let ctx = installed();
    let before = latin_shape(&ctx);

    // Bold has other advances than Regular, so a fallback in front of Roboto
    // would change the width.
    let bold = |name: &str| FallbackFont {
        data: BOLD.to_vec(),
        ..face(name)
    };
    let rejected = add_fallbacks(&ctx, vec![bold("fb-one"), bold("fb-two")]);
    frame(&ctx);

    assert_eq!(rejected, NONE);
    assert_eq!(latin_shape(&ctx), before);
}

/// Two copies of one face in a `.ttc` file, so index 1 is valid and 2 is not.
fn two_face_collection(ttf: &[u8]) -> Vec<u8> {
    let header = 12 + 4 * 2;
    let tables = usize::from(u16::from_be_bytes([ttf[4], ttf[5]]));
    let mut out = b"ttcf".to_vec();
    out.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    out.extend_from_slice(&2_u32.to_be_bytes());
    for copy in 0..2 {
        let start = header + copy * ttf.len();
        out.extend_from_slice(&u32::try_from(start).expect("small").to_be_bytes());
    }
    for copy in 0..2 {
        let shift = u32::try_from(header + copy * ttf.len()).expect("small");
        let mut face = ttf.to_vec();
        // Table offsets in a `.ttc` file count from the file start, so each copy shifts them.
        for table in 0..tables {
            let at = 12 + 16 * table + 8;
            let offset = u32::from_be_bytes(face[at..at + 4].try_into().expect("four bytes"));
            face[at..at + 4].copy_from_slice(&(offset + shift).to_be_bytes());
        }
        out.extend_from_slice(&face);
    }
    out
}

fn collection_face(index: u32) -> FallbackFont {
    FallbackFont {
        data: two_face_collection(REGULAR),
        index,
        ..face("fb-bad")
    }
}

fn reject_and_keep_the_good_face(bad: FallbackFont) {
    let ctx = installed();
    let before = current(&ctx);
    let bad_name = bad.name.clone();

    let rejected = add_fallbacks(&ctx, vec![bad, face("fb-good")]);
    frame(&ctx);

    assert_eq!(rejected, [bad_name.as_str()]);
    let after = current(&ctx);
    assert!(!after.font_data.contains_key(&bad_name));
    for (label, _) in ALL_FAMILIES {
        let list = &after.families[&family(label)];
        assert!(!list.contains(&bad_name), "{label}");
        assert_eq!(list.last().map(String::as_str), Some("fb-good"), "{label}");
        assert_eq!(list.len(), before.families[&family(label)].len() + 1);
    }
}

#[test]
fn junk_bytes_are_rejected_and_a_good_face_still_lands() {
    let junk = FallbackFont {
        data: b"not a font".to_vec(),
        ..face("fb-bad")
    };

    reject_and_keep_the_good_face(junk);
}

#[test]
fn a_truncated_face_is_rejected_and_a_good_face_still_lands() {
    let truncated = FallbackFont {
        data: REGULAR[..8].to_vec(),
        ..face("fb-bad")
    };

    reject_and_keep_the_good_face(truncated);
}

#[test]
fn an_index_past_the_face_count_is_rejected_and_a_good_face_still_lands() {
    let past_the_end = FallbackFont {
        index: 3,
        ..face("fb-bad")
    };

    reject_and_keep_the_good_face(past_the_end);
    reject_and_keep_the_good_face(collection_face(2));
}

#[test]
fn the_face_index_reaches_the_font_data() {
    let ctx = installed();

    let rejected = add_fallbacks(&ctx, vec![collection_face(1)]);
    frame(&ctx);

    assert_eq!(rejected, NONE);
    assert_eq!(current(&ctx).font_data["fb-bad"].index, 1);
}
