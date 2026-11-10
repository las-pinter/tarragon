//! Keyboard shortcuts that do what a menu item or a gallery hook does.

use egui::{Context, Event, Key, Modifiers};

use super::{MenuEvent, Shell};

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
    /// Shift+Enter and Command+Enter do nothing.
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
    }

    /// Whether no widget other than a tile holds the keyboard focus.
    ///
    /// A focused button or text field reads Enter itself, so it keeps the key.
    fn enter_is_free(&self, ctx: &Context) -> bool {
        ctx.memory(egui::Memory::focused)
            .is_none_or(|id| self.gallery.is_tile(id))
    }
}
