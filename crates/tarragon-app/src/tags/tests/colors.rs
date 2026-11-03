//! Tests for the Preview colour squares.

use std::collections::HashMap;

use eframe::egui::{Color32, Context, RawInput, vec2};
use tarragon_core::color::ColorBucket;
use tarragon_core::entity::{Tag, TagSource};
use tarragon_ui::preview::{ColorState, ColorStates, Preview};
use tarragon_ui::swatch;

use super::{preview, read_from, selection};
use crate::filtering::tests::seeded;
use crate::tags::{color_states, refresh_with};

fn tagged(name: &str, source: TagSource) -> Tag {
    Tag {
        id: 1,
        name: name.to_owned(),
        source,
    }
}

fn auto(name: &str) -> Tag {
    tagged(name, TagSource::AutoColor)
}

/// The states with these `(Swatch::ALL index, state)` pairs set.
fn states_with(set: &[(usize, ColorState)]) -> ColorStates {
    let mut states = ColorStates::default();
    for (index, state) in set {
        states[*index] = *state;
    }
    states
}

#[test]
fn one_file_makes_its_colour_squares_all_and_the_rest_none() {
    let by_file = HashMap::from([(
        "/r/a.png".to_owned(),
        vec![auto("blue"), auto("transparent")],
    )]);
    assert_eq!(
        color_states(&by_file, 1),
        states_with(&[(6, ColorState::All), (11, ColorState::All)])
    );
}

#[test]
fn a_colour_on_some_files_is_some_and_on_all_files_is_all() {
    let mut by_file = HashMap::from([
        ("/r/a.png".to_owned(), vec![auto("red")]),
        ("/r/b.png".to_owned(), Vec::new()),
    ]);
    assert_eq!(
        color_states(&by_file, 2),
        states_with(&[(0, ColorState::Some)])
    );
    by_file.insert("/r/b.png".to_owned(), vec![auto("red")]);
    assert_eq!(
        color_states(&by_file, 2),
        states_with(&[(0, ColorState::All)])
    );
}

#[test]
fn user_tags_make_no_colour_square_change() {
    let by_file = HashMap::from([(
        "/r/a.png".to_owned(),
        vec![
            tagged("Sunset", TagSource::User),
            tagged("red", TagSource::User),
        ],
    )]);
    assert_eq!(color_states(&by_file, 1), ColorStates::default());
}

#[test]
fn an_empty_selection_gives_all_none() {
    assert_eq!(color_states(&HashMap::new(), 0), ColorStates::default());
}

/// The fill of each bucket square one frame paints.
///
/// The transparent square paints smaller checker cells, so 11 squares match,
/// not 12.
fn square_fills(ctx: &Context, preview: &mut Preview) -> Vec<Color32> {
    use eframe::egui::epaint::Shape;
    fn collect(shape: &Shape, found: &mut Vec<Color32>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            Shape::Rect(r) if r.rect.size() == vec2(swatch::SIZE, swatch::SIZE) => {
                found.push(r.fill);
            }
            _ => {}
        }
    }
    let out = ctx.run_ui(RawInput::default(), |ui| preview.show(ui));
    let mut found = Vec::new();
    for clipped in &out.shapes {
        collect(&clipped.shape, &mut found);
    }
    out.drop_without_applying_deltas();
    found
}

#[test]
fn a_refresh_paints_the_colour_squares_of_the_selection() {
    let (db, _dir) = seeded();
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut preview = preview();
    // Files `a` and `b` both have red; neither has blue.
    refresh_with(&mut preview, &selection(), true, read_from(&db));
    let fills = square_fills(&ctx, &mut preview);
    assert_eq!(fills.len(), 11);
    assert_eq!(fills[0], swatch::bucket_color(ColorBucket::Red));
    let faint = swatch::bucket_color(ColorBucket::Blue).gamma_multiply(ColorState::None.opacity());
    assert_eq!(fills[6], faint);
}

#[test]
fn the_colour_setting_shows_or_hides_the_squares() {
    let (db, _dir) = seeded();
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut preview = preview();
    refresh_with(&mut preview, &selection(), false, read_from(&db));
    assert_eq!(square_fills(&ctx, &mut preview).len(), 0);
    refresh_with(&mut preview, &selection(), true, read_from(&db));
    assert_eq!(square_fills(&ctx, &mut preview).len(), 11);
}
