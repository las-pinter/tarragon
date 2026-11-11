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
    pub(super) fn handle_shortcuts(&mut self, ctx: &Context) {
        if self.crash_card.is_open() || self.prefs.is_open() || self.gallery.manager_open() {
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
        self.grid_key_events(ctx);
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
    /// harmless as the event has no repeat flag. Paste takes the
    /// release of V with Ctrl held and no Shift or Alt. Paste fires on that
    /// release: if the user lets go of Ctrl before V, nothing happens. A paste
    /// event is left alone, so a text field still gets it. F2 and Delete take
    /// a new key press, not a repeat of a held key.
    fn grid_key_events(&mut self, ctx: &Context) {
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
                Key::V => take_event(ctx, |event| {
                    matches!(
                        event,
                        Event::Key { key: Key::V, pressed: false, modifiers: held, .. }
                            if held.matches_exact(Modifiers::COMMAND)
                    )
                }),
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
