//! Tests for the Preview pane inside the dock.

use crate::shell::Shell;
use crate::shell::tests::{input, painted_text};
use crate::theme::apply_theme;
use egui::Context;

#[test]
fn the_preview_pane_shows_the_empty_hint_when_nothing_is_selected() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();

    // The first frame lays the dock out; the second one paints the bodies.
    let mut text = Vec::new();
    for _ in 0..2 {
        let output = ctx.run_ui(input(), |ui| shell.show(ui));
        text = painted_text(&output);
        output.drop_without_applying_deltas();
    }
    assert!(
        text.iter().any(|t| t == crate::preview::EMPTY_HINT),
        "the empty hint is not drawn: {text:?}"
    );
}

#[test]
fn the_preview_pane_shows_the_metadata_labels_and_values_once_set() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_meta(Some(crate::preview::Meta {
        name: "sketch_014.png".into(),
        dimensions: Some((2048, 1536)),
        size: Some(2_516_582),
        format: "PNG".into(),
    }));

    let mut text = Vec::new();
    for _ in 0..2 {
        let output = ctx.run_ui(input(), |ui| shell.show(ui));
        text = painted_text(&output);
        output.drop_without_applying_deltas();
    }
    for want in [
        "Metadata",
        "File",
        "sketch_014.png",
        "Dimensions",
        "2048 x 1536",
        "Size",
        "2.4 MB",
        "Format",
        "PNG",
    ] {
        assert!(
            text.iter().any(|t| t == want),
            "{want} is not drawn: {text:?}"
        );
    }
    assert!(!text.iter().any(|t| t == crate::preview::EMPTY_HINT));
}

/// The texture and rect of every picture shape in the frame.
fn painted_pictures(output: &egui::FullOutput) -> Vec<(egui::TextureId, egui::Rect)> {
    use egui::epaint::Shape;
    fn collect(shape: &Shape, found: &mut Vec<(egui::TextureId, egui::Rect)>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            Shape::Rect(rect) if rect.fill_texture_id() != egui::TextureId::default() => {
                found.push((rect.fill_texture_id(), rect.rect));
            }
            Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                found.push((mesh.texture_id, mesh.calc_bounds()));
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    found
}

#[test]
fn the_preview_pane_paints_the_picture_inside_the_pane_only_when_set() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    let id = egui::TextureId::User(7);
    let frame = |ctx: &Context, shell: &mut Shell| {
        let output = ctx.run_ui(input(), |ui| shell.show(ui));
        let found = painted_pictures(&output);
        output.drop_without_applying_deltas();
        found
    };
    frame(&ctx, &mut shell);
    assert!(
        frame(&ctx, &mut shell).is_empty(),
        "no picture without an image"
    );

    shell.preview.set_image(Some(crate::preview::PreviewImage {
        texture: id,
        size: (64, 48),
    }));
    frame(&ctx, &mut shell);
    let found = frame(&ctx, &mut shell);
    let pane = crate::shell::tests::leaf_rect(&shell, crate::layout::Pane::Preview);
    assert_eq!(found.len(), 1, "one picture: {found:?}");
    assert_eq!(found[0].0, id);
    assert!(
        pane.contains_rect(found[0].1),
        "{:?} is outside {pane:?}",
        found[0].1
    );
    // The picture grows to the box height, with its 4:3 shape kept.
    let size = found[0].1.size();
    assert!(
        (size.y - crate::tokens::PREVIEW_IMAGE_HEIGHT).abs() < 0.01,
        "{size:?}"
    );
    assert!((size.x / size.y - 4.0 / 3.0).abs() < 0.001, "{size:?}");

    shell.preview.set_image(None);
    frame(&ctx, &mut shell);
    assert!(frame(&ctx, &mut shell).is_empty(), "the picture is removed");
}

#[test]
fn a_large_picture_is_scaled_down_inside_the_pane() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_image(Some(crate::preview::PreviewImage {
        texture: egui::TextureId::User(9),
        size: (4000, 3000),
    }));
    let mut found = Vec::new();
    for _ in 0..2 {
        let output = ctx.run_ui(input(), |ui| shell.show(ui));
        found = painted_pictures(&output);
        output.drop_without_applying_deltas();
    }
    let pane = crate::shell::tests::leaf_rect(&shell, crate::layout::Pane::Preview);
    assert_eq!(found.len(), 1, "one picture: {found:?}");
    let size = found[0].1.size();
    assert!(
        pane.contains_rect(found[0].1),
        "{:?} is outside {pane:?}",
        found[0].1
    );
    assert!(
        size.x < 4000.0 && size.y <= crate::tokens::PREVIEW_IMAGE_HEIGHT + 0.01,
        "{size:?}"
    );
    assert!((size.x / size.y - 4.0 / 3.0).abs() < 0.001, "{size:?}");
}

/// The position of the first painted text that equals `want`.
fn text_pos(output: &egui::FullOutput, want: &str) -> Option<egui::Pos2> {
    use egui::epaint::Shape;
    fn find(shape: &Shape, want: &str) -> Option<egui::Pos2> {
        match shape {
            Shape::Text(text) if text.galley.job.text == want => Some(text.pos),
            Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, want)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|c| find(&c.shape, want))
}

#[test]
fn a_short_pane_scrolls_so_the_metadata_can_be_reached() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_image(Some(crate::preview::PreviewImage {
        texture: egui::TextureId::User(3),
        size: (64, 48),
    }));
    shell.preview.set_meta(Some(crate::preview::Meta {
        name: "a.png".into(),
        dimensions: Some((64, 48)),
        size: Some(10),
        format: "PNG".into(),
    }));
    // The screen is too short for the box and the grid together.
    let short = |events: Vec<egui::Event>| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 420.0),
        )),
        events,
        ..Default::default()
    };
    let frame = |ctx: &Context, shell: &mut Shell, events| {
        let output = ctx.run_ui(short(events), |ui| shell.show(ui));
        let pos = text_pos(&output, "Format");
        output.drop_without_applying_deltas();
        pos
    };
    frame(&ctx, &mut shell, vec![]);
    let pane = crate::shell::tests::leaf_rect(&shell, crate::layout::Pane::Preview);
    // egui may cull a clipped label or may paint it outside the pane.
    let visible = |pos: Option<egui::Pos2>| pos.is_some_and(|p| pane.contains(p));
    let before = frame(&ctx, &mut shell, vec![]);
    assert!(
        !visible(before),
        "the last row is already visible before scrolling"
    );

    let wheel = vec![
        egui::Event::PointerMoved(pane.center()),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -200.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    // The scroll eases over several frames, so poll until the row shows up.
    let mut found = visible(frame(&ctx, &mut shell, wheel));
    for _ in 0..30 {
        if found {
            break;
        }
        found = visible(frame(&ctx, &mut shell, vec![]));
    }
    assert!(found, "the last row is still hidden after scrolling");
}
