//! Tests for the dock: where the panels sit, and what hides or shows them.

use super::*;
use crate::layout::{first_dock, is_visible, to_json};
use crate::shell::tab_places;
use crate::shell::tests::{draw, input, leaf_rect, painted_text};
use crate::shell::{DOCK_GALLERY, DOCK_LIBRARY, DOCK_LOG, DOCK_PREVIEW, Shell};
use crate::theme::apply_theme;
use crate::tokens;
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, RawInput, vec2};

#[test]
fn the_docks_draw_at_the_default_fractions() {
    let ctx = Context::default();
    apply_theme(&ctx);

    let mut shell = Shell::new();
    draw(&ctx, &mut shell);

    // The splits are shares of the window width; the screen is wider than the
    // default width the shares come from. A divider takes a few points.
    let width = 1400.0;
    let library = tokens::SIDEBAR_WIDTH / tokens::WINDOW_SIZE[0] * width;
    let preview = tokens::PREVIEW_WIDTH / (tokens::WINDOW_SIZE[0] - tokens::SIDEBAR_WIDTH)
        * (width - library);
    let got_library = leaf_rect(&shell, Pane::Library).width();
    let got_preview = leaf_rect(&shell, Pane::Preview).width();
    assert!(
        (got_library - library).abs() < 6.0,
        "library is {got_library} points wide, want {library}"
    );
    assert!(
        (got_preview - preview).abs() < 6.0,
        "preview is {got_preview} points wide, want {preview}"
    );
}

#[test]
fn the_default_dock_draws_the_panels_and_the_log_after_a_toggle() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();

    // Each panel paints its own title text; the gallery paints its sort pill.
    let painted = |ctx: &Context, shell: &mut Shell| {
        let output = ctx.run_ui(input(), |ui| shell.show(ui));
        let text = painted_text(&output);
        output.drop_without_applying_deltas();
        text
    };
    painted(&ctx, &mut shell);
    let text = painted(&ctx, &mut shell);
    for label in [DOCK_LIBRARY, DOCK_PREVIEW, DOCK_GALLERY] {
        assert!(text.iter().any(|t| t == label), "{label} is not drawn");
    }
    assert!(
        text.iter().any(|t| t == "FAVORITES"),
        "the library body is not drawn: {text:?}"
    );
    assert!(
        text.iter()
            .any(|t| t.contains("Natural") || t.contains("Date")),
        "the gallery body is not drawn: {text:?}"
    );
    assert!(!text.iter().any(|t| t == "Clear"), "the log starts hidden");

    shell.toggle_log();
    painted(&ctx, &mut shell);
    let text = painted(&ctx, &mut shell);
    assert!(text.iter().any(|t| t == "Clear"), "the log body is drawn");
    let [log, gallery] = [Pane::Log, Pane::Gallery].map(|pane| leaf_rect(&shell, pane));
    assert!(
        log.min.y >= gallery.max.y - 1.0,
        "the log sits under the gallery"
    );
}

#[test]
fn the_log_toggle_changes_whether_the_panel_is_drawn() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();

    let drawn = |ctx: &Context, shell: &mut Shell| {
        draw(ctx, shell);
        // A pane in the tree must also have a drawn leaf.
        is_visible(&shell.dock, Pane::Log) && {
            leaf_rect(shell, Pane::Log);
            true
        }
    };

    assert!(!drawn(&ctx, &mut shell), "the log panel starts hidden");

    assert!(shell.toggle_log(), "the first toggle shows the log");
    assert!(drawn(&ctx, &mut shell), "the log panel is drawn");

    assert!(!shell.toggle_log(), "the second toggle hides the log");
    assert!(!drawn(&ctx, &mut shell), "the log panel is hidden again");
}

#[test]
fn each_panel_toggles_off_and_back_on() {
    let mut shell = Shell::new();
    for pane in [Pane::Library, Pane::Gallery, Pane::Preview, Pane::Log] {
        let before = is_visible(&shell.dock, pane);
        assert_eq!(shell.toggle(pane), !before);
        assert_eq!(is_visible(&shell.dock, pane), !before);
        assert_eq!(shell.toggle(pane), before);
        assert_eq!(is_visible(&shell.dock, pane), before);
    }
    assert_eq!(
        shell.dock.iter_all_tabs().count(),
        3,
        "the panes are back once each"
    );
}

