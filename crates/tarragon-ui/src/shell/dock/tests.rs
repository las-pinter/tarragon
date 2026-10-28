//! Tests for the dock: where the panels sit, and what hides or shows them.

use super::*;
use crate::layout::{first_dock, is_visible, to_json};
use crate::shell::tests::{draw, input, leaf_rect, painted_text};
use crate::shell::{DOCK_GALLERY, DOCK_LIBRARY, DOCK_PREVIEW, Shell};
use crate::theme::apply_theme;
use crate::tokens;
use egui::Context;

#[test]
fn the_docks_draw_at_the_default_fractions() {
    let ctx = Context::default();
    apply_theme(&ctx);

    let mut shell = Shell::new();
    draw(&ctx, &mut shell);

    // The splits are shares of the window width; the screen is wider than the
    // 1280 points the shares come from. A divider takes a few points.
    let width = 1400.0;
    let library = tokens::SIDEBAR_WIDTH / 1280.0 * width;
    let preview = tokens::PREVIEW_WIDTH / (1280.0 - tokens::SIDEBAR_WIDTH) * (width - library);
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
fn removing_a_tab_gives_the_same_tree_as_the_toggle() {
    let mut closed = first_dock();
    let mut toggled = Shell::new();

    // The close button calls `DockState::remove_tab`, as `set_visible` does,
    // so the trees match. The button itself is not clicked here.
    let path = closed.find_tab(&Pane::Preview).expect("preview shows");
    closed.remove_tab(path);
    toggled.toggle(Pane::Preview);

    assert!(!is_visible(&closed, Pane::Preview));
    assert_eq!(to_json(&closed), to_json(&toggled.dock));
}
