//! Tests for the filter bar row.

use std::path::Path;

use super::{GROUP_GAP, PAD_X, PAD_Y, SWATCH_LINE, show};
use crate::filter_bar::{CLEAR_TEXT, FOLDERS_TEXT, TAGS_TEXT};
use crate::swatch::FILTER_SIZE;
use crate::test_support::{click, context, input};
use crate::tokens;
use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Context, CornerRadius, Event, Pos2, Rect, vec2};
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_bar::{ColorChoice, FilterBar};
use tarragon_core::filter_state::FilterScope;

/// One bar under test, drawn in its own context.
struct Rig {
    ctx: Context,
    bar: FilterBar,
    scope: FilterScope,
    colors: bool,
    width: f32,
}

/// What one frame drew.
struct Drawn {
    changed: bool,
    /// The rect the bar took in its parent.
    used: Rect,
    shapes: Vec<Shape>,
}

impl Drawn {
    /// The painted texts with their rects, in paint order.
    fn texts(&self) -> Vec<(String, Rect)> {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text(t) => Some((
                    t.galley.text().to_owned(),
                    Rect::from_min_size(t.pos, t.galley.size()),
                )),
                _ => None,
            })
            .collect()
    }

    fn text(&self, name: &str) -> Option<Rect> {
        self.texts().into_iter().find(|t| t.0 == name).map(|t| t.1)
    }

    fn has(&self, name: &str) -> bool {
        self.text(name).is_some()
    }

    /// The chip names in reading order: texts that are not an action.
    fn chips(&self) -> Vec<String> {
        let actions = [TAGS_TEXT, FOLDERS_TEXT, CLEAR_TEXT];
        let mut found: Vec<_> = self
            .texts()
            .into_iter()
            .filter(|t| !actions.contains(&t.0.as_str()))
            .collect();
        found.sort_by(|a, b| {
            (a.1.min.y.round(), a.1.min.x)
                .partial_cmp(&(b.1.min.y.round(), b.1.min.x))
                .expect("no NaN")
        });
        found.into_iter().map(|t| t.0).collect()
    }

    /// Filled circles: centre and fill.
    fn dots(&self) -> Vec<(Pos2, f32, Color32)> {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Circle(c) if c.fill != Color32::TRANSPARENT => {
                    Some((c.center, c.radius, c.fill))
                }
                _ => None,
            })
            .collect()
    }
}

fn flat(shapes: &[ClippedShape]) -> Vec<Shape> {
    fn collect(shape: &Shape, found: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|s| collect(s, found)),
            other => found.push(other.clone()),
        }
    }
    let mut found = Vec::new();
    for c in shapes {
        collect(&c.shape, &mut found);
    }
    found
}

