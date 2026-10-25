//! Start the window: wgpu first, glow when wgpu cannot start.

use std::error::Error;

use eframe::{NativeOptions, Renderer};
use tarragon_ui::shell::PRODUCT_NAME;

use crate::app::App;
use crate::window;

/// Run the window with one renderer until it closes.
fn run_with(renderer: Renderer, app: App) -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: window::viewport_for(PRODUCT_NAME, app.geometry()),
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "tarragon",
        options,
        Box::new(move |cc| {
            tarragon_ui::apply_theme(&cc.egui_ctx);
            app.install_waker(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

/// Why the window did not run.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The app could not be built again for the glow try.
    #[error("cannot open the database: {0}")]
    Open(Box<dyn Error>),
    /// The window failed to run.
    #[error("cannot show the window: {0}")]
    Window(#[from] eframe::Error),
}

/// Run the window until it closes. `first` is used for the wgpu try. `reopen`
/// builds a fresh app for the glow try, because a failed try drops its app.
///
/// # Errors
///
/// Returns the error of the glow try when both renderers fail, or the error
/// from `reopen` when the app cannot be built for the second try.
pub fn run(first: App, reopen: &dyn Fn() -> Result<App, Box<dyn Error>>) -> Result<(), RunError> {
    match run_with(Renderer::Wgpu, first) {
        Ok(()) => Ok(()),
        Err(error) => {
            tracing::warn!("cannot start with wgpu, trying glow: {error}");
            Ok(run_with(Renderer::Glow, reopen().map_err(RunError::Open)?)?)
        }
    }
}
