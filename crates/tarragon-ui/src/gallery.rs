//! Gallery strip and virtual grid.
//!
//! The strip holds the scope tabs, the search field, the info bar, and the sort
//! pills. The grid paints the visible items. [`Gallery::set_items`] sorts the
//! items; the scope and name filters run when a filter input changes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::Ui;
use tarragon_core::filter_bar::FilterBar;
use tarragon_core::filter_state::{FilterScope, FilterState};
use tarragon_core::sorting::SortMode;

use crate::filter_bar::Shown;
use crate::grid::{GridItem, Selection, VirtualGrid, items_in_order, metrics_for};
use crate::tag_manager::TagManager;
pub use crate::tag_manager::{TagManagerEvent, TagRow};
use crate::tokens;

mod info;
mod manager;
mod selected;
pub(crate) mod strip;
mod thumbs;

#[cfg(test)]
mod tests;

/// The quiet window before a typed search applies.
pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

/// One frame of slack on the search wake.
///
/// egui may run the wake frame slightly before the deadline, so each wake
/// frame asks for the remaining time plus one frame.
pub const WAKE_SLACK: Duration = Duration::from_millis(16);

/// The gallery scope: one folder, or every image in the library.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// Show the current folder only.
    #[default]
    Folder,
    /// Show every image.
    All,
}

impl Scope {
    /// The tab label for this scope.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Folder => "Folder",
            Self::All => "All Images",
        }
    }
}

/// A timer that applies a change after a quiet window.
#[derive(Clone, Copy, Debug, Default)]
struct Debouncer {
    /// The instant when the quiet window ends, or none.
    deadline: Option<Instant>,
}

impl Debouncer {
    /// Start the quiet window at `now`.
    fn arm(&mut self, now: Instant) {
        self.deadline = Some(now + SEARCH_DEBOUNCE);
    }

    /// The time left before the quiet window ends, or none when no window runs.
    fn remaining(&self, now: Instant) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    /// Return whether the quiet window has passed, and clear the timer.
    fn is_ready(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.deadline else {
            return false;
        };
        if now >= deadline {
            self.deadline = None;
            return true;
        }
        false
    }
}

/// The gallery panel: the top strip and the virtual grid.
pub struct Gallery {
    /// Every item in the gallery, in no fixed order.
    items: Vec<GridItem>,
    /// The active scope tab.
    scope: Scope,
    /// The folder that the folder scope shows, or none.
    folder: Option<PathBuf>,
    /// The folder path as the caller gave it, or none.
    ///
    /// A scanner reports items under the path it was given, so an item parent
    /// may match this path and not the canonical one.
    raw_folder: Option<PathBuf>,
    /// The text the user typed.
    search: String,
    /// The paths that the database filter keeps, or none for no filter.
    filter_result: Option<HashSet<PathBuf>>,
    /// The colour, tag, and folder filters.
    bar: FilterBar,
    /// The tag names that the tag menu lists.
    tags: Vec<String>,
    /// The folder paths that the folder menu lists.
    folders: Vec<String>,
    /// Whether the colour row shows.
    colors_enabled: bool,
    /// What the filter bar reported since the last take.
    filter_events: Shown,
    /// The "Manage tags" window.
    tag_manager: TagManager,
    /// The timer that applies the typed search.
    debouncer: Debouncer,
    /// The current sort order.
    sort_mode: SortMode,
    /// The selected cell indices.
    selection: Selection,
    /// Grows each time the result of `selected_paths` changes.
    selection_revision: u64,
    /// The selected paths that the revision last saw.
    seen_paths: Vec<PathBuf>,
    /// Whether the sort mode changed since the last take.
    sort_changed: bool,
    /// Item indices in sort order.
    order: Vec<usize>,
    /// Item indices in display order, after the scope and name filters.
    visible: Vec<usize>,
    /// Item index by path, so a thumbnail setter does not scan the list.
    index: HashMap<PathBuf, usize>,
    /// Item indices of the tiles the last frame painted.
    painted: Vec<usize>,
    /// Reused buffer for the tiles of the frame in progress.
    painted_scratch: Vec<usize>,
    /// Grows each time the painted set changes.
    visible_revision: u64,
}

impl Gallery {
    /// Build an empty gallery with the given sort mode.
    #[must_use]
    pub fn new(sort_mode: SortMode) -> Self {
        Self {
            items: Vec::new(),
            scope: Scope::Folder,
            folder: None,
            raw_folder: None,
            search: String::new(),
            filter_result: None,
            bar: FilterBar::default(),
            tags: Vec::new(),
            folders: Vec::new(),
            colors_enabled: true,
            filter_events: Shown::default(),
            tag_manager: TagManager::default(),
            debouncer: Debouncer::default(),
            sort_mode,
            selection: Selection::default(),
            selection_revision: 0,
            seen_paths: Vec::new(),
            sort_changed: false,
            order: Vec::new(),
            visible: Vec::new(),
            index: HashMap::new(),
            painted: Vec::new(),
            painted_scratch: Vec::new(),
            visible_revision: 0,
        }
    }

