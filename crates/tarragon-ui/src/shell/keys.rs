//! Keyboard shortcuts that do what a menu item or a gallery hook does.

use egui::{Context, Event, Key, Modifiers};

use super::{MenuEvent, Shell};

/// Take one press of `key` held with the command modifier and no extra Shift
/// or Alt.
///
/// `consume_shortcut` accepts extra Shift and Alt, so Ctrl+Shift+O would open
/// the picker. The press is checked with `matches_exact` first.
fn take_command_press(ctx: &Context, key: Key) -> bool {
    ctx.input_mut(|input| {
        let exact = input.events.iter().any(|event| {
            matches!(
                event,
                Event::Key { key: pressed, modifiers, pressed: true, .. }
                    if *pressed == key && modifiers.matches_exact(Modifiers::COMMAND)
            )
        });
        exact && input.consume_key(Modifiers::COMMAND, key)
    })
}

impl Shell {
    /// Act on Command+O, Command+Comma, Command+F, and Command+A.
    ///
    /// A window over the dock owns the keyboard, so none of the keys act, or
    /// get consumed, while one is open. Command+A also leaves a focused text
    /// field its own select all. Focus on a button or a tile does not block it.
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
    }
}
