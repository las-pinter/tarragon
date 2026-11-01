//! Tests for the tag filter.

use super::show_tags;
use crate::test_support::input;
use egui::{Context, Event, PointerButton, Pos2, epaint::Shape};
use tarragon_core::filter_bar::FilterBar;

struct Rig {
    ctx: Context,
    bar: FilterBar,
    names: Vec<String>,
    ticks: usize,
}

/// A text shape: its string, colour and position.
type Text = (String, egui::Color32, Pos2);

impl Rig {
    fn new() -> Self {
        let names = ["character", "lighting"].map(String::from).to_vec();
        Self {
            ctx: crate::test_support::context(),
            bar: FilterBar::default(),
            names,
            ticks: 0,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> (bool, Vec<Text>) {
        let mut changed = false;
        let out = self.ctx.run_ui(input(600.0, events), |ui| {
            changed = show_tags(ui, "tags", &mut self.bar, &self.names);
        });
        let texts = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(t) => Some((t.galley.text().to_owned(), t.fallback_color, t.pos)),
                _ => None,
            })
            .collect();
        let is_path = |s: &egui::epaint::ClippedShape| matches!(s.shape, Shape::Path(_));
        self.ticks = out.shapes.iter().filter(|s| is_path(s)).count();
        out.drop_without_applying_deltas();
        (changed, texts)
    }

    fn view(&mut self) -> Vec<Text> {
        self.frame(vec![]);
        self.frame(vec![]).1
    }

    /// Click at `pos` and return whether the filter changed.
    fn click(&mut self, pos: Pos2) -> bool {
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(pos), button(true)]);
        self.frame(vec![button(false)]).0
    }

    /// Click the text drawn in the button row (`in_bar`) or in the menu.
    fn click_text(&mut self, text: &str, in_bar: bool) -> bool {
        let view = self.view();
        let row = bar_row(&view);
        let found = view
            .into_iter()
            .find(|(s, _, at)| s == text && ((at.y - row).abs() < 6.0) == in_bar);
        let (_, _, at) = found.expect("text is drawn");
        self.click(at + egui::vec2(2.0, 2.0))
    }

    fn chips(&mut self) -> Vec<String> {
        let view = self.view();
        let row = bar_row(&view);
        let in_row = view.into_iter().filter(|t| (t.2.y - row).abs() < 6.0);
        in_row
            .map(|t| t.0)
            .filter(|s| s != "Filter Tags" && s != "x")
            .collect()
    }
}

/// The y of the button row.
fn bar_row(view: &[Text]) -> f32 {
    view.iter()
        .find(|t| t.0 == "Filter Tags")
        .map_or(0.0, |t| t.2.y)
}

fn open_and_tick(rig: &mut Rig, name: &str) -> bool {
    if !rig.view().iter().any(|t| t.0 == name) {
        rig.click_text("Filter Tags", true);
    }
    rig.click_text(name, false)
}

#[test]
fn chips_follow_tick_order() {
    let mut rig = Rig::new();
    open_and_tick(&mut rig, "lighting");
    open_and_tick(&mut rig, "character");
    assert_eq!(rig.chips(), ["lighting", "character"]);
}

#[test]
fn chip_x_unticks_the_row() {
    let mut rig = Rig::new();
    open_and_tick(&mut rig, "character");
    assert_eq!(rig.chips(), ["character"]);
    assert_eq!(rig.ticks, 1);
    assert!(rig.click_text("x", true));
    assert!(rig.bar.state.tags.is_empty());
    assert_eq!(rig.chips().len(), 0);
    rig.view();
    assert_eq!(rig.ticks, 0);
}

#[test]
fn opening_and_closing_the_menu_returns_false() {
    let mut rig = Rig::new();
    assert!(!rig.click_text("Filter Tags", true));
    assert!(rig.view().iter().any(|t| t.0 == "character"));
    assert!(!rig.click_text("Filter Tags", true));
    assert!(!rig.view().iter().any(|t| t.0 == "character"));
}

#[test]
fn unknown_tag_still_shows_its_chip() {
    let mut rig = Rig::new();
    rig.bar.toggle_tag("ghost");
    assert_eq!(rig.chips(), ["ghost"]);
}

#[test]
fn changes_return_true_and_idle_returns_false() {
    let mut rig = Rig::new();
    assert!(!rig.frame(vec![]).0);
    assert!(open_and_tick(&mut rig, "character"));
    assert!(!rig.frame(vec![]).0);
    assert!(rig.click_text("x", true));
}

#[test]
fn tag_count_counts_up() {
    let mut rig = Rig::new();
    let mut counts = vec![rig.bar.state.tags.len()];
    for name in ["character", "lighting"] {
        open_and_tick(&mut rig, name);
        counts.push(rig.bar.state.tags.len());
    }
    assert_eq!(counts, [0, 1, 2]);
}
