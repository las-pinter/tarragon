//! Keyboard shortcuts that do what a menu item or a gallery hook does.

use egui::{Context, Event, Key, Modifiers};

use super::{MenuEvent, Shell};
use crate::gallery::Gallery;

/// Take one press of `key` held with exactly `modifiers`.
///
/// `consume_shortcut` accepts extra Shift and Alt, so Ctrl+Shift+O would open
/// the picker. The press is checked with `matches_exact` first.
fn take_exact_press(ctx: &Context, key: Key, modifiers: Modifiers) -> bool {
    ctx.input_mut(|input| {
        let exact = input.events.iter().any(|event| {
            matches!(
                event,
                Event::Key { key: pressed, modifiers: held, pressed: true, .. }
                    if *pressed == key && held.matches_exact(modifiers)
            )
        });
        exact && input.consume_key(modifiers, key)
    })
}

/// Take one new press of `key` held with exactly `modifiers`. A repeat of a
/// held key is neither taken nor counted, so a held key acts once.
fn take_fresh_press(ctx: &Context, key: Key, modifiers: Modifiers) -> bool {
    let fresh = ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                Event::Key { key: pressed, modifiers: held, pressed: true, repeat: false, .. }
                    if *pressed == key && held.matches_exact(modifiers)
            )
        })
    });
    fresh && take_exact_press(ctx, key, modifiers)
}

/// Take the first event that `is_wanted` accepts out of the input.
fn take_event(ctx: &Context, is_wanted: fn(&Event) -> bool) -> bool {
    ctx.input_mut(|input| {
        let found = input.events.iter().position(is_wanted);
        found.map(|at| input.events.remove(at)).is_some()
    })
}

/// How long, in seconds, a held Ctrl or a Ctrl+Shift chord counts for a later
/// V event. It covers the gap between Ctrl going up and V going up, not frame
/// gaps.
const KEY_MEMORY: f64 = 1.0;

/// When Command, and Command with Shift or Alt, were last held.
#[derive(Clone, Copy)]
struct Trail {
    command_at: f64,
    chord_at: f64,
    /// Command went up after the chord. A Command press after that starts a
    /// new gesture, so it clears the chord.
    chord_ended: bool,
}

impl Trail {
    const NEVER: Self = Self {
        command_at: f64::NEG_INFINITY,
        chord_at: f64::NEG_INFINITY,
        chord_ended: false,
    };

    /// Add the modifiers `held` that stood at time `now`.
    fn note(&mut self, held: Modifiers, now: f64) {
        if held.command {
            self.command_at = now;
        }
        if held.command && (held.shift || held.alt) {
            self.chord_at = now;
            self.chord_ended = false;
        } else if !held.command {
            self.chord_ended = true;
        } else if self.chord_ended {
            self.chord_at = f64::NEG_INFINITY;
            self.chord_ended = false;
        }
    }

    fn command_held(&self, now: f64) -> bool {
        now - self.command_at <= KEY_MEMORY
    }

    fn chord_held(&self, now: f64) -> bool {
        now - self.chord_at <= KEY_MEMORY
    }
}

/// What earlier frames showed of the keys that decide a Ctrl+V.
#[derive(Clone, Copy)]
pub(super) struct PrevKeys {
    /// Whether V was down at the end of the last frame.
    v_down: bool,
    trail: Trail,
}

impl Default for PrevKeys {
    fn default() -> Self {
        Self {
            v_down: false,
            trail: Trail::NEVER,
        }
    }
}

/// The modifiers after each event of the frame, then those at its end.
fn frame_modifiers(input: &egui::InputState) -> impl Iterator<Item = Modifiers> + '_ {
    let after_events = input.events.iter().filter_map(|event| match event {
        Event::ModifiersChanged(held)
        | Event::Key {
            modifiers: held, ..
        } => Some(*held),
        _ => None,
    });
    after_events.chain(std::iter::once(input.modifiers))
}

impl PrevKeys {
    /// Add the frame that `ctx` is running to `self`.
    fn after_frame(mut self, ctx: &Context) -> Self {
        ctx.input(|input| {
            self.v_down = input.key_down(Key::V);
            for held in frame_modifiers(input) {
                self.trail.note(held, input.time);
            }
        });
        self
    }
}

/// Take the Ctrl+V that pastes.
///
/// This leans on egui-winit 0.36 (lib.rs:1020-1043): it turns the Ctrl+V press
/// into a paste event, so only the V release comes as a key event. That release
/// has no V press before it, here or in an earlier frame. Ctrl was held
/// shortly before it, in this frame or an earlier one, and never with Shift or
/// Alt. The user may let go of Ctrl before V, so the release can carry no
/// modifiers, but it must not carry Shift or Alt.
///
/// A platform that delivers the press as a key event pastes on the press: a
/// first V press with exactly Ctrl held. Its release is then not taken, as V
/// was down.
fn take_swallowed_paste(ctx: &Context, prev: PrevKeys) -> bool {
    ctx.input_mut(|input| {
        let now = input.time;
        let mut trail = prev.trail;
        let mut v_down = prev.v_down;
        let mut found = None;
        for (at, event) in input.events.iter().enumerate() {
            let (Event::ModifiersChanged(held)
            | Event::Key {
                modifiers: held, ..
            }) = event
            else {
                continue;
            };
            trail.note(*held, now);
            match event {
                Event::Key {
                    key: Key::V,
                    pressed: true,
                    repeat,
                    ..
                } => {
                    if !v_down && !repeat && held.matches_exact(Modifiers::COMMAND) {
                        found = Some(at);
                        break;
                    }
                    v_down = true;
                }
                Event::Key {
                    key: Key::V,
                    pressed: false,
                    ..
                } => {
                    let plain = !held.shift && !held.alt;
                    if !v_down && trail.command_held(now) && !trail.chord_held(now) && plain {
                        found = Some(at);
                        break;
                    }
                    v_down = false;
                }
                _ => {}
            }
        }
        found.map(|at| input.events.remove(at)).is_some()
    })
}

