//! The Preferences window.
//!
//! The window floats inside the main window and is modal: a dim layer covers
//! the panes and takes their input. The caller sets the values when it opens
//! the window. It gets them back only on OK.

use egui::{Align2, FontFamily, Key, Modifiers, RichText, Window, vec2};

mod draft;
mod form;

use draft::Draft;
use form::{Action, View};

use crate::{backdrop, fonts, tokens};

/// Window width, in points.
const WIDTH: f32 = 520.0;

/// Every setting the window shows.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefsValues {
    /// How many PSD files decode at the same time.
    pub max_psd_workers: u32,
    /// How many images the Preview panel shows for a multi-selection.
    pub max_multi_preview: u32,
    /// The canvas size, in megapixels, above which a file counts as large.
    pub large_canvas_mp: f32,
    /// The tile grid size, such as `3x3`.
    pub tile_grid: String,
    /// Whether the dominant colours of an image are computed.
    pub color_tag_enabled: bool,
    /// How many colours are kept for an image.
    pub palette_size: u32,
    /// The smallest share of an image that a colour must cover.
    pub min_share: f32,
    /// The saturation below which a colour counts as neutral.
    pub neutral_s: f32,
    /// The cache folder, or `None` for the default one.
    pub cache_dir: Option<String>,
    /// The cache file format.
    pub cache_format: String,
    /// Whether the full-size cache is cleared when the app exits.
    pub clear_full_res: bool,
    /// The cache size cap, in GiB. Zero means no cap.
    pub cache_cap_gb: u32,
    /// Whether debug messages are logged.
    pub debug: bool,
}

/// A choice the user made in the window. The caller acts on it.
#[derive(Clone, Debug, PartialEq)]
pub enum PrefsEvent {
    /// The user pressed OK. The window is closed.
    Ok(PrefsValues),
    /// The user cancelled. The window is closed.
    Cancel,
    /// The user asked to delete the cache. The window stays open.
    PurgeCache,
    /// The user asked to pick the cache folder. The window stays open.
    BrowseCacheDir,
}

/// The state of the Preferences window.
#[derive(Debug, Default)]
pub struct Prefs {
    draft: Option<Draft>,
    cache_size: String,
    purging: bool,
    error: Option<String>,
    events: Vec<PrefsEvent>,
}

impl Prefs {
    /// Show the window from the next frame, with `values` in the fields.
    /// `cache_size` is the text for the Cache Size row, such as `1.4 GB`.
    pub fn open(&mut self, values: PrefsValues, cache_size: String) {
        self.draft = Some(Draft::new(values));
        self.cache_size = cache_size;
        self.error = None;
    }

    /// Whether the window is shown.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.draft.is_some()
    }

    /// Set the cache folder to the one the user picked.
    pub fn set_cache_dir(&mut self, dir: String) {
        if let Some(draft) = &mut self.draft {
            draft.set_cache_dir(dir);
        }
    }

    /// Set the text of the Cache Size row.
    pub fn set_cache_size(&mut self, size: String) {
        self.cache_size = size;
    }

    /// Show the busy strip and disable the buttons while a purge runs.
    pub fn set_purging(&mut self, purging: bool) {
        self.purging = purging;
    }

    /// Show an error above the buttons, or clear it.
    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    /// Take the choices the user made since the last call.
    pub fn take_events(&mut self) -> Vec<PrefsEvent> {
        std::mem::take(&mut self.events)
    }

    /// Draw the window when it is open.
    pub fn show(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.draft else {
            return;
        };
        backdrop::show(ctx, "prefs_backdrop");
        let view = View {
            cache_size: &self.cache_size,
            purging: self.purging,
            error: self.error.as_deref(),
        };
        let title = RichText::new("Preferences")
            .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
            .color(tokens::TEXT_PRIMARY);
        let shown = Window::new(title)
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                form::contents(ui, draft, &view)
            });
        let mut action = shown.and_then(|r| r.inner).flatten();
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        // The purge runs on, so Escape waits like the Cancel button does.
        if escape && !self.purging {
            action = Some(Action::Cancel);
        }
        match action {
            Some(Action::Ok) => {
                if let Some(values) = draft.parse() {
                    self.events.push(PrefsEvent::Ok(values));
                    self.draft = None;
                }
            }
            Some(Action::Cancel) => {
                self.events.push(PrefsEvent::Cancel);
                self.draft = None;
            }
            Some(Action::Purge) => self.events.push(PrefsEvent::PurgeCache),
            Some(Action::Browse) => self.events.push(PrefsEvent::BrowseCacheDir),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests;