#[test]
fn two_hidden_panes_come_back_at_the_default_widths_in_either_order() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut start = Shell::new();
    start.toggle_log();
    draw(&ctx, &mut start);
    let want = widths(&start);
    for order in [[Pane::Gallery, Pane::Log], [Pane::Log, Pane::Gallery]] {
        let mut shell = Shell::new();
        shell.toggle_log();
        for pane in [Pane::Gallery, Pane::Log] {
            shell.toggle(pane);
        }
        draw(&ctx, &mut shell);
        for pane in order {
            shell.toggle(pane);
        }
        draw(&ctx, &mut shell);
        let got = widths(&shell);
        assert_eq!(got.len(), want.len(), "{order:?}");
        for (got, want) in got.iter().zip(&want) {
            assert!(
                (got.1 - want.1).abs() < 1.0,
                "{:?} is {} wide after {order:?}, was {}",
                want.0,
                got.1,
                want.1
            );
        }
    }
}

/// The widths of the shown leaves, from the last frame.
fn widths(shell: &Shell) -> Vec<(Pane, f32)> {
    [Pane::Library, Pane::Gallery, Pane::Preview, Pane::Log]
        .into_iter()
        .filter(|pane| is_visible(&shell.dock, *pane))
        .map(|pane| (pane, leaf_rect(shell, pane).width()))
        .collect()
}

#[test]
fn a_pane_hidden_and_shown_again_keeps_its_default_width() {
    let ctx = Context::default();
    apply_theme(&ctx);
    // Two docks: the fresh start, and the one with the Log shown.
    for log in [false, true] {
        let fresh = || {
            let mut shell = Shell::new();
            if log {
                shell.toggle_log();
            }
            shell
        };
        let mut start = fresh();
        draw(&ctx, &mut start);
        let want = widths(&start);
        for (pane, _) in &want {
            let mut shell = fresh();
            shell.toggle(*pane);
            draw(&ctx, &mut shell);
            shell.toggle(*pane);
            draw(&ctx, &mut shell);
            let got = widths(&shell);
            assert_eq!(got.len(), want.len());
            for (got, want) in got.iter().zip(&want) {
                assert_eq!(got.0, want.0);
                assert!(
                    (got.1 - want.1).abs() < 1.0,
                    "{:?} is {} wide after {pane:?} came back, was {}",
                    want.0,
                    got.1,
                    want.1
                );
            }
        }
    }
}

#[test]
fn hiding_a_tab_removes_it_like_the_toggle() {
    let mut closed = first_dock();
    let mut toggled = Shell::new();

    // The Hide menu action and the toggle both go through `remove_tab`, so the
    // trees match.
    let path = closed.find_tab(&Pane::Preview).expect("preview shows");
    closed.remove_tab(path);
    toggled.toggle(Pane::Preview);

    assert!(!is_visible(&closed, Pane::Preview));
    assert_eq!(to_json(&closed), to_json(&toggled.dock));
}

/// Run one frame with `events`, and return the text and its position.
fn frame_text(ctx: &Context, shell: &mut Shell, events: Vec<egui::Event>) -> Vec<(String, Pos2)> {
    fn collect(shape: &egui::epaint::Shape, text: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::epaint::Shape::Text(shape) => {
                text.push((shape.galley.job.text.clone(), shape.pos));
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, text)),
            _ => {}
        }
    }
    let output = ctx.run_ui(RawInput { events, ..input() }, |ui| shell.show(ui));
    let mut text = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut text);
    }
    output.drop_without_applying_deltas();
    text
}

/// A pointer button press or release at `pos`.
fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn each_panel_title_is_painted_once() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    for log_shown in [false, true] {
        if log_shown {
            shell.toggle_log();
        }
        frame_text(&ctx, &mut shell, vec![]);
        let text = frame_text(&ctx, &mut shell, vec![]);
        let titles = [DOCK_LIBRARY, DOCK_GALLERY, DOCK_PREVIEW, DOCK_LOG];
        for title in titles {
            let want = usize::from(title != DOCK_LOG || log_shown);
            let got = text.iter().filter(|(t, _)| t == title).count();
            assert_eq!(
                got, want,
                "{title} is painted {got} times (log: {log_shown})"
            );
        }
    }
}

#[test]
fn the_tab_bar_is_a_title_strip_with_no_buttons() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    draw(&ctx, &mut shell);

    // Nothing but the title text and the strip is in the tab bar: a close
    // button, a close-all button, or a collapse arrow would add more shapes.
    // The bar is one title line plus the padding above and below, so the text
    // sits one padding below the leaf top.
    let leaf = leaf_rect(&shell, Pane::Library);
    let text = frame_text(&ctx, &mut shell, vec![]);
    let (_, title_pos) = text
        .iter()
        .find(|(t, _)| t == DOCK_LIBRARY)
        .expect("the title is painted");
    let want = leaf.min.y + tokens::SPACING_XS;
    assert!(
        (title_pos.y - want).abs() < 1.0,
        "title at {}, want {want}",
        title_pos.y
    );

    // A click where a close-all button would sit changes nothing.
    let corner = Pos2::new(leaf.max.x - 12.0, leaf.min.y + 10.0);
    let events = vec![
        Event::PointerMoved(corner),
        button(corner, PointerButton::Primary, true),
        button(corner, PointerButton::Primary, false),
    ];
    frame_text(&ctx, &mut shell, events);
    frame_text(&ctx, &mut shell, vec![]);
    assert_eq!(to_json(&shell.dock), to_json(&first_dock()));
}

