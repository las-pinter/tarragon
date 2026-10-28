//! The window start options, the maximized flag, and the raise-the-window commands.

use eframe::egui::{UserAttentionType, ViewportBuilder, ViewportCommand, ViewportInfo};

/// The inner size of a window that is not maximized, in points.
const DEFAULT_SIZE: [f32; 2] = tarragon_ui::tokens::WINDOW_SIZE;

/// The maximized flag to keep after a frame. A minimized or full-screen window
/// reports a state that is not the user's choice, and a frame with no reading
/// (some platforms report only on the first frame) says nothing, so both keep
/// the old value.
pub fn maximized_from(info: &ViewportInfo, old: Option<bool>) -> Option<bool> {
    if info.minimized == Some(true) || info.fullscreen == Some(true) {
        return old;
    }
    info.maximized.or(old)
}

/// The viewport for a start. The size is always the default, so a window that
/// leaves the maximized state has a normal size to return to. The caller
/// centers it.
pub fn viewport_for(title: &str, maximized: bool) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(DEFAULT_SIZE)
        .with_maximized(maximized)
}

/// The commands that bring the window to the front for an activate request.
/// Wayland ignores focus, so the last command asks for user attention.
pub fn activate_commands(requested: bool) -> Vec<ViewportCommand> {
    if !requested {
        return Vec::new();
    }
    vec![
        ViewportCommand::Minimized(false),
        ViewportCommand::Visible(true),
        ViewportCommand::Focus,
        ViewportCommand::RequestUserAttention(UserAttentionType::Informational),
    ]
}

#[cfg(test)]
mod tests;
