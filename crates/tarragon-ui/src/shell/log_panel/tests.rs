//! Tests for the Log panel content.

use super::*;
use crate::shell::Shell;
use crate::theme::apply_theme;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};

/// The headless screen.
fn input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
        ..Default::default()
    }
}

/// A shell with the log shown, and a themed context.
fn setup() -> (Context, Shell) {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.toggle_log();
    (ctx, shell)
}

/// Draw one frame with extra input events.
fn frame(
    ctx: &Context,
    shell: &mut Shell,
    view: &LogView<'_>,
    events: Vec<Event>,
) -> (egui::FullOutput, LogPanelEvents) {
    let mut result = LogPanelEvents::default();
    let output = ctx.run_ui(RawInput { events, ..input() }, |ui| {
        result = shell.show_with_log(ui, view);
    });
    (output, result)
}

/// The text shapes of a frame: text, colour, position, and size.
fn texts(output: &egui::FullOutput) -> Vec<(String, Color32, Pos2, egui::Vec2)> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<(String, Color32, Pos2, egui::Vec2)>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push((
                text.galley.job.text.clone(),
                text.galley.job.sections[0].format.color,
                text.pos,
                text.galley.size(),
            )),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

/// Click the text `label` in two settled frames and return the last events.
fn click(ctx: &Context, shell: &mut Shell, view: &LogView<'_>, label: &str) -> LogPanelEvents {
    // Two frames let the layout settle.
    frame(ctx, shell, view, vec![])
        .0
        .drop_without_applying_deltas();
    let (output, _) = frame(ctx, shell, view, vec![]);
    let (_, _, pos, size) = texts(&output)
        .into_iter()
        .find(|(text, ..)| text == label)
        .unwrap_or_else(|| panic!("{label} is painted"));
    output.drop_without_applying_deltas();
    let at = pos + size / 2.0;
    let press = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [vec![Event::PointerMoved(at)], vec![press(true)]] {
        frame(ctx, shell, view, events)
            .0
            .drop_without_applying_deltas();
    }
    let (output, events) = frame(ctx, shell, view, vec![press(false)]);
    output.drop_without_applying_deltas();
    events
}

const EMPTY: LogView<'static> = LogView {
    source: &LogSlice(&[]),
    debug: false,
};

#[test]
fn lines_use_the_level_colors() {
    let (ctx, mut shell) = setup();
    let lines = [
        LogLine {
            level: LogLevel::Debug,
            text: "d-line",
        },
        LogLine {
            level: LogLevel::Info,
            text: "i-line",
        },
        LogLine {
            level: LogLevel::Warn,
            text: "w-line",
        },
        LogLine {
            level: LogLevel::Error,
            text: "e-line",
        },
    ];
    let view = LogView {
        source: &LogSlice(&lines),
        debug: false,
    };
    frame(&ctx, &mut shell, &view, vec![])
        .0
        .drop_without_applying_deltas();
    let (output, _) = frame(&ctx, &mut shell, &view, vec![]);
    let painted = texts(&output);
    let color = |name: &str| painted.iter().find(|t| t.0 == name).map(|t| t.1);
    assert_eq!(color("d-line"), Some(tokens::TEXT_TERTIARY));
    assert_eq!(color("i-line"), Some(tokens::TEXT_PRIMARY));
    assert_eq!(color("w-line"), Some(tokens::AMBER_ACCENT));
    assert_eq!(color("e-line"), Some(tokens::CORAL_STRONG));
    output.drop_without_applying_deltas();
}

#[test]
fn clicking_clear_reports_clear() {
    let (ctx, mut shell) = setup();
    let events = click(&ctx, &mut shell, &EMPTY, "Clear");
    assert_eq!(
        events,
        LogPanelEvents {
            clear: true,
            debug: None
        }
    );
}

#[test]
fn clicking_debug_reports_the_flipped_value_once() {
    for start in [false, true] {
        let (ctx, mut shell) = setup();
        let view = LogView {
            source: &LogSlice(&[]),
            debug: start,
        };
        let events = click(&ctx, &mut shell, &view, "Debug");
        assert_eq!(
            events,
            LogPanelEvents {
                clear: false,
                debug: Some(!start)
            }
        );
        // The next frame has no click, so it reports nothing.
        let (output, next) = frame(&ctx, &mut shell, &view, vec![]);
        output.drop_without_applying_deltas();
        assert_eq!(next, LogPanelEvents::default());
    }
}

