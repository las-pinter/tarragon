//! Tests for the rows of the top-bar menus and the tab menu.

use egui::epaint::{ColorMode, Shape};
use egui::{Color32, Context, Event, Modifiers, PointerButton, Pos2, Rect};

use crate::layout::Pane;
use crate::shell::tests::leaf_rect;
use crate::theme::apply_theme;
use crate::{Shell, test_support, tokens};

/// What one frame painted: texts with their colour, fills, and tick lines.
#[derive(Default)]
struct Painted {
    texts: Vec<(String, Rect, Color32)>,
    fills: Vec<(Rect, Color32)>,
    ticks: Vec<Rect>,
}

impl Painted {
    fn collect(&mut self, shape: &Shape) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| self.collect(s)),
            Shape::Text(t) => {
                let color = t
                    .galley
                    .job
                    .sections
                    .first()
                    .map(|s| s.format.color)
                    .filter(|c| *c != Color32::PLACEHOLDER)
                    .unwrap_or(t.fallback_color);
                let rect = t.galley.rect.translate(t.pos.to_vec2());
                self.texts.push((t.galley.job.text.clone(), rect, color));
            }
            Shape::Rect(r) => self.fills.push((r.rect, r.fill)),
            Shape::Path(p) if p.stroke.color == ColorMode::Solid(tokens::AMBER_ACCENT) => {
                self.ticks.push(Rect::from_points(&p.points));
            }
            _ => {}
        }
    }

    /// The last text `want`: a popup paints after the page, so it is on top.
    fn text(&self, want: &str) -> (Rect, Color32) {
        self.texts
            .iter()
            .rev()
            .find(|(t, _, _)| t == want)
            .map_or_else(
                || panic!("no text {want:?}"),
                |(_, rect, color)| (*rect, *color),
            )
    }

    /// Whether a coral-deep fill sits under the text `want`.
    fn lit(&self, want: &str) -> bool {
        self.filled(want, tokens::CORAL_DARK)
    }

    /// Whether a fill of `color` sits under the text `want`.
    fn filled(&self, want: &str, color: Color32) -> bool {
        let (rect, _) = self.text(want);
        self.fills
            .iter()
            .any(|(fill, c)| *c == color && fill.contains_rect(rect))
    }

    /// Whether a tick sits left of the text `want`, on its row.
    fn ticked(&self, want: &str) -> bool {
        let (rect, _) = self.text(want);
        self.ticks.iter().any(|tick| {
            tick.max.x < rect.min.x && (tick.center().y - rect.center().y).abs() < rect.height()
        })
    }
}

/// A context with the app theme and no fade, so a menu paints at full
/// opacity on its first frame.
fn themed() -> Context {
    let ctx = test_support::context();
    apply_theme(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx
}

fn frame(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> Painted {
    let mut input = test_support::input(1400.0, events);
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 900.0)));
    let output = ctx.run_ui(input, |ui| shell.show(ui));
    let mut painted = Painted::default();
    for clipped in &output.shapes {
        painted.collect(&clipped.shape);
    }
    output.drop_without_applying_deltas();
    painted
}

/// Open the top-bar menu `label`, then rest the pointer at `hover` and
/// return the last frame.
fn open_menu(ctx: &Context, shell: &mut Shell, label: &str, hover: &str) -> Painted {
    frame(ctx, shell, vec![]);
    let bar = frame(ctx, shell, vec![]);
    frame(ctx, shell, test_support::click(bar.text(label).0.center()));
    let menu = frame(ctx, shell, vec![]);
    let at = menu.text(hover).0.center();
    frame(ctx, shell, vec![Event::PointerMoved(at)]);
    frame(ctx, shell, vec![Event::PointerMoved(at)])
}

#[test]
fn a_hovered_file_menu_row_takes_the_coral_deep_fill_and_light_coral_text() {
    let ctx = themed();
    let mut shell = Shell::new();
    let painted = open_menu(&ctx, &mut shell, "File", "Quit");
    assert!(painted.lit("Quit"), "Quit is not lit");
    assert_eq!(painted.text("Quit").1, tokens::CORAL_STRONG);
    assert!(!painted.lit("Open Folder..."), "Open Folder... is lit");
    assert_eq!(painted.text("Open Folder...").1, tokens::TEXT_PRIMARY);
    // The File button stays lit while its menu shows.
    assert!(
        painted.filled("File", tokens::SURFACE_HIGHLIGHT),
        "File is not lit"
    );
    assert_eq!(painted.text("File").1, tokens::TEXT_PRIMARY);
    assert!(
        !painted.filled("View", tokens::SURFACE_HIGHLIGHT),
        "View is lit"
    );
}

#[test]
fn the_view_menu_ticks_the_shown_panels_and_lights_the_hovered_row() {
    let ctx = themed();
    let mut shell = Shell::new();
    let painted = open_menu(&ctx, &mut shell, "View", "Log");
    for shown in ["Library", "Gallery", "Preview"] {
        assert!(painted.ticked(shown), "{shown} has no tick");
    }
    assert!(!painted.ticked("Log"), "the hidden Log has a tick");
    assert!(!painted.ticked("Reset Window Layout"));
    assert!(painted.lit("Log"), "Log is not lit");
    // A click on the row still shows the panel.
    let at = painted.text("Log").0.center();
    frame(&ctx, &mut shell, test_support::click(at));
    assert!(crate::layout::is_visible(&shell.dock, Pane::Log));
}

#[test]
fn a_hovered_tab_menu_row_takes_the_coral_deep_fill() {
    let ctx = themed();
    let mut shell = Shell::new();
    frame(&ctx, &mut shell, vec![]);
    frame(&ctx, &mut shell, vec![]);
    let leaf = leaf_rect(&shell, Pane::Preview);
    let tab = Pos2::new(leaf.min.x + 20.0, leaf.min.y + 10.0);
    let button = |pressed| Event::PointerButton {
        pos: tab,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(
        &ctx,
        &mut shell,
        vec![Event::PointerMoved(tab), button(true), button(false)],
    );
    let menu = frame(&ctx, &mut shell, vec![]);
    let at = menu.text("Hide panel").0.center();
    frame(&ctx, &mut shell, vec![Event::PointerMoved(at)]);
    let painted = frame(&ctx, &mut shell, vec![Event::PointerMoved(at)]);
    assert!(painted.lit("Hide panel"), "Hide panel is not lit");
    assert!(!painted.lit("Reset Window Layout"));
}
