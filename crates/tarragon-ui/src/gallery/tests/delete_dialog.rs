//! Tests for the Delete dialogs.

use super::context_menu::{key, menu_shows, press_at, right_click};
use super::rename_dialog::{click_button, has_text, texts};
use super::*;
use crate::test_support;
use egui::{Context, Key};

/// The `/images` paths of `names`.
fn paths(names: &[&str]) -> Vec<PathBuf> {
    names
        .iter()
        .map(|name| PathBuf::from("/images").join(name))
        .collect()
}

/// A way to open one kind of dialog.
type Opener = fn(&mut Gallery, Vec<PathBuf>);

/// Open the Trash confirm.
fn trash(gallery: &mut Gallery, paths: Vec<PathBuf>) {
    gallery.open_delete(paths, false);
}

/// Open the permanent confirm.
fn permanent(gallery: &mut Gallery, paths: Vec<PathBuf>) {
    gallery.open_delete(paths, true);
}

/// Open the "Delete permanently instead?" prompt.
fn instead(gallery: &mut Gallery, paths: Vec<PathBuf>) {
    gallery.open_delete_instead(paths);
}

/// The three kinds.
const KINDS: [Opener; 3] = [trash, permanent, instead];

/// A gallery over `/images` with a dialog opened by `opener`, not yet drawn.
fn start(names: &[&str], opener: Opener) -> (Context, Gallery) {
    let ctx = test_support::context();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    opener(&mut gallery, paths(names));
    (ctx, gallery)
}

/// Like [`start`], with the focus settled.
fn open(names: &[&str], opener: Opener) -> (Context, Gallery) {
    let (ctx, mut gallery) = start(names, opener);
    for _ in 0..3 {
        texts(&ctx, &mut gallery, Vec::new());
    }
    (ctx, gallery)
}

/// A key event: a press, a repeat, or a release.
fn key_event(key: Key, pressed: bool, repeat: bool) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat,
        modifiers: Modifiers::NONE,
    }
}

/// Press a key, then let go.
fn tap(ctx: &Context, gallery: &mut Gallery, key: Key) {
    texts(ctx, gallery, vec![key_event(key, true, false)]);
    texts(ctx, gallery, vec![key_event(key, false, false)]);
    texts(ctx, gallery, Vec::new());
}

#[test]
fn the_trash_confirm_shows_the_count_and_enter_confirms_without_permanent() {
    let (ctx, mut gallery) = open(&["a.png", "b.png", "c.png"], trash);
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, "Move 3 items to the Trash?"));
    assert!(has_text(&seen, "Move to Trash"));
    assert!(has_text(&seen, "Cancel"));

    tap(&ctx, &mut gallery, Key::Enter);
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::ConfirmedDelete {
            paths: paths(&["a.png", "b.png", "c.png"]),
            permanent: false,
        }]
    );
    assert!(!gallery.delete_open(), "the dialog closed");
}

#[test]
fn the_trash_confirm_names_a_single_file() {
    let (ctx, mut gallery) = open(&["a.png"], trash);
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(&seen, "Move 'a.png' to the Trash?"));
}

#[test]
fn the_move_to_trash_button_confirms_without_permanent() {
    let (ctx, mut gallery) = open(&["a.png"], trash);
    click_button(&ctx, &mut gallery, "Move to Trash");
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::ConfirmedDelete {
            paths: paths(&["a.png"]),
            permanent: false,
        }]
    );
}

#[test]
fn the_permanent_confirm_shows_the_warning_and_enter_closes_with_no_event() {
    let (ctx, mut gallery) = open(&["a.png", "b.png"], permanent);
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(
        &seen,
        "Delete 2 items permanently? This cannot be undone."
    ));

    tap(&ctx, &mut gallery, Key::Enter);
    assert_eq!(gallery.take_menu_events(), vec![]);
    assert!(!gallery.delete_open(), "Enter closed the dialog");
}

#[test]
fn a_click_on_delete_permanently_sends_the_permanent_event() {
    let (ctx, mut gallery) = open(&["a.png", "b.png"], permanent);
    click_button(&ctx, &mut gallery, "Delete permanently");
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::ConfirmedDelete {
            paths: paths(&["a.png", "b.png"]),
            permanent: true,
        }]
    );
    assert!(!gallery.delete_open(), "the dialog closed");
}

