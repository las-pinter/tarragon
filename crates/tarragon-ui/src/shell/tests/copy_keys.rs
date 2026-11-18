//! Tests for Ctrl+C in the shape egui-winit sends it: a copy event with no
//! modifiers, and Ctrl that may go up in the same frame. Also the Ctrl+Shift+V
//! chord with Ctrl up first.

use super::grid_keys::{mods, path, run_raw, v_release};
use super::keys::{ctrl, frame, frame_at, press, search_id, select_one, setup};
use crate::gallery::GridMenuEvent;
use egui::{Event, Modifiers};

fn copy_b() -> GridMenuEvent {
    GridMenuEvent::Copy(vec![path("b.png")])
}

#[test]
fn copy_fires_when_ctrl_goes_up_in_the_frame_of_the_copy_event() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    let events = vec![mods(ctrl()), Event::Copy, mods(Modifiers::NONE)];
    let queued = run_raw(&ctx, &mut shell, vec![events]);
    assert_eq!(queued, vec![vec![copy_b()]]);
}

#[test]
fn copy_fires_when_ctrl_goes_up_in_the_next_frame() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    let frames = vec![vec![mods(ctrl()), Event::Copy], vec![mods(Modifiers::NONE)]];
    let queued = run_raw(&ctx, &mut shell, frames);
    assert_eq!(queued, vec![vec![copy_b()], vec![]]);
}

#[test]
fn copy_fires_when_ctrl_was_down_for_a_long_idle_time() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    frame_at(&ctx, &mut shell, vec![mods(ctrl())], Some(10.0));
    assert_eq!(shell.gallery_mut().take_menu_events(), vec![]);
    // No frame ran for 5 s. The copy and the Ctrl release share one frame.
    let events = vec![Event::Copy, mods(Modifiers::NONE)];
    frame_at(&ctx, &mut shell, events, Some(15.0));
    assert_eq!(shell.gallery_mut().take_menu_events(), vec![copy_b()]);
}

#[test]
fn copy_does_nothing_for_the_ctrl_shift_and_ctrl_alt_shapes() {
    for extra in [Modifiers::SHIFT, Modifiers::ALT] {
        let (ctx, mut shell) = setup();
        select_one(&ctx, &mut shell, 1);
        let chord = ctrl() | extra;
        let one_frame = vec![
            mods(ctrl()),
            mods(chord),
            Event::Copy,
            mods(extra),
            mods(Modifiers::NONE),
        ];
        let queued = run_raw(&ctx, &mut shell, vec![one_frame]);
        assert_eq!(queued, vec![vec![]], "{extra:?} in one frame");

        // The extra key goes up first, then Ctrl, all in the frame of the copy.
        let ctrl_last = vec![
            mods(chord),
            Event::Copy,
            mods(ctrl()),
            mods(Modifiers::NONE),
        ];
        let queued = run_raw(&ctx, &mut shell, vec![ctrl_last]);
        assert_eq!(queued, vec![vec![]], "{extra:?}, Ctrl last");

        let two_frames = vec![vec![mods(chord)], vec![Event::Copy, mods(Modifiers::NONE)]];
        let queued = run_raw(&ctx, &mut shell, two_frames);
        assert_eq!(queued, vec![vec![], vec![]], "{extra:?} in two frames");
    }
}

#[test]
fn a_lone_copy_event_with_no_ctrl_history_copies_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    let queued = run_raw(&ctx, &mut shell, vec![vec![Event::Copy]]);
    assert_eq!(queued, vec![vec![]]);
}

#[test]
fn a_lone_copy_event_soon_after_ctrl_went_up_copies_nothing() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    frame_at(&ctx, &mut shell, vec![mods(ctrl())], Some(10.0));
    frame_at(&ctx, &mut shell, vec![mods(Modifiers::NONE)], Some(10.1));
    frame_at(&ctx, &mut shell, vec![Event::Copy], Some(10.3));
    assert_eq!(shell.gallery_mut().take_menu_events(), vec![]);
}

#[test]
fn copy_does_nothing_while_a_text_field_has_focus() {
    let (ctx, mut shell) = setup();
    select_one(&ctx, &mut shell, 1);
    frame(&ctx, &mut shell, vec![press(egui::Key::F, ctrl())]);
    frame(&ctx, &mut shell, vec![]);
    assert!(ctx.memory(|m| m.has_focus(search_id())));

    let events = vec![mods(ctrl()), Event::Copy, mods(Modifiers::NONE)];
    let queued = run_raw(&ctx, &mut shell, vec![events]);
    assert_eq!(queued, vec![vec![]]);
}

#[test]
fn ctrl_shift_v_with_ctrl_up_first_pastes_nothing() {
    let (ctx, mut shell) = setup();
    let chord = ctrl() | Modifiers::SHIFT;
    let first = vec![mods(ctrl()), mods(chord), v_release(chord)];
    // Ctrl goes up before Shift, so the last V release carries nothing.
    let second = vec![
        mods(Modifiers::SHIFT),
        mods(Modifiers::NONE),
        v_release(Modifiers::NONE),
    ];
    let queued = run_raw(
        &ctx,
        &mut shell,
        vec![[first.clone(), second.clone()].concat()],
    );
    assert_eq!(queued, vec![vec![]], "in one frame");

    let queued = run_raw(&ctx, &mut shell, vec![first, second]);
    assert_eq!(queued, vec![vec![], vec![]], "in two frames");
}
