//! Start the window: wgpu first, glow when wgpu cannot start.

use std::error::Error;
use std::ffi::OsString;

use eframe::{NativeOptions, Renderer};
use tarragon_ui::shell::PRODUCT_NAME;
use winit::error::EventLoopError;

use crate::app::App;
use crate::window;

/// Run the window with one renderer until it closes.
fn run_with(renderer: Renderer, app: App) -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: window::viewport_for(PRODUCT_NAME, app.maximized()),
        centered: true,
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
    /// There is no display to draw on, so no renderer can work.
    #[error(
        "cannot connect to the display (DISPLAY={display}, WAYLAND_DISPLAY={wayland}): {source}"
    )]
    NoDisplay {
        display: String,
        wayland: String,
        #[source]
        source: Box<eframe::Error>,
    },
    /// The window failed to run.
    #[error("cannot show the window: {0}")]
    Window(#[from] eframe::Error),
}

/// What to do after the wgpu try failed.
#[derive(Debug, PartialEq, Eq)]
enum Next {
    /// Try again with glow.
    Glow,
    /// The display connection failed; glow cannot help.
    NoDisplay,
    /// The error does not depend on the renderer; report it.
    Report,
}

/// Decide what follows a failed wgpu try.
fn classify(error: &eframe::Error) -> Next {
    match error {
        // The build of the event loop failed (Os or NotSupported). winit
        // marks the loop as created even then, so a glow try would only fail
        // with "can't be recreated" and hide the real cause.
        eframe::Error::WinitEventLoop(EventLoopError::Os(_) | EventLoopError::NotSupported(_)) => {
            Next::NoDisplay
        }
        // A loop that is gone or that exited with a status says nothing about
        // the display. The user code is the same for both renderers. A glow
        // try cannot change either.
        eframe::Error::WinitEventLoop(
            EventLoopError::RecreationAttempt | EventLoopError::ExitFailure(_),
        )
        | eframe::Error::AppCreation(_) => Next::Report,
        // Adapter, device, and surface errors come from wgpu. A window error
        // may differ with glow, so try glow. Glow errors cannot come from a
        // wgpu try; glow is the safe answer.
        eframe::Error::Winit(_)
        | eframe::Error::Wgpu(_)
        | eframe::Error::Glutin(_)
        | eframe::Error::NoGlutinConfigs(..)
        | eframe::Error::OpenGL(_) => Next::Glow,
    }
}

/// Show an environment value, or "unset".
fn show_env(value: Option<OsString>) -> String {
    value.map_or_else(|| "unset".to_owned(), |v| v.to_string_lossy().into_owned())
}

/// The error for a failed display connection.
fn no_display(
    display: Option<OsString>,
    wayland: Option<OsString>,
    source: eframe::Error,
) -> RunError {
    RunError::NoDisplay {
        display: show_env(display),
        wayland: show_env(wayland),
        source: Box::new(source),
    }
}

/// Run the window until it closes. `first` is used for the wgpu try. `reopen`
/// builds a fresh app for the glow try, because a failed try drops its app.
///
/// # Errors
///
/// Returns the error of the glow try when both renderers fail, the display
/// error when the display cannot be reached, or the error from `reopen` when
/// the app cannot be built for the second try.
pub fn run(first: App, reopen: &dyn Fn() -> Result<App, Box<dyn Error>>) -> Result<(), RunError> {
    let error = match run_with(Renderer::Wgpu, first) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    match classify(&error) {
        Next::NoDisplay => Err(no_display(
            std::env::var_os("DISPLAY"),
            std::env::var_os("WAYLAND_DISPLAY"),
            error,
        )),
        Next::Report => Err(error.into()),
        Next::Glow => {
            tracing::warn!("cannot start with wgpu, trying glow: {error}");
            Ok(run_with(Renderer::Glow, reopen().map_err(RunError::Open)?)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui_wgpu::WgpuError;

    #[test]
    fn a_recreation_error_or_exit_status_is_reported_without_a_retry() {
        for error in [
            EventLoopError::RecreationAttempt,
            EventLoopError::ExitFailure(1),
        ] {
            assert_eq!(
                classify(&eframe::Error::WinitEventLoop(error)),
                Next::Report
            );
        }
    }

    #[test]
    fn a_wgpu_error_tries_glow() {
        let error = eframe::Error::Wgpu(WgpuError::NoSurfaceFormatsAvailable);
        assert_eq!(classify(&error), Next::Glow);
        let error = eframe::Error::Wgpu(WgpuError::CustomNativeAdapterSelectionError("x".into()));
        assert_eq!(classify(&error), Next::Glow);
    }

    #[test]
    fn an_app_creation_error_is_reported() {
        let error = eframe::Error::AppCreation("broken".into());
        assert_eq!(classify(&error), Next::Report);
    }

    #[test]
    fn the_display_message_names_both_variables_and_the_cause() {
        let source = eframe::Error::AppCreation("the cause".into());
        let text = no_display(Some(":99".into()), None, source).to_string();
        assert!(
            text.starts_with(
                "cannot connect to the display (DISPLAY=:99, WAYLAND_DISPLAY=unset): "
            ),
            "{text}"
        );
    }
}
