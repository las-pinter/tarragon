//! Tests for the folder filter.

use std::path::Path;

use super::show_folders;
use crate::test_support::input;
use egui::{Context, Event, PointerButton, Pos2, epaint::Shape};
use tarragon_core::filter_bar::FilterBar;
use tarragon_core::filter_state::FilterScope;

/// A text shape: its string and position.
type Text = (String, Pos2);

struct Rig {
    ctx: Context,
    bar: FilterBar,
    folders: Vec<String>,
    scope: FilterScope,
    ticks: usize,
}

fn collect(shape: &Shape, found: &mut Vec<Text>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| collect(s, found)),
        Shape::Text(t) => found.push((t.galley.text().to_owned(), t.pos)),
        _ => {}
    }
}

impl Rig {
    fn new(active: &[&str]) -> Self {
        let mut bar = FilterBar::default();
        for p in active {
            bar.toggle_folder(Path::new(p));
        }
        Self {
            ctx: crate::test_support::context(),
            bar,
            folders: ["/p/one/a", "/q/two/b"].map(String::from).to_vec(),
            scope: FilterScope::Global,
            ticks: 0,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> (bool, Vec<Text>) {
        let mut changed = false;
        let mut raw = input(600.0, events);
        raw.time = Some(f64::from(u32::try_from(self.ctx.cumulative_pass_nr()).unwrap_or(0)) * 0.5);
        let out = self.ctx.run_ui(raw, |ui| {
            ui.style_mut().interaction.tooltip_delay = 0.0;
            changed = show_folders(ui, "folders", &mut self.bar, &self.folders, self.scope);
        });
        let mut texts = Vec::new();
        for clipped in &out.shapes {
            collect(&clipped.shape, &mut texts);
        }
        let is_path = |s: &egui::epaint::ClippedShape| matches!(s.shape, Shape::Path(_));
        self.ticks = out.shapes.iter().filter(|s| is_path(s)).count();
        out.drop_without_applying_deltas();
        (changed, texts)
    }

    fn view(&mut self) -> Vec<Text> {
        self.frame(vec![]);
        self.frame(vec![]).1
    }

    fn names(&mut self) -> Vec<String> {
        self.view().into_iter().map(|t| t.0).collect()
    }

    fn pos(&mut self, text: &str, nth: usize) -> Pos2 {
        let view = self.view();
        let found = view.iter().filter(|t| t.0 == text).nth(nth);
        found.map(|t| t.1).expect("the text is drawn") + egui::vec2(2.0, 2.0)
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

    fn click_text(&mut self, text: &str) -> bool {
        let at = self.pos(text, 0);
        self.click(at)
    }
}

#[test]
fn the_button_shows_in_global_scope_only() {
    let mut rig = Rig::new(&["/p/one/a"]);
    assert!(rig.names().contains(&"Filter Folders".into()));
    rig.scope = FilterScope::Folder;
    let names = rig.names();
    assert!(!names.contains(&"Filter Folders".into()));
    assert!(names.contains(&"one/a".into()));
}

#[test]
fn chips_follow_activation_order() {
    let mut rig = Rig::new(&["/z/last/x", "/a/first/y", "/p/two/a"]);
    let names = rig.names();
    let at = |s: &str| names.iter().position(|t| t == s);
    let (a, b, c) = (at("last/x"), at("first/y"), at("two/a"));
    assert!(a.is_some() && b.is_some() && c.is_some());
    assert!(a < b && b < c);
}

#[test]
fn equal_short_names_stay_two_chips() {
    let mut rig = Rig::new(&["/p/one/a", "/q/one/a"]);
    let count = rig.names().iter().filter(|t| *t == "one/a").count();
    assert_eq!(count, 2);
}

#[test]
fn the_chip_tooltip_holds_the_full_path() {
    let (first, second) = ("/home/u/photos/vacation", "/home/u/photos/travel");
    let mut rig = Rig::new(&[first, second]);
    rig.scope = FilterScope::Folder;
    let at = rig.pos("photos/travel", 0);
    rig.frame(vec![Event::PointerMoved(at)]);
    let held: Vec<_> = (0..4).flat_map(|_| rig.frame(vec![]).1).collect();
    assert!(held.iter().any(|t| t.0 == second));
    assert!(!held.iter().any(|t| t.0 == first));
}

/// The y of the one tick drawn in the open menu, and the y of each row text.
fn tick_and_rows(rig: &mut Rig) -> (f32, Vec<(String, f32)>) {
    rig.click_text("Filter Folders");
    rig.view();
    let mut texts = Vec::new();
    let out = rig.ctx.run_ui(input(600.0, vec![]), |ui| {
        show_folders(ui, "folders", &mut rig.bar, &rig.folders, rig.scope);
    });
    let mut ticks = Vec::new();
    for clipped in &out.shapes {
        collect(&clipped.shape, &mut texts);
        if let Shape::Path(p) = &clipped.shape {
            ticks.extend(p.points.iter().map(|pt| pt.y));
        }
    }
    out.drop_without_applying_deltas();
    let tick = ticks.iter().copied().fold(f32::MAX, f32::min);
    (tick, texts.into_iter().map(|(s, p)| (s, p.y)).collect())
}

#[test]
fn a_ticked_row_matches_the_bar() {
    for (active, ticked, other) in [
        ("/q/two/b", "two/b", "one/a"),
        ("/p/one/a", "one/a", "two/b"),
    ] {
        let mut rig = Rig::new(&[active]);
        let (tick, rows) = tick_and_rows(&mut rig);
        // The menu rows sit below the chip row, so take the lowest match.
        let row = |name: &str| {
            rows.iter()
                .filter(|r| r.0 == name)
                .map(|r| r.1)
                .fold(0.0, f32::max)
        };
        assert!((tick - row(ticked)).abs() < 10.0, "tick on {ticked}");
        assert!((tick - row(other)).abs() > 10.0, "no tick on {other}");
    }
}

#[test]
fn a_row_click_toggles_the_folder() {
    let mut rig = Rig::new(&[]);
    rig.click_text("Filter Folders");
    assert!(rig.click_text("one/a"));
    assert!(rig.bar.state.folders.contains(Path::new("/p/one/a")));
    assert!(!rig.frame(vec![]).0);
}

#[test]
fn the_chip_x_removes_the_folder() {
    let mut rig = Rig::new(&["/p/one/a"]);
    assert!(rig.click_text("x"));
    assert!(rig.bar.state.folders.is_empty());
    assert_eq!(rig.bar.folder_order(), [] as [&Path; 0]);
}

#[test]
fn the_menu_is_closed_after_a_scope_round_trip() {
    let mut rig = Rig::new(&[]);
    rig.click_text("Filter Folders");
    assert!(rig.names().contains(&"one/a".into()));
    rig.scope = FilterScope::Folder;
    rig.view();
    rig.scope = FilterScope::Global;
    assert!(!rig.names().contains(&"one/a".into()));
}