#[test]
fn focus_starts_on_cancel_so_space_never_deletes_permanently() {
    for opener in [permanent, instead] {
        let (ctx, mut gallery) = open(&["a.png"], opener);
        tap(&ctx, &mut gallery, Key::Space);
        assert_eq!(gallery.take_menu_events(), vec![]);
        assert!(!gallery.delete_open(), "Space pressed Cancel");
    }
}

#[test]
fn tab_then_space_on_delete_permanently_sends_the_permanent_event() {
    for opener in [permanent, instead] {
        let (ctx, mut gallery) = open(&["a.png"], opener);
        tap(&ctx, &mut gallery, Key::Tab);
        assert!(gallery.delete_open(), "Tab did not close the dialog");
        assert_eq!(gallery.take_menu_events(), vec![]);
        tap(&ctx, &mut gallery, Key::Space);
        assert_eq!(
            gallery.take_menu_events(),
            vec![GridMenuEvent::ConfirmedDelete {
                paths: paths(&["a.png"]),
                permanent: true,
            }]
        );
    }
}

#[test]
fn a_modified_enter_on_delete_permanently_sends_no_event() {
    for opener in [permanent, instead] {
        let (ctx, mut gallery) = open(&["a.png"], opener);
        tap(&ctx, &mut gallery, Key::Tab);
        for modifiers in [Modifiers::SHIFT, Modifiers::CTRL, Modifiers::ALT] {
            let enter = Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: true,
                repeat: false,
                modifiers,
            };
            texts(&ctx, &mut gallery, vec![enter]);
            // Let go, or egui reads the next press as a repeat.
            texts(
                &ctx,
                &mut gallery,
                vec![key_event(Key::Enter, false, false)],
            );
            assert_eq!(gallery.take_menu_events(), vec![], "{modifiers:?}");
            assert!(!gallery.delete_open(), "{modifiers:?} left the dialog open");
            opener(&mut gallery, paths(&["a.png"]));
            for _ in 0..3 {
                texts(&ctx, &mut gallery, Vec::new());
            }
            tap(&ctx, &mut gallery, Key::Tab);
        }
    }
}

#[test]
fn the_instead_prompt_shows_the_failed_count_and_enter_closes_with_no_event() {
    let (ctx, mut gallery) = open(&["a.png", "b.png"], instead);
    let seen = texts(&ctx, &mut gallery, Vec::new());
    assert!(has_text(
        &seen,
        "2 items could not be moved to the Trash. Delete permanently instead? \
         This cannot be undone."
    ));
    assert!(has_text(&seen, "Cancel"));

    tap(&ctx, &mut gallery, Key::Enter);
    assert_eq!(gallery.take_menu_events(), vec![]);
    assert!(!gallery.delete_open(), "Enter closed the dialog");
}

#[test]
fn the_instead_prompt_sends_permanent_only_from_its_button() {
    let (ctx, mut gallery) = open(&["a.png"], instead);
    click_button(&ctx, &mut gallery, "Delete permanently");
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::ConfirmedDelete {
            paths: paths(&["a.png"]),
            permanent: true,
        }]
    );
}

#[test]
fn cancel_closes_each_dialog_with_no_event() {
    for opener in KINDS {
        let (ctx, mut gallery) = open(&["a.png"], opener);
        click_button(&ctx, &mut gallery, "Cancel");
        assert!(!gallery.delete_open(), "the dialog closed");
        assert_eq!(gallery.take_menu_events(), vec![]);
    }
}

#[test]
fn escape_closes_each_dialog_with_no_event_and_is_consumed() {
    for opener in KINDS {
        let (ctx, mut gallery) = open(&["a.png"], opener);
        let input = test_support::input(900.0, vec![key(Key::Escape)]);
        let mut seen_later = true;
        ctx.run_ui(input, |ui| {
            gallery.show(ui);
            seen_later = ui.input(|input| input.key_pressed(Key::Escape));
        })
        .drop_without_applying_deltas();
        assert!(!seen_later, "Escape was left for another handler");
        assert!(!gallery.delete_open(), "the dialog closed");
        assert_eq!(gallery.take_menu_events(), vec![]);
    }
}

