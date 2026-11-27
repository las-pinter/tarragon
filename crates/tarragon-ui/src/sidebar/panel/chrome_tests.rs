//! Tests for the Library panel chrome: the section heads and the add action.

use super::chrome::ADD_ID;
use super::tests::{find, frame, panel, panel_with, ready, warm_up};
use super::*;
use crate::tokens;
use egui::{Id, Pos2, RawInput, Rect, Shape, vec2};

#[test]
fn the_heads_and_the_empty_text_are_painted() {
    let mut sidebar = Sidebar::default();
    let ctx = ready();
    warm_up(&ctx, &mut sidebar);
    let texts = frame(&ctx, &mut sidebar, Vec::new());
    for wanted in ["FOLDERS", "FAVORITES", "No favorites yet."] {
        find(&texts, wanted);
    }
}

#[test]
fn the_heads_use_the_medium_family_and_letter_spacing() {
    let mut sidebar = Sidebar::default();
    let ctx = ready();
    warm_up(&ctx, &mut sidebar);
    let texts = frame(&ctx, &mut sidebar, Vec::new());
    for wanted in ["FOLDERS", "FAVORITES"] {
        let format = &find(&texts, wanted).format;
        assert_eq!(
            format.font_id.family,
            egui::FontFamily::Name(crate::fonts::MEDIUM_FAMILY.into())
        );
        assert!(
            (format.extra_letter_spacing - 0.88).abs() < 1e-4,
            "{wanted}"
        );
    }
}

#[test]
fn the_add_action_stays_on_screen_with_a_tall_tree() {
    let mut sidebar = panel_with(60);
    let texts = frame(&ready(), &mut sidebar, Vec::new());
    let y = find(&texts, ADD_TEXT).pos.y;
    assert!(y > 0.0 && y < 300.0, "the add action at {y}");
}

#[test]
fn the_add_action_paints_coral_medium_text_and_a_plus() {
    let ctx = ready();
    let mut sidebar = panel();
    warm_up(&ctx, &mut sidebar);
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
            ..Default::default()
        },
        |ui| sidebar.show(ui),
    );
    let mut text = None;
    let mut plus = 0;
    for clipped in &output.shapes {
        match &clipped.shape {
            Shape::Text(shape) if shape.galley.job.text == ADD_TEXT => {
                text = Some((shape.pos, shape.galley.job.sections[0].format.clone()));
            }
            Shape::LineSegment { stroke, .. } if stroke.color == tokens::CORAL_STRONG => plus += 1,
            _ => {}
        }
    }
    output.drop_without_applying_deltas();
    let (pos, format) = text.expect("the add text painted");
    assert_eq!(format.color, tokens::CORAL_STRONG);
    assert_eq!(
        format.font_id.family,
        egui::FontFamily::Name(crate::fonts::MEDIUM_FAMILY.into())
    );
    assert_eq!(plus, 2, "the plus is two strokes");
    // 12 pt of panel padding, 8 of action padding, the 16 pt plus, and 8 of gap.
    assert!((pos.x - 44.0).abs() < 0.5, "add text at {}", pos.x);
}

#[test]
fn the_heads_keep_the_page_rhythm() {
    let ctx = ready();
    let mut sidebar = panel();
    warm_up(&ctx, &mut sidebar);
    let texts = frame(&ctx, &mut sidebar, Vec::new());
    let folders = find(&texts, "FOLDERS");
    // 16 pt of panel padding and 4 pt above the label; 20 pt of label inset.
    assert!(
        (folders.pos.y - 20.0).abs() < 0.5,
        "FOLDERS at {:?}",
        folders.pos
    );
    assert!(
        (folders.pos.x - 20.0).abs() < 0.5,
        "FOLDERS at {:?}",
        folders.pos
    );
    // The first row starts 8 pt below the label, and its name is centred in 34.
    let root = find(&texts, "r");
    let row_top = folders.pos.y + folders.height + 8.0;
    let want = row_top + (34.0 - root.height) / 2.0;
    assert!(
        (root.pos.y - want).abs() < 0.5,
        "r at {}, want {want}",
        root.pos.y
    );
    // The next row follows after the 4 pt row gap.
    let next = find(&texts, "a");
    assert!(
        (next.pos.y - root.pos.y - 38.0).abs() < 0.5,
        "a at {}",
        next.pos.y
    );
}

#[test]
fn a_focused_add_action_paints_the_coral_ring() {
    let ctx = ready();
    let mut sidebar = panel();
    warm_up(&ctx, &mut sidebar);
    let ring = egui::Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
    let rings = |ctx: &egui::Context, sidebar: &mut Sidebar| {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                ..Default::default()
            },
            |ui| sidebar.show(ui),
        );
        let count = output
            .shapes
            .iter()
            .filter(|clipped| matches!(&clipped.shape, Shape::Rect(rect) if rect.stroke == ring))
            .count();
        output.drop_without_applying_deltas();
        count
    };
    assert_eq!(rings(&ctx, &mut sidebar), 0, "a ring without focus");
    ctx.memory_mut(|memory| memory.request_focus(Id::new(ADD_ID)));
    assert_eq!(
        rings(&ctx, &mut sidebar),
        1,
        "no ring on the focused action"
    );
}
