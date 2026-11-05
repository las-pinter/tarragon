//! The folder dialog of the app: File, Open Folder and the cache folder
//! Browse button.
//!
//! The picker sits in the egui context, like the cache state, so the app
//! struct does not grow.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use eframe::egui::{Context, Id};

use crate::app::App;
use crate::picker::{Picker, Purpose};
use crate::sidebar;

/// The context key of the picker.
pub(super) const PICKER_ID: &str = "folder_picker";

/// The context key of a home folder that replaces the real one. Tests set it.
#[cfg(test)]
pub(super) const HOME_ID: &str = "home_override";

/// The picker as the context holds it.
pub(super) type Shared = Arc<Mutex<Picker>>;

/// The home folder to compare a picked folder with.
fn home(ctx: &Context) -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(fake) = ctx.data(|data| data.get_temp::<PathBuf>(Id::new(HOME_ID))) {
        return Some(fake);
    }
    let _ = ctx;
    sidebar::home_dir()
}

/// The picker of this context. It is built on first use.
fn shared(ctx: &Context) -> Shared {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_insert_with(Id::new(PICKER_ID), || {
            let waker = ctx.clone();
            Arc::new(Mutex::new(Picker::new(move || waker.request_repaint())))
        })
        .clone()
    })
}

/// Open a dialog for `purpose` at `start`.
pub(super) fn ask_folder(ctx: &Context, purpose: Purpose, start: Option<PathBuf>) {
    shared(ctx)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .ask(purpose, start);
}

impl App {
    /// Act on the answer of the dialog, if one arrived. A cancel changes
    /// nothing.
    pub(super) fn poll_picker(&mut self, ctx: &Context) {
        let answer = shared(ctx)
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .poll();
        let Some((purpose, picked)) = answer else {
            return;
        };
        match (purpose, picked) {
            (Purpose::OpenFolder, Some(path)) => {
                self.open_picked(&path, home(ctx).as_deref());
            }
            (Purpose::CacheDir, Some(path)) => self
                .shell
                .prefs_mut()
                .set_cache_dir(path.to_string_lossy().into_owned()),
            (_, None) => {}
        }
        // The answer lands after the shell drew, and no input event follows.
        ctx.request_repaint();
    }

    /// Open a folder the user picked. A folder outside home and the saved
    /// roots is stored as a root first, so it stays in the tree after a
    /// restart. A folder under one of them only opens.
    pub(super) fn open_picked(&mut self, path: &Path, home: Option<&Path>) {
        let path = sidebar::normalize(path);
        if !home.is_some_and(|home| path.starts_with(home)) {
            self.add_root(&path, home);
        }
        self.navigate(&path);
    }

    /// Store `path` as a library root unless a saved root already holds it,
    /// then show it. The tree changes only after the write worked.
    fn add_root(&mut self, path: &Path, home: Option<&Path>) {
        match self.db.list_library_roots() {
            Ok(saved) if saved.iter().any(|root| path.starts_with(root)) => return,
            Ok(_) => {}
            Err(error) => {
                tracing::error!("cannot read the library roots: {error}");
                return;
            }
        }
        match self.db.add_library_root(&path.to_string_lossy()) {
            Ok(_) => sidebar::refresh_roots(&mut self.shell, &self.db, home),
            Err(error) => tracing::error!("cannot save the library root: {error}"),
        }
    }
}
