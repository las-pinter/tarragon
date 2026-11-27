//! Tests for the row under the mosaic.

use egui::epaint::Shape;
use egui::{Context, Event, FontFamily, Modifiers, PointerButton, Pos2, Rect, TextureId, vec2};

use super::{EDIT_TAGS_TEXT, MultiSummary, OPEN_ALL_TEXT, detail_text};
use crate::preview::{ColorState, MosaicCell, Preview, PreviewEvent, PreviewImage, TagPill};
use crate::{fonts, tokens};

/// One painted text: the string, its first section format, and its place.
type Text = (String, egui::TextFormat, Rect);

fn mosaic(count: usize) -> Vec<MosaicCell> {
    (0..count)
        .map(|i| MosaicCell {
            image: PreviewImage {
                texture: TextureId::User(10 + i as u64),
                size: (64, 48),
            },
            path: format!("/a/{i}.png").into(),
        })
        .collect()
}

/// A themed context and a Preview of `count` selected images, `shown` ready.
fn setup(shown: usize, count: usize, app: Option<&str>) -> (Context, Preview) {
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    let mut preview = Preview::default();
    preview.set_mosaic(mosaic(shown), count);
    preview.set_summary(MultiSummary {
        bytes: 19_503_513,
        app: app.map(str::to_owned),
    });
    preview.set_tags(vec![TagPill {
        name: "hands".into(),
        state: ColorState::Some,
    }]);
    frame(&ctx, &mut preview, vec![]);
    (ctx, preview)
}

/// Run one 500 by 800 point frame and return every text.
fn frame(ctx: &Context, preview: &mut Preview, events: Vec<Event>) -> Vec<Text> {
    fn collect(shape: &Shape, found: &mut Vec<Text>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            Shape::Text(t) => found.push((
                t.galley.job.text.clone(),
                t.galley.job.sections[0].format.clone(),
                Rect::from_min_size(t.pos, t.galley.size()),
            )),
            _ => {}
        }
    }
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 800.0))),
        events,
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| preview.show(ui));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    output.drop_without_applying_deltas();
    found
}

fn find<'a>(texts: &'a [Text], want: &str) -> &'a Text {
    texts
        .iter()
        .find(|(text, ..)| text == want)
        .unwrap_or_else(|| panic!("{want} is not drawn: {texts:?}"))
}

fn has(texts: &[Text], want: &str) -> bool {
    texts.iter().any(|(text, ..)| text == want)
}

/// Click at `at`, with the pointer moved in a frame of its own first.
fn click(ctx: &Context, preview: &mut Preview, at: Pos2) {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(ctx, preview, vec![Event::PointerMoved(at)]);
    frame(ctx, preview, vec![button(true)]);
    frame(ctx, preview, vec![button(false)]);
}

#[test]
fn the_count_is_a_medium_title_over_the_total_size() {
    let (ctx, mut preview) = setup(6, 6, Some("Krita"));
    let texts = frame(&ctx, &mut preview, vec![]);
    let (_, count, count_rect) = find(&texts, "6 selected");
    assert_eq!(count.font_id.size, tokens::SIZE_TITLE);
    assert_eq!(
        count.font_id.family,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into())
    );
    let (_, line, line_rect) = find(&texts, "18.6 MB in all");
    assert_eq!(line.color, tokens::TEXT_SECONDARY);
    assert!(line_rect.top() >= count_rect.bottom() - 0.5);
}

#[test]
fn images_past_the_cap_join_the_size_line() {
    let (ctx, mut preview) = setup(9, 12, None);
    let texts = frame(&ctx, &mut preview, vec![]);
    find(&texts, "12 selected");
    find(&texts, "18.6 MB in all · +3 more");
}

#[test]
fn the_open_button_names_one_app_or_reads_open_all() {
    let (ctx, mut preview) = setup(2, 2, Some("Krita"));
    let texts = frame(&ctx, &mut preview, vec![]);
    let label = find(&texts, "Open all in Krita").2;
    click(&ctx, &mut preview, label.center());
    assert_eq!(preview.take_events(), [PreviewEvent::OpenAll]);

    let (ctx, mut preview) = setup(2, 2, None);
    let texts = frame(&ctx, &mut preview, vec![]);
    find(&texts, OPEN_ALL_TEXT);
}

#[test]
fn a_long_app_name_gives_way_to_the_plain_open_all() {
    let long = "Very Long Desktop Image Editing Application Name";
    let (ctx, mut preview) = setup(2, 2, Some(long));
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(!has(&texts, &format!("{OPEN_ALL_TEXT} in {long}")));
    let button = find(&texts, OPEN_ALL_TEXT).2;
    let count = find(&texts, "2 selected").2;
    assert!(count.right() < button.left(), "{count:?} meets {button:?}");
}

