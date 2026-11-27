//! Tests for the name, the details line, the open button, and its menu.

use egui::epaint::Shape;
use egui::{Context, Event, FontFamily, Modifiers, PointerButton, Pos2, Rect, vec2};

use super::{OpenButton, ROW_HEIGHT};
use crate::preview::{Meta, Preview, PreviewEvent};
use crate::{fonts, tokens};

/// One painted text: the string, its first section format, and its place.
type Text = (String, egui::TextFormat, Rect);

fn meta(name: &str) -> Meta {
    Meta {
        name: name.into(),
        dimensions: Some((1536, 2048)),
        size: Some(2_516_582),
        format: "PNG".into(),
        modified: Some("Oct 14, 2026".into()),
    }
}

fn open() -> OpenButton {
    OpenButton {
        label: "Open in Krita".into(),
        choices: vec!["Krita".into(), "GIMP".into()],
    }
}

/// A themed context and a Preview of `name` with the open button.
fn setup(name: &str, button: Option<OpenButton>) -> (Context, Preview) {
    let ctx = Context::default();
    crate::theme::apply_theme(&ctx);
    let mut preview = Preview::default();
    preview.set_meta(Some(meta(name)));
    preview.set_open(button);
    frame(&ctx, &mut preview, vec![]);
    (ctx, preview)
}

/// Run one 400 by 600 point frame and return every text.
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
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0))),
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
        .unwrap_or_else(|| panic!("{want} is not drawn"))
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
fn the_name_is_a_medium_title_over_the_secondary_details_line() {
    let (ctx, mut preview) = setup("sketch_014.png", Some(open()));
    let texts = frame(&ctx, &mut preview, vec![]);
    let (_, name, name_rect) = find(&texts, "sketch_014.png");
    assert_eq!(name.font_id.size, tokens::SIZE_TITLE);
    assert_eq!(
        name.font_id.family,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into())
    );
    assert_eq!(name.color, tokens::TEXT_PRIMARY);
    let (_, line, line_rect) = find(&texts, "1536 × 2048 · 2.4 MB · PNG · Oct 14, 2026");
    assert_eq!(line.font_id.size, tokens::SIZE_BODY);
    assert_eq!(line.color, tokens::TEXT_SECONDARY);
    assert!(
        line_rect.top() >= name_rect.bottom() - 0.5,
        "the line is under the name"
    );
}

#[test]
fn the_open_button_sits_right_of_the_name_and_queues_open() {
    let (ctx, mut preview) = setup("a.png", Some(open()));
    let texts = frame(&ctx, &mut preview, vec![]);
    let name = find(&texts, "a.png").2;
    let label = find(&texts, "Open in Krita").2;
    assert!(label.left() > name.right());
    assert!((label.center().y - name.bottom()).abs() < ROW_HEIGHT / 2.0);
    click(&ctx, &mut preview, label.center());
    assert_eq!(preview.take_events(), [PreviewEvent::Open]);
}

#[test]
fn the_caret_lists_the_apps_and_a_row_queues_its_index() {
    let (ctx, mut preview) = setup("a.png", Some(open()));
    let texts = frame(&ctx, &mut preview, vec![]);
    let label = find(&texts, "Open in Krita").2;
    // The caret is the square right of the button, past the button gap.
    let caret = Pos2::new(label.right() + 30.0, label.center().y);
    click(&ctx, &mut preview, caret);
    let menu = frame(&ctx, &mut preview, vec![]);
    let gimp = find(&menu, "GIMP").2;
    find(&menu, "Krita");
    click(&ctx, &mut preview, gimp.center());
    assert_eq!(preview.take_events(), [PreviewEvent::OpenWith(1)]);
}

#[test]
fn with_no_open_button_only_the_text_shows() {
    let (ctx, mut preview) = setup("a.png", None);
    let texts = frame(&ctx, &mut preview, vec![]);
    find(&texts, "a.png");
    assert!(!texts.iter().any(|(t, ..)| t.starts_with("Open")));
}

#[test]
fn a_long_name_ends_before_the_button() {
    let long = "a_very_long_reference_sketch_name_for_the_overflow_case_number_two.png";
    let (ctx, mut preview) = setup(long, Some(open()));
    let texts = frame(&ctx, &mut preview, vec![]);
    let label = find(&texts, "Open in Krita").2;
    let name = texts
        .iter()
        .find(|(t, f, _)| t.starts_with("a_very_long") && f.font_id.size == tokens::SIZE_TITLE)
        .expect("the name is drawn")
        .2;
    assert!(
        name.right() <= label.left(),
        "{name:?} runs under {label:?}"
    );
}
