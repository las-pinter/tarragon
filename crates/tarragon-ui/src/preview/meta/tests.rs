//! Tests for the metadata text.

use super::*;

#[test]
fn size_text_has_exact_output_at_the_unit_boundaries() {
    let cases: [(u64, &str); 14] = [
        (0, "0 B"),
        (1, "1 B"),
        (1023, "1023 B"),
        (1024, "1.0 KB"),
        (1280, "1.3 KB"),
        (1536, "1.5 KB"),
        (1_048_575, "1.0 MB"),
        (1_048_576, "1.0 MB"),
        (2_516_582, "2.4 MB"),
        (1_073_741_823, "1.0 GB"),
        (1_073_741_824, "1.0 GB"),
        (5_368_709_120, "5.0 GB"),
        (1_099_511_627_776, "1024.0 GB"),
        (u64::MAX, "17179869184.0 GB"),
    ];
    for (bytes, want) in cases {
        assert_eq!(size_text(bytes), want, "{bytes} bytes");
    }
}

#[test]
fn dimensions_text_joins_width_and_height() {
    assert_eq!(dimensions_text((2048, 1536)), "2048 x 1536");
    assert_eq!(dimensions_text((0, 0)), "0 x 0");
}

#[test]
fn rows_give_the_four_mockup_fields_and_unknown_for_missing_values() {
    let meta = Meta {
        name: "sketch_014.png".into(),
        dimensions: Some((2048, 1536)),
        size: Some(2_516_582),
        format: "PNG".into(),
    };
    let want = [
        ("File", "sketch_014.png"),
        ("Dimensions", "2048 x 1536"),
        ("Size", "2.4 MB"),
        ("Format", "PNG"),
    ];
    assert_eq!(meta.rows(), want.map(|(k, v)| (k, v.to_owned())));

    let bare = Meta {
        dimensions: None,
        size: None,
        ..meta
    };
    let want = [
        ("File", "sketch_014.png"),
        ("Dimensions", "Unknown"),
        ("Size", "Unknown"),
        ("Format", "PNG"),
    ];
    assert_eq!(bare.rows(), want.map(|(k, v)| (k, v.to_owned())));
}

mod paint {
    use super::*;
    use crate::preview::Preview;
    use egui::{Color32, Context, Pos2, Rect, Shape, vec2};

    /// A painted text: its string, its place, the colour of its first section,
    /// and the override colour the shape carries.
    type Text = (String, Pos2, Color32, Option<Color32>);

    fn walk(shape: &Shape, found: &mut Vec<Text>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
            Shape::Text(t) => found.push((
                t.galley.job.text.clone(),
                t.pos,
                t.galley.job.sections[0].format.color,
                t.override_text_color,
            )),
            _ => {}
        }
    }

    fn texts() -> Vec<Text> {
        let mut preview = Preview::default();
        preview.set_meta(Some(Meta {
            name: "a.png".into(),
            dimensions: Some((64, 48)),
            size: Some(10),
            format: "PNG".into(),
        }));
        let ctx = Context::default();
        crate::theme::apply_theme(&ctx);
        let mut found = Vec::new();
        for _ in 0..3 {
            found.clear();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0))),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| preview.show(ui));
            output
                .shapes
                .iter()
                .for_each(|c| walk(&c.shape, &mut found));
            output.drop_without_applying_deltas();
        }
        found
    }

    fn at(texts: &[Text], want: &str) -> Text {
        texts.iter().find(|t| t.0 == want).cloned().unwrap()
    }

    #[test]
    fn metadata_rows_are_18_points_apart() {
        let found = texts();
        let rows = ["File", "Dimensions", "Size", "Format"].map(|k| at(&found, k).1.y);
        let pitch = tokens::PREVIEW_META_ROW_HEIGHT + tokens::SPACING_XS / 2.0;
        assert_eq!(
            rows.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>(),
            [pitch; 3]
        );
    }

    #[test]
    fn values_are_tertiary_and_keys_muted_with_no_override() {
        let found = texts();
        let value = at(&found, "PNG");
        assert_eq!((value.2, value.3), (tokens::TEXT_TERTIARY, None));
        let key = at(&found, "Format");
        assert_eq!((key.2, key.3), (tokens::TEXT_MUTED, None));
    }
}