#[test]
fn the_key_that_opened_the_dialog_does_not_confirm_it() {
    // egui reads a repeat of a key that is not down as a new press. So Enter
    // goes down first, with no dialog open, and the repeats come after.
    let (ctx, mut gallery) = start(&[], trash);
    texts(&ctx, &mut gallery, vec![key_event(Key::Enter, true, false)]);
    gallery.open_delete(paths(&["a.png"]), false);
    for _ in 0..3 {
        texts(&ctx, &mut gallery, Vec::new());
    }
    // A held Delete repeats, then lets go. A held Enter repeats the same way.
    let frames = [
        vec![key_event(Key::Enter, true, true)],
        vec![key_event(Key::Delete, true, false)],
        vec![key_event(Key::Delete, true, true)],
        vec![key_event(Key::Delete, false, false)],
        vec![key_event(Key::Enter, true, true)],
        vec![key_event(Key::Enter, false, false)],
    ];
    for events in frames {
        texts(&ctx, &mut gallery, events);
        assert_eq!(gallery.take_menu_events(), vec![]);
        assert!(gallery.delete_open(), "the dialog stayed open");
    }
}

#[test]
fn enter_before_the_default_button_has_focus_does_not_confirm() {
    let (ctx, mut gallery) = start(&["a.png"], trash);
    texts(&ctx, &mut gallery, vec![key(Key::Enter)]);
    for _ in 0..3 {
        texts(&ctx, &mut gallery, Vec::new());
    }
    assert_eq!(gallery.take_menu_events(), vec![]);
    assert!(gallery.delete_open(), "the dialog stayed open");
}

#[test]
fn the_dialog_acts_on_the_files_it_opened_with() {
    let (ctx, mut gallery) = open(&["b.png"], trash);
    gallery.select_all_visible();
    tap(&ctx, &mut gallery, Key::Enter);
    assert_eq!(
        gallery.take_menu_events(),
        vec![GridMenuEvent::ConfirmedDelete {
            paths: paths(&["b.png"]),
            permanent: false,
        }]
    );
}

#[test]
fn a_second_open_does_not_replace_or_stack_on_the_first() {
    for second in KINDS {
        let (ctx, mut gallery) = open(&["a.png"], trash);
        second(&mut gallery, paths(&["b.png"]));
        tap(&ctx, &mut gallery, Key::Enter);
        assert_eq!(
            gallery.take_menu_events(),
            vec![GridMenuEvent::ConfirmedDelete {
                paths: paths(&["a.png"]),
                permanent: false,
            }]
        );
        assert!(!gallery.delete_open(), "no second dialog waited");
    }
}

#[test]
fn an_empty_list_opens_nothing() {
    let (_ctx, gallery) = start(&[], trash);
    assert!(!gallery.delete_open());
}

#[test]
fn opening_the_dialog_closes_an_open_tile_menu() {
    let (ctx, mut gallery) = super::context_menu::setup();
    let (_, seen) = right_click(&ctx, &mut gallery, 1);
    assert!(menu_shows(&seen), "the menu opened");

    gallery.open_delete(paths(&["b.png"]), false);
    for _ in 0..2 {
        super::context_menu::run(&ctx, &mut gallery, Vec::new());
    }
    gallery.delete = None;
    let seen = super::context_menu::run(&ctx, &mut gallery, Vec::new());
    assert!(!menu_shows(&seen), "the menu stayed open");
}

#[test]
fn a_right_click_does_not_open_a_menu_while_a_dialog_is_open() {
    let kinds = [
        (trash as Opener, "Delete"),
        (permanent, "Delete"),
        (instead, "Delete permanently instead?"),
    ];
    for (opener, title) in kinds {
        let (ctx, mut gallery) = super::context_menu::setup();
        opener(&mut gallery, paths(&["b.png"]));
        for _ in 0..2 {
            super::context_menu::run(&ctx, &mut gallery, Vec::new());
        }
        // Tile 0 lies outside the dialog window, so only the dim layer can
        // keep the click from it.
        let pos = super::context_menu::tile_center(&ctx, 0);
        let window = ctx
            // egui keys a window by the optional text of its title.
            .memory(|memory| memory.area_rect(Id::new(Some(title))))
            .expect("the dialog window was drawn");
        assert!(!window.contains(pos), "the tile is under the window");
        let seen = press_at(&ctx, &mut gallery, pos, PointerButton::Secondary);
        assert!(!menu_shows(&seen), "a menu opened over the dialog");
        gallery.delete = None;
        let seen = super::context_menu::run(&ctx, &mut gallery, Vec::new());
        assert!(!menu_shows(&seen), "a menu waited under the dialog");
    }
}
