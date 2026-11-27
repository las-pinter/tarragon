//! The Preferences window.
//!
//! The window floats inside the main window and is modal: a dim layer covers
//! the panes and takes their input. The caller sets the values when it opens
//! the window. It gets them back only on Save.

use egui::{Align2, CornerRadius, Key, Modifiers, Window, vec2};

mod draft;
mod fields;
mod form;
mod groups;
mod nav;

use draft::Draft;
use form::{Action, View};
pub use nav::Group;

use crate::theme::dialog_frame;
use crate::{backdrop, tokens};

/// The window size of the redesign page, in points.
const SIZE: egui::Vec2 = vec2(800.0, 560.0);
/// The least room the window leaves at each screen edge, in points.
const SCREEN_EDGE: f32 = 16.0;

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
    /// Whether the dominant colors of an image are computed.
    pub color_tag_enabled: bool,
    /// How many colors are kept for an image.
    pub palette_size: u32,
    /// The smallest share of an image that a color must cover.
    pub min_share: f32,
    /// The saturation below which a color counts as neutral.
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
    /// The user pressed Save. The window is closed.
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
    /// The group shown at the right; it lasts for the session.
    group: Group,
    cache_size: String,
    purging: bool,
    error: Option<String>,
    events: Vec<PrefsEvent>,
}

impl Prefs {
    /// Show the window from the next frame, with `values` in the fields.
    /// `cache_size` is the text for the Cache size row, such as `1.4 GB`.
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

    /// Set the text of the Cache size row.
    pub fn set_cache_size(&mut self, size: String) {
        self.cache_size = size;
    }

    /// Show the busy strip and disable the buttons while a purge runs.
    pub fn set_purging(&mut self, purging: bool) {
        self.purging = purging;
    }

    /// Show an error at the left of the buttons, or clear it.
    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    /// The group the window shows.
    #[must_use]
    pub fn group(&self) -> Group {
        self.group
    }

    /// Show `group` at the right of the list.
    pub fn show_group(&mut self, group: Group) {
        self.group = group;
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
        // The page size, held inside the screen with a little room around it.
        let screen = ctx.content_rect().size() - vec2(2.0, 2.0) * SCREEN_EDGE;
        let size = SIZE.min(screen).max(vec2(1.0, 1.0));
        let frame = dialog_frame()
            .inner_margin(0)
            .corner_radius(CornerRadius::same(tokens::RADIUS_XL + 2));
        let group = &mut self.group;
        let shown = Window::new("Preferences")
            .title_bar(false)
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .fixed_size(size)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(frame)
            .show(ctx, |ui| form::contents(ui, size, draft, &view, group));
        let mut action = shown.and_then(|r| r.inner).flatten();
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        // The purge runs on, so Escape waits like the Cancel button does.
        if escape && !self.purging {
            action = Some(Action::Cancel);
        }
        match action {
            Some(Action::Save) => {
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
