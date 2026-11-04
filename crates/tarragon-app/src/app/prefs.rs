//! The Preferences window of the app: it opens the window with the stored
//! values, saves them on OK, and applies what takes effect at once.

use eframe::egui::{Context, Id};
use tarragon_db::settings::Settings;
use tarragon_ui::prefs::{PrefsEvent, PrefsValues};
use tarragon_ui::shell::MenuEvent;

use super::{App, max_multi_preview};
use crate::filtering::colors_enabled;
use crate::picker::Purpose;
use crate::{prefs, sidebar, tags};

/// The Cache Size text until a measure ends, and when a measure fails.
const CACHE_SIZE_UNKNOWN: &str = "unknown";

/// How many frames the app keeps asking for after the window opens. The form
/// learns its size over several frames, and egui repaints only on input.
const SETTLE_FRAMES: u8 = 4;

/// The context key of the settle frames counter.
pub(super) const SETTLE_ID: &str = "prefs_settle_frames";

mod open;
#[cfg(test)]
mod open_tests;
mod purge;
#[cfg(test)]
mod purge_tests;

impl App {
    /// Open the Preferences window on its menu choice, save and apply the
    /// values when the user presses OK, and purge the cache on its button.
    pub(super) fn apply_prefs_events(&mut self, ctx: &Context) {
        let settle = Id::new(SETTLE_ID);
        for event in self.shell.take_menu_events() {
            match event {
                MenuEvent::OpenPreferences => self.open_prefs(ctx),
                MenuEvent::OpenFolder => {
                    let start = sidebar::home_dir();
                    open::ask_folder(ctx, Purpose::OpenFolder, start);
                }
            }
        }
        for event in self.shell.prefs_mut().take_events() {
            match event {
                PrefsEvent::Ok(values) => self.save_prefs(ctx, values),
                PrefsEvent::PurgeCache => self.purge_cache(ctx),
                PrefsEvent::BrowseCacheDir => {
                    let start = Some(self.cache_root.clone());
                    open::ask_folder(ctx, Purpose::CacheDir, start);
                }
                // A cancel closed the window and stores nothing.
                PrefsEvent::Cancel => {}
            }
        }
        self.poll_picker(ctx);
        self.poll_cache(ctx);
        let left = ctx.data(|data| data.get_temp::<u8>(settle)).unwrap_or(0);
        if left > 0 {
            ctx.data_mut(|data| data.insert_temp(settle, left - 1));
            ctx.request_repaint();
        }
    }

    /// Write `values` in one step. On an error the window opens again with the
    /// same input and the error text; on success the settings that apply at
    /// once take effect.
    fn save_prefs(&mut self, ctx: &Context, values: PrefsValues) {
        if let Err(error) = Settings::new(&self.db).set_many(&prefs::pairs(&values)) {
            self.show_prefs(ctx, values);
            self.shell.prefs_mut().set_error(Some(error.to_string()));
            ctx.request_repaint();
            return;
        }
        self.debug = values.debug;
        self.control.set_debug(values.debug);
        // The registry may clamp the number, so the stored one counts.
        self.max_multi_preview = max_multi_preview(&self.db);
        self.shell
            .preview_mut()
            .set_max_multi_preview(self.max_multi_preview);
        self.feed_preview();
        let enabled = colors_enabled(&self.db);
        self.shell.gallery_mut().set_colors_enabled(enabled);
        tags::refresh_preview(&mut self.shell, &self.db);
        ctx.request_repaint();
    }
}