impl Rig {
    fn new() -> Self {
        Self {
            ctx: context(),
            bar: FilterBar::default(),
            scope: FilterScope::Global,
            colors: true,
            width: 800.0,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Drawn {
        let mut raw = input(self.width, events);
        raw.time = Some(f64::from(u32::try_from(self.ctx.cumulative_pass_nr()).unwrap_or(0)) * 0.5);
        let tags = ["character", "lighting"].map(String::from);
        let folders = ["/home/u/sketches"].map(String::from);
        let mut seen = None;
        let output = self.ctx.run_ui(raw, |ui| {
            ui.style_mut().interaction.tooltip_delay = 0.0;
            let top = ui.cursor().min;
            let shown = show(ui, &mut self.bar, &tags, &folders, self.scope, self.colors);
            let used = Rect::from_min_max(top, ui.min_rect().max);
            seen = Some((shown.changed, used));
        });
        let shapes = flat(&output.shapes);
        output.drop_without_applying_deltas();
        let (changed, used) = seen.expect("the closure ran");
        Drawn {
            changed,
            used,
            shapes,
        }
    }

    /// Two idle frames, so the layout has settled.
    fn view(&mut self) -> Drawn {
        self.frame(vec![]);
        self.frame(vec![])
    }

    /// Click at `at` and return the frame that saw the click.
    fn click(&mut self, at: Pos2) -> Drawn {
        self.frame(click(at))
    }

    /// Click the centre of the text `name`.
    fn click_text(&mut self, name: &str) -> Drawn {
        let at = self.view().text(name).expect(name).center();
        self.click(at)
    }
}

fn held_bar() -> FilterBar {
    let mut bar = FilterBar::default();
    bar.toggle_tag("lighting");
    bar.toggle_folder(Path::new("/home/u/sketches"));
    bar.toggle_color(ColorChoice::Transparent);
    bar.toggle_tag("character");
    bar.toggle_color(ColorChoice::Bucket(ColorBucket::Blue));
    bar
}

#[test]
fn the_bar_is_a_rounded_panel_with_the_page_padding() {
    let mut rig = Rig::new();
    let drawn = rig.view();
    let panel = drawn.shapes.iter().find_map(|s| match s {
        Shape::Rect(r) if r.fill == tokens::BG_SECONDARY => Some((r.rect, r.corner_radius)),
        _ => None,
    });
    let (rect, radius) = panel.expect("the panel fill is painted");
    assert_eq!(radius, CornerRadius::same(tokens::RADIUS_XL));
    assert_eq!(rect, drawn.used, "the panel spans the bar");
    let (center, ..) = drawn.dots()[0];
    let swatch = Rect::from_center_size(center, vec2(FILTER_SIZE, FILTER_SIZE));
    assert_eq!(swatch.min.x - rect.min.x, f32::from(PAD_X));
    // The first line is an action tall, and the swatch sits in its middle.
    let line = super::ACTION_HEIGHT;
    assert_eq!(
        swatch.center().y - rect.min.y,
        f32::from(PAD_Y) + line / 2.0
    );
    assert_eq!(rect.height(), 2.0 * f32::from(PAD_Y) + line, "one line");
}

#[test]
fn the_actions_sit_at_the_right_end_of_the_first_line() {
    let mut rig = Rig::new();
    rig.bar = held_bar();
    let drawn = rig.view();
    let (tags, folders, clear) = (
        drawn.text(TAGS_TEXT).expect("Tags"),
        drawn.text(FOLDERS_TEXT).expect("Folders"),
        drawn.text(CLEAR_TEXT).expect("Clear"),
    );
    assert!(tags.max.x < folders.min.x && folders.max.x < clear.min.x);
    let first = drawn.dots()[0].0.y;
    for action in [tags, folders, clear] {
        assert!((action.center().y - first).abs() < 1.0, "{action:?}");
    }
    let end = drawn.used.max.x - f32::from(PAD_X);
    assert!(
        (end - clear.max.x - 4.0).abs() < 0.01,
        "Clear ends at the padding"
    );
}

#[test]
fn folders_shows_in_all_images_and_clear_only_while_a_filter_is_held() {
    let mut rig = Rig::new();
    let drawn = rig.view();
    assert!(drawn.has(TAGS_TEXT) && drawn.has(FOLDERS_TEXT));
    assert!(!drawn.has(CLEAR_TEXT));
    rig.scope = FilterScope::Folder;
    assert!(!rig.view().has(FOLDERS_TEXT));
    rig.bar.toggle_tag("character");
    assert!(rig.view().has(CLEAR_TEXT));
}

#[test]
fn chips_list_colours_then_tags_then_folders() {
    let mut rig = Rig::new();
    rig.bar = held_bar();
    assert_eq!(
        rig.view().chips(),
        ["Blue", "Transparent", "lighting", "character", "u/sketches"]
    );
}

#[test]
fn tag_dots_are_coral_and_folder_dots_amber() {
    let mut rig = Rig::new();
    rig.colors = false;
    rig.bar.toggle_tag("character");
    rig.bar.toggle_folder(Path::new("/home/u/sketches"));
    let drawn = rig.view();
    let fills: Vec<_> = drawn.dots().into_iter().map(|d| d.2).collect();
    assert_eq!(fills, [tokens::CORAL_STRONG, tokens::AMBER_ACCENT]);
}

#[test]
fn the_divider_shows_between_swatches_and_chips_only() {
    let divider = |drawn: &Drawn| {
        drawn.shapes.iter().any(|s| {
            matches!(s, Shape::Rect(r) if r.fill == tokens::SURFACE_HIGHLIGHT
                && r.rect.size() == vec2(1.0, 24.0))
        })
    };
    let mut rig = Rig::new();
    assert!(!divider(&rig.view()), "no chips, no divider");
    rig.bar.toggle_tag("character");
    let drawn = rig.view();
    assert!(divider(&drawn));
    rig.colors = false;
    assert!(!divider(&rig.view()), "no swatches, no divider");
}

#[test]
fn a_chip_x_drops_its_filter() {
    let mut rig = Rig::new();
    rig.bar = held_bar();
    for (name, gone) in [
        ("Blue", 4),
        ("lighting", 3),
        ("u/sketches", 2),
        ("Transparent", 1),
        ("character", 0),
    ] {
        let drawn = rig.view();
        let chip = drawn.text(name).expect(name);
        // The x sits one gap right of the name, 20 points wide.
        let x = Pos2::new(chip.max.x + 6.0 + 10.0, chip.center().y);
        assert!(rig.click(x).changed, "{name}");
        assert!(!rig.view().has(name), "{name}");
        assert_eq!(rig.view().chips().len(), gone);
    }
    assert!(!rig.bar.holds_any());
}

#[test]
fn clear_drops_every_filter_and_keeps_the_search_text() {
    let mut rig = Rig::new();
    rig.bar = held_bar();
    rig.bar.state.text = "cat".to_owned();
    assert!(rig.click_text(CLEAR_TEXT).changed);
    assert!(!rig.bar.holds_any());
    assert_eq!(rig.bar.state.text, "cat");
    let drawn = rig.view();
    assert!(drawn.chips().is_empty() && !drawn.has(CLEAR_TEXT));
}

#[test]
fn a_swatch_click_adds_a_chip() {
    let mut rig = Rig::new();
    let red = rig.view().dots()[0].0;
    assert!(rig.click(red).changed);
    assert_eq!(rig.view().chips(), ["Red"]);
}

#[test]
fn folder_chips_show_in_the_folder_scope_with_the_full_path_tooltip() {
    let mut rig = Rig::new();
    rig.scope = FilterScope::Folder;
    rig.bar.toggle_folder(Path::new("/home/u/photos/vacation"));
    rig.bar.toggle_folder(Path::new("/home/v/photos/vacation"));
    let drawn = rig.view();
    let same = drawn
        .chips()
        .iter()
        .filter(|c| *c == "photos/vacation")
        .count();
    assert_eq!(same, 2, "equal short names stay two chips");
    // The second chip in reading order, which may have wrapped.
    let second = drawn
        .texts()
        .into_iter()
        .filter(|t| t.0 == "photos/vacation")
        .map(|t| t.1)
        .max_by(|a, b| {
            (a.min.y, a.min.x)
                .partial_cmp(&(b.min.y, b.min.x))
                .expect("no NaN")
        })
        .expect("the chip is drawn");
    rig.frame(vec![Event::PointerMoved(second.center())]);
    let held: Vec<_> = (0..4).flat_map(|_| rig.frame(vec![]).texts()).collect();
    assert!(held.iter().any(|t| t.0 == "/home/v/photos/vacation"));
    assert!(!held.iter().any(|t| t.0 == "/home/u/photos/vacation"));
}

#[test]
fn an_unknown_tag_still_shows_its_chip() {
    let mut rig = Rig::new();
    rig.bar.toggle_tag("ghost");
    assert_eq!(rig.view().chips(), ["ghost"]);
}

#[test]
fn the_folder_menu_is_closed_after_a_scope_round_trip() {
    let mut rig = Rig::new();
    rig.click_text(FOLDERS_TEXT);
    assert!(rig.view().has("u/sketches"), "the menu row shows");
    rig.scope = FilterScope::Folder;
    rig.view();
    rig.scope = FilterScope::Global;
    assert!(!rig.view().has("u/sketches"));
}

#[test]
fn a_narrow_bar_puts_the_actions_on_a_line_below() {
    let mut rig = Rig::new();
    rig.width = SWATCH_LINE + 2.0 * f32::from(PAD_X) + GROUP_GAP + 20.0;
    let drawn = rig.view();
    let tags = drawn.text(TAGS_TEXT).expect("Tags");
    let lowest = drawn.dots().iter().map(|d| d.0.y).fold(f32::MIN, f32::max);
    assert!(tags.min.y > lowest, "Tags sits below the swatches");
    let folders = drawn.text(FOLDERS_TEXT).expect("Folders");
    let end = drawn.used.max.x - f32::from(PAD_X);
    assert!(
        (end - folders.max.x - 4.0).abs() < 0.01,
        "the line ends at the padding"
    );
    assert!(drawn.used.width() <= rig.width);
}

#[test]
fn the_swatches_wrap_inside_a_narrow_bar() {
    let mut rig = Rig::new();
    rig.width = 260.0;
    let drawn = rig.view();
    for (center, radius, _) in drawn.dots() {
        assert!(center.x + radius <= drawn.used.max.x - f32::from(PAD_X) + 0.01);
    }
    let dots = drawn.dots();
    let first = dots[0].0.y;
    assert!(
        dots.iter().any(|d| d.0.y > first + FILTER_SIZE),
        "the swatches take more than one line"
    );
}

#[test]
fn shown_ors_the_manage_tags_flag() {
    let mut shown = super::Shown::default();
    shown |= super::Shown {
        manage_tags: true,
        ..super::Shown::default()
    };
    shown |= super::Shown::default();
    assert!(shown.manage_tags);
    assert!(!shown.changed && !shown.opened);
}