#[test]
fn edit_tags_shows_and_hides_the_tag_row_and_the_state_lasts() {
    let (ctx, mut preview) = setup(3, 3, None);
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(!has(&texts, "hands"), "the row is hidden at first");
    assert!(!has(&texts, "+ Tag"));
    let edit = find(&texts, EDIT_TAGS_TEXT).2;
    assert!(edit.right() < find(&texts, OPEN_ALL_TEXT).2.left());
    click(&ctx, &mut preview, edit.center());
    // The mosaic gives the row its room on the next frame.
    frame(&ctx, &mut preview, vec![]);
    let texts = frame(&ctx, &mut preview, vec![]);
    let pill = find(&texts, "hands").2;
    let edit = find(&texts, EDIT_TAGS_TEXT).2;
    assert!(pill.top() > edit.bottom(), "the row sits under the footer");
    assert!(pill.bottom() < 800.0, "the row stays in view: {pill:?}");
    find(&texts, "+ Tag");

    preview.selection_changed();
    preview.set_mosaic(mosaic(4), 4);
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(has(&texts, "hands"), "the open row lasts across selections");

    let edit = find(&texts, EDIT_TAGS_TEXT).2;
    click(&ctx, &mut preview, edit.center());
    let texts = frame(&ctx, &mut preview, vec![]);
    assert!(!has(&texts, "hands"));
}

/// Run one frame with no input and return every filled rect and its fill.
fn fills(ctx: &Context, preview: &mut Preview) -> Vec<(Rect, egui::Color32)> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 800.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| preview.show(ui));
    let found = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(r) => Some((r.rect, r.fill)),
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    found
}

#[test]
fn the_mosaic_fills_the_height_the_footer_leaves() {
    let (ctx, mut preview) = setup(6, 6, None);
    frame(&ctx, &mut preview, vec![]);
    let texts = frame(&ctx, &mut preview, vec![]);
    let footer = find(&texts, "6 selected").2;
    // The footer sits at the bottom of the 800 point screen.
    assert!(footer.bottom() > 750.0, "{footer:?}");
    let cells_bottom = fills(&ctx, &mut preview)
        .iter()
        .filter(|(_, fill)| *fill == tokens::BG_TERTIARY)
        .map(|(rect, _)| rect.bottom())
        .fold(0.0, f32::max);
    // The block gap and half the 34 pt row lie between the cells and the text.
    let gap = footer.top() - cells_bottom;
    assert!(
        gap > 0.0 && gap < 40.0,
        "the cells end {gap} above the footer"
    );
}

#[test]
fn edit_tags_is_lit_while_the_row_shows() {
    let (ctx, mut preview) = setup(3, 3, None);
    let texts = frame(&ctx, &mut preview, vec![]);
    let edit = find(&texts, EDIT_TAGS_TEXT).2;
    // The button fill: the last small rect painted under the label.
    let fill_at = |found: &[(Rect, egui::Color32)], at: Pos2| {
        found
            .iter()
            .filter(|(rect, _)| rect.contains(at) && rect.width() < 200.0)
            .map(|(_, fill)| *fill)
            .next_back()
    };
    assert_eq!(
        fill_at(&fills(&ctx, &mut preview), edit.center()),
        Some(tokens::SURFACE_HOVER)
    );
    click(&ctx, &mut preview, edit.center());
    // Move away, so the fill is the rest fill and not the hover one. The
    // footer moves up once the row takes its room.
    frame(&ctx, &mut preview, vec![Event::PointerMoved(Pos2::ZERO)]);
    frame(&ctx, &mut preview, vec![]);
    let edit = find(&frame(&ctx, &mut preview, vec![]), EDIT_TAGS_TEXT).2;
    assert_eq!(
        fill_at(&fills(&ctx, &mut preview), edit.center()),
        Some(tokens::SURFACE_HIGHLIGHT)
    );
}

#[test]
fn the_detail_line_reads_the_size_alone_without_a_hint() {
    let summary = MultiSummary {
        bytes: 2048,
        app: None,
    };
    assert_eq!(detail_text(&summary, None), "2.0 KB in all");
    assert_eq!(
        detail_text(&summary, Some("+1 more")),
        "2.0 KB in all · +1 more"
    );
}

#[test]
fn both_footer_buttons_are_the_row_height() {
    let (ctx, mut preview) = setup(2, 2, Some("Krita"));
    let texts = frame(&ctx, &mut preview, vec![]);
    let found = fills(&ctx, &mut preview);
    for (label, fill) in [
        ("Open all in Krita", tokens::CORAL_BRIGHT),
        (EDIT_TAGS_TEXT, tokens::SURFACE_HOVER),
    ] {
        let at = find(&texts, label).2.center();
        let button = found
            .iter()
            .find(|(rect, f)| *f == fill && rect.contains(at))
            .unwrap_or_else(|| panic!("{label} has no button fill"))
            .0;
        assert!(
            (button.height() - super::ROW_HEIGHT).abs() < 0.01,
            "{label} is {} tall",
            button.height()
        );
    }
}