    /// Replace the items, cache their names, and sort them.
    ///
    /// The selection and its anchor stay by path. A selected path that is not
    /// in the new list drops out.
    pub fn set_items(&mut self, mut items: Vec<GridItem>) {
        for item in &mut items {
            item.refresh_name();
        }
        let path_at = |index: usize| self.items.get(index).map(|item| item.path.clone());
        let kept: Vec<PathBuf> = self.selection.iter().filter_map(path_at).collect();
        let anchor = self.selection.anchor().and_then(path_at);
        self.index.clear();
        for (position, item) in items.iter().enumerate() {
            // The first item wins when a path repeats.
            self.index.entry(item.path.clone()).or_insert(position);
        }
        self.items = items;
        self.selection = Selection::from_parts(
            kept.iter().filter_map(|path| self.index.get(path).copied()),
            anchor.and_then(|path| self.index.get(&path).copied()),
        );
        self.painted.clear();
        self.visible_revision += 1;
        self.resort();
    }

    /// Set the folder that the folder scope shows.
    ///
    /// The path is canonicalized once, here, so a `..` step or a symlink in the
    /// caller's path cannot empty the gallery. A path that does not exist keeps
    /// its text, because canonicalization needs a real file system entry.
    pub fn set_folder(&mut self, folder: Option<PathBuf>) {
        self.folder = folder
            .as_ref()
            .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()));
        self.raw_folder = folder;
        self.refresh_view();
    }

    /// The folder that the folder scope shows.
    #[must_use]
    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_deref()
    }

    /// The active scope tab.
    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Set the scope tab and drop a selection that leaves the view.
    pub fn set_scope(&mut self, scope: Scope) {
        if scope == self.scope {
            return;
        }
        self.scope = scope;
        self.filter_events.changed = true;
        self.refresh_view();
    }

    /// The filter scope that matches the scope tab.
    fn filter_scope(&self) -> FilterScope {
        match self.scope {
            Scope::Folder => FilterScope::Folder,
            Scope::All => FilterScope::Global,
        }
    }

    /// The current sort mode.
    #[must_use]
    pub fn sort_mode(&self) -> SortMode {
        self.sort_mode
    }

    /// Set the sort mode, resort, and clear the changed flag.
    pub fn set_sort_mode(&mut self, mode: SortMode) {
        self.sort_mode = mode;
        self.resort();
        self.sort_changed = false;
    }

    /// Return the sort mode once after a sort change, and clear the flag.
    pub fn take_sort_changed(&mut self) -> Option<SortMode> {
        if !self.sort_changed {
            return None;
        }
        self.sort_changed = false;
        Some(self.sort_mode)
    }

    /// Return the filters and scope once after a bar change or a scope switch.
    pub fn take_filter_change(&mut self) -> Option<(FilterState, FilterScope)> {
        if !std::mem::take(&mut self.filter_events.changed) {
            return None;
        }
        Some((self.bar.state.clone(), self.filter_scope()))
    }

    /// Replace the tag names and folder paths that the filter menus list.
    ///
    /// Active tag and folder chips that the new lists lack are dropped, and
    /// that flags a filter change.
    pub fn set_filter_lists(&mut self, tags: Vec<String>, folders: Vec<String>) {
        self.tags = tags;
        self.folders = folders;
        let pruned_folders = self.bar.prune_folders(&self.folders);
        let pruned_tags = self.bar.prune_tags(&self.tags);
        self.filter_events.changed |= pruned_folders || pruned_tags;
    }

    /// Rename an active tag chip in place and flag a filter change.
    /// If `new` is active too, the old chip is dropped.
    ///
    /// Call it before `set_filter_lists`, so the new list does not read the
    /// rename as a removed tag. An inactive tag changes nothing.
    pub fn rename_filter_tag(&mut self, old: &str, new: &str) {
        self.filter_events.changed |= self.bar.rename_tag(old, new);
    }

    /// Flag a filter change so the caller runs the query again.
    pub fn refresh_filter(&mut self) {
        self.filter_events.changed = true;
    }

    /// The tag names and folder paths that the filter menus list.
    #[must_use]
    pub fn filter_lists(&self) -> (&[String], &[String]) {
        (&self.tags, &self.folders)
    }

    /// Keep only the items whose path is in `paths`, or all items for `None`.
    ///
    /// The set holds paths in the form of the item paths.
    pub fn set_filter_result(&mut self, paths: Option<HashSet<PathBuf>>) {
        self.filter_result = paths;
        self.refresh_view();
    }

    /// Return true once after a tag or folder menu opened, and clear the flag.
    pub fn take_filter_menu_opened(&mut self) -> bool {
        std::mem::take(&mut self.filter_events.opened)
    }

    /// Show or hide the colour row. Hiding clears the colour filters.
    pub fn set_colors_enabled(&mut self, enabled: bool) {
        self.colors_enabled = enabled;
        if enabled {
            return;
        }
        let before = (self.bar.state.colors.len(), self.bar.state.transparent);
        self.bar.clear_colors();
        self.filter_events.changed |= before != (0, false);
    }

    /// Number of items the filters show.
    #[must_use]
    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    /// The cached item indices in display order, after the filters.
    #[cfg(test)]
    pub(crate) fn visible_order(&self) -> &[usize] {
        &self.visible
    }

    /// Draw the top strip and the grid.
    ///
    /// The tabs and the search field draw first and refresh the cached view.
    /// The info bar and the grid then paint that one set in this frame.
    pub fn show(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        self.apply_pending_search(now);
        self.strip(ui, now);
        self.request_search_wake(ui, now);
        self.tag_manager.show(ui.ctx());
        ui.add_space(tokens::SPACING_S);
        self.grid(ui);
    }

    /// Ask for a repaint while the search window runs.
    ///
    /// egui clears a delayed repaint request on each pass, so an early wake
    /// frame must ask again for the time that is left.
    fn request_search_wake(&self, ui: &Ui, now: Instant) {
        if let Some(left) = self.debouncer.remaining(now) {
            ui.ctx().request_repaint_after(left + WAKE_SLACK);
        }
    }

    /// Arm the search timer at `now`.
    fn arm_search(&mut self, now: Instant) {
        self.debouncer.arm(now);
    }

    /// Apply a sort pill pick: resort, rebuild the view, and flag the change.
    fn pick_sort_mode(&mut self, mode: SortMode) {
        if mode == self.sort_mode {
            return;
        }
        self.sort_mode = mode;
        self.resort();
        self.sort_changed = true;
    }

    /// Sort the item indices into the current mode and rebuild the view.
    fn resort(&mut self) {
        self.order = items_in_order(&self.items, self.sort_mode);
        self.refresh_view();
    }

    /// Apply the typed search once the quiet window has passed.
    fn apply_pending_search(&mut self, now: Instant) {
        if !self.debouncer.is_ready(now) {
            return;
        }
        let applied = self.search.to_lowercase();
        if applied == self.bar.state.text {
            return;
        }
        self.bar.state.text = applied;
        self.refresh_view();
    }

    /// Rebuild the cached display order and drop a hidden selection.
    ///
    /// The call runs only on a filter input change, so a quiet frame reads the
    /// cache instead of rebuilding it.
    fn refresh_view(&mut self) {
        let visible: Vec<usize> = self
            .order
            .iter()
            .copied()
            .filter(|&index| self.in_scope(index) && self.matches(index))
            .collect();
        self.visible = visible;
        self.selection.retain_from(&self.visible);
        self.sync_selection_revision();
    }

    /// Return whether the item belongs to the active scope.
    ///
    /// The item parent may equal the canonical folder or the folder as given,
    /// and the item path is compared as given. So the check does no file I/O,
    /// and a symlinked image stays in the folder that holds the link.
    fn in_scope(&self, index: usize) -> bool {
        if self.scope == Scope::All {
            return true;
        }
        let Some(folder) = self.folder.as_deref() else {
            return false;
        };
        self.items
            .get(index)
            .and_then(|item| item.path.parent())
            .is_some_and(|parent| parent == folder || self.raw_folder.as_deref() == Some(parent))
    }

    /// Return whether the item passes the applied search and the path set.
    fn matches(&self, index: usize) -> bool {
        let Some(item) = self.items.get(index) else {
            return false;
        };
        let text = &self.bar.state.text;
        (text.is_empty() || item.name_lower().contains(text))
            && self
                .filter_result
                .as_ref()
                .is_none_or(|set| set.contains(&item.path))
    }

    /// Draw the filtered items through the virtual grid.
    fn grid(&mut self, ui: &mut Ui) {
        let grid = VirtualGrid {
            metrics: metrics_for(ui.available_width()),
            item_count: self.visible.len(),
        };
        self.painted_scratch.clear();
        let response = grid.show_items_collect(
            ui,
            &self.items,
            &self.visible,
            &self.selection,
            &mut self.painted_scratch,
        );
        self.record_painted();
        if let Some(click) = response.click {
            self.apply_grid_click(click);
        }
    }
}