#[test]
fn the_menu_actions_hide_a_pane_and_reset_the_layout() {
    let mut hidden = Shell::new();
    hidden.apply_tab_menu(TabMenu::Hide(Pane::Preview));
    let mut toggled = Shell::new();
    toggled.toggle(Pane::Preview);
    assert_eq!(to_json(&hidden.dock), to_json(&toggled.dock));

    hidden.toggle(Pane::Gallery);
    hidden.toggle_log();
    hidden.apply_tab_menu(TabMenu::Reset);
    assert_eq!(to_json(&hidden.dock), to_json(&first_dock()));
}

#[test]
fn a_right_click_on_a_tab_offers_hide_panel_and_reset() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    draw(&ctx, &mut shell);

    let leaf = leaf_rect(&shell, Pane::Preview);
    let tab = Pos2::new(leaf.min.x + 20.0, leaf.min.y + 10.0);
    let events = vec![
        Event::PointerMoved(tab),
        button(tab, PointerButton::Secondary, true),
        button(tab, PointerButton::Secondary, false),
    ];
    frame_text(&ctx, &mut shell, events);
    let text = frame_text(&ctx, &mut shell, vec![]);
    let find = |label: &str| text.iter().find(|(t, _)| t == label).map(|(_, p)| *p);
    let hide = find("Hide panel").expect("the menu offers Hide panel");
    assert!(find("Reset Window Layout").is_some());
    assert!(find("Eject").is_none() && find("Close").is_none());

    let click = hide + vec2(10.0, 6.0);
    let events = vec![
        Event::PointerMoved(click),
        button(click, PointerButton::Primary, true),
        button(click, PointerButton::Primary, false),
    ];
    frame_text(&ctx, &mut shell, events);
    frame_text(&ctx, &mut shell, vec![]);
    assert!(!shell.preview_visible(), "Hide panel removed the pane");
}

#[test]
fn dropping_a_dragged_tab_asks_for_a_repaint() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    draw(&ctx, &mut shell);
    draw(&ctx, &mut shell);
    let before = tab_places(&shell.dock);

    let from = leaf_rect(&shell, Pane::Preview).min + vec2(20.0, 10.0);
    let gallery = leaf_rect(&shell, Pane::Gallery);
    let to = gallery.center();
    let mut repainted = false;
    let steps = [
        vec![Event::PointerMoved(from)],
        vec![button(from, PointerButton::Primary, true)],
        vec![Event::PointerMoved(from + vec2(-30.0, 40.0))],
        vec![Event::PointerMoved(to)],
        vec![Event::PointerMoved(to)],
        vec![button(to, PointerButton::Primary, false)],
    ];
    for events in steps {
        let output = ctx.run_ui(RawInput { events, ..input() }, |ui| shell.show(ui));
        output.drop_without_applying_deltas();
        repainted = ctx.requested_repaint_last_pass();
    }
    assert_ne!(tab_places(&shell.dock), before, "the drop moved the tab");
    assert!(repainted, "the drop asks for another frame");
}

#[test]
fn every_panel_title_paints_in_amber() {
    fn panel_title_colors(ctx: &Context, shell: &mut Shell) -> Vec<(String, Color32)> {
        let output = ctx.run_ui(input(), |ui| shell.show(ui));
        let mut found = Vec::new();
        for clipped in &output.shapes {
            if let egui::epaint::Shape::Text(shape) = &clipped.shape {
                let text = shape.galley.job.text.clone();
                if [DOCK_LIBRARY, DOCK_GALLERY, DOCK_PREVIEW, DOCK_LOG].contains(&text.as_str()) {
                    // A section colour is the one the glyphs use.
                    let color = shape.galley.job.sections[0].format.color;
                    found.push((text, color));
                }
            }
        }
        output.drop_without_applying_deltas();
        found
    }

    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.toggle_log();
    panel_title_colors(&ctx, &mut shell);
    let titles = panel_title_colors(&ctx, &mut shell);
    assert_eq!(titles.len(), 4, "{titles:?}");
    for (title, color) in titles {
        assert_eq!(color, tokens::AMBER_ACCENT, "{title} is not amber");
    }
}

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