/// Take one press of `key` held with the command modifier and no extra Shift
/// or Alt.
fn take_command_press(ctx: &Context, key: Key) -> bool {
    take_exact_press(ctx, key, Modifiers::COMMAND)
}

impl Shell {
    /// Act on Command+O, Command+Comma, Command+F, Command+A, and Enter.
    ///
    /// A window over the dock owns the keyboard, so none of the keys act, or
    /// get consumed, while one is open. Command+A also leaves a focused text
    /// field its own select all. Focus on a button or a tile does not block it.
    ///
    /// Enter without a modifier opens the selected image. It acts, and is
    /// consumed, only when exactly one image is selected and no widget but a
    /// tile has focus, so a focused button or text field keeps its own Enter.
    /// Shift+Enter and Command+Enter do nothing. The grid keys follow the same
    /// focus rule, see `grid_key_events`.
    ///
    /// Command+Z raises [`crate::gallery::GridMenuEvent::Undo`] under the same
    /// focus rule, and only while the gallery knows of a step to undo. A focused
    /// text field keeps its own undo.
    pub(super) fn handle_shortcuts(&mut self, ctx: &Context) {
        // Kept before the early returns, so the next frame sees this one.
        let prev = self.prev_keys;
        self.prev_keys = prev.after_frame(ctx);
        if self.crash_card.is_open()
            || self.prefs.is_open()
            || self.gallery.manager_open()
            || self.gallery.rename_open()
            || self.gallery.delete_open()
            || self.gallery.tag_picker_open()
        {
            return;
        }
        if take_command_press(ctx, Key::O) {
            self.menu_events.push(MenuEvent::OpenFolder);
        }
        if take_command_press(ctx, Key::Comma) {
            self.menu_events.push(MenuEvent::OpenPreferences);
        }
        if take_command_press(ctx, Key::F) {
            self.gallery.focus_search();
        }
        if !ctx.text_edit_focused() && take_command_press(ctx, Key::A) {
            self.gallery.select_all_visible();
        }
        // The press is consumed only when it opens an image, so it stays for
        // anything else that reads Enter.
        if self.enter_is_free(ctx)
            && self.gallery.sole_selected().is_some()
            && take_exact_press(ctx, Key::Enter, Modifiers::NONE)
        {
            self.gallery.open_selected();
        }
        // Taken only when the app holds a step, so with an empty slot the press
        // stays for anything else that reads it. Ctrl+Shift+Z does nothing.
        if self.enter_is_free(ctx)
            && self.gallery.undo_ready()
            && take_fresh_press(ctx, Key::Z, Modifiers::COMMAND)
        {
            self.gallery.raise_undo();
        }
        self.grid_key_events(ctx, prev);
    }

    /// Raise the menu event of F2, Delete, Shift+Delete, Command+C, and
    /// Command+V.
    ///
    /// A key acts, and is consumed, only when no widget but a tile has focus
    /// and the gallery is ready for it, so a focused text field keeps its own
    /// copy and paste and an unready press stays in the input.
    ///
    /// The platform layer reports a Ctrl+C press as a copy event and drops
    /// the Ctrl+V press, but it reports the V release as a key event whatever
    /// the clipboard holds. The copy event also comes for Ctrl+Shift+C and
    /// Ctrl+Alt+C and carries no modifiers, so Copy takes it only while the
    /// held modifiers are exactly Ctrl. A held Ctrl+C repeats Copy, which is
    /// harmless as the event has no repeat flag.
    ///
    /// Paste takes the release of a V that has no press before it, see
    /// `take_swallowed_paste`. Ctrl must have been held shortly before that
    /// release, in this frame or an earlier one, and never with Shift or Alt.
    /// So Paste also works when the user lets go of Ctrl before V. A plain V
    /// press and release does nothing. A paste event is left alone, so a text
    /// field still gets it.
    ///
    /// F2 and Delete take a new key press, not a repeat of a held key.
    fn grid_key_events(&mut self, ctx: &Context, prev: PrevKeys) {
        if !self.enter_is_free(ctx) {
            return;
        }
        for (key, modifiers) in Gallery::grid_keys() {
            if !self.gallery.grid_key_ready(key, modifiers) {
                continue;
            }
            let taken = match key {
                Key::C => {
                    ctx.input(|input| input.modifiers.matches_exact(Modifiers::COMMAND))
                        && take_event(ctx, |event| matches!(event, Event::Copy))
                }
                Key::V => take_swallowed_paste(ctx, prev),
                _ => take_fresh_press(ctx, key, modifiers),
            };
            if taken {
                self.gallery.raise_grid_key(key, modifiers);
            }
        }
    }

    /// Whether no widget other than a tile holds the keyboard focus.
    ///
    /// A focused button or text field reads Enter itself, so it keeps the key.
    fn enter_is_free(&self, ctx: &Context) -> bool {
        ctx.memory(egui::Memory::focused)
            .is_none_or(|id| self.gallery.is_tile(id))
    }
}