#[test]
fn a_hidden_panel_draws_no_lines_and_reports_nothing() {
    let (ctx, mut shell) = setup();
    shell.toggle_log();
    let lines = [LogLine {
        level: LogLevel::Info,
        text: "hidden-line",
    }];
    let view = LogView {
        source: &LogSlice(&lines),
        debug: true,
    };
    let (output, events) = frame(&ctx, &mut shell, &view, vec![]);
    assert!(texts(&output).iter().all(|t| t.0 != "hidden-line"));
    assert_eq!(events, LogPanelEvents::default());
    output.drop_without_applying_deltas();
}

/// Make `count` texts named `line-0`, `line-1`, and so on.
fn numbered(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("line-{i}")).collect()
}

/// Wrap the texts as info lines.
fn as_lines(owned: &[String]) -> Vec<LogLine<'_>> {
    owned
        .iter()
        .map(|text| LogLine {
            level: LogLevel::Info,
            text,
        })
        .collect()
}

/// Count the painted rows whose text starts with `line-`.
fn drawn_rows(count: usize) -> usize {
    let (ctx, mut shell) = setup();
    let owned = numbered(count);
    let lines = as_lines(&owned);
    let view = LogView {
        source: &LogSlice(&lines),
        debug: false,
    };
    for _ in 0..3 {
        frame(&ctx, &mut shell, &view, vec![])
            .0
            .drop_without_applying_deltas();
    }
    let (output, _) = frame(&ctx, &mut shell, &view, vec![]);
    let drawn = texts(&output)
        .iter()
        .filter(|t| t.0.starts_with("line-"))
        .count();
    output.drop_without_applying_deltas();
    drawn
}

#[test]
fn five_thousand_lines_paint_as_many_rows_as_fifty() {
    let few = drawn_rows(50);
    assert!(few > 0, "some rows are drawn");
    assert_eq!(drawn_rows(5000), few);
}

#[test]
fn the_view_sticks_to_the_last_line() {
    let (ctx, mut shell) = setup();
    let owned = numbered(5000);
    let lines = as_lines(&owned);
    let view = LogView {
        source: &LogSlice(&lines),
        debug: false,
    };
    for _ in 0..3 {
        frame(&ctx, &mut shell, &view, vec![])
            .0
            .drop_without_applying_deltas();
    }
    let (output, _) = frame(&ctx, &mut shell, &view, vec![]);
    let last = texts(&output).iter().any(|t| t.0.starts_with("line-4999"));
    output.drop_without_applying_deltas();
    assert!(last, "the newest line is visible");
}

#[test]
fn a_long_line_is_clipped_to_the_panel_width() {
    let (ctx, mut shell) = setup();
    let long = "x".repeat(2000);
    let lines = [LogLine {
        level: LogLevel::Info,
        text: &long,
    }];
    let view = LogView {
        source: &LogSlice(&lines),
        debug: false,
    };
    frame(&ctx, &mut shell, &view, vec![])
        .0
        .drop_without_applying_deltas();
    let (output, _) = frame(&ctx, &mut shell, &view, vec![]);
    let painted = texts(&output);
    let line = painted
        .iter()
        .find(|t| t.0.starts_with('x'))
        .expect("the long line is painted");
    let debug = painted
        .iter()
        .find(|t| t.0 == "Debug")
        .expect("the Debug checkbox is painted");
    // The Debug label sits at the right edge of the panel content.
    let (line_right, panel_right) = (line.2.x + line.3.x, debug.2.x + debug.3.x);
    output.drop_without_applying_deltas();
    assert!(
        line_right <= panel_right,
        "the line ends at {line_right}, the panel at {panel_right}"
    );
}

#[test]
fn clear_reports_once() {
    let (ctx, mut shell) = setup();
    assert!(click(&ctx, &mut shell, &EMPTY, "Clear").clear);
    let (output, next) = frame(&ctx, &mut shell, &EMPTY, vec![]);
    output.drop_without_applying_deltas();
    assert_eq!(next, LogPanelEvents::default());
}
