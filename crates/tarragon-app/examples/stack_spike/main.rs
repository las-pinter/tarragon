//! GUI stack spike: a real window that measures frame time with many live textures.
//!
//! The example opens an eframe window and uploads about 1000 procedural
//! textures of 256 by 256 pixels. It shows them in a grid that scrolls by
//! itself, so every texture stays alive and many draw in each frame. A small
//! `egui_dock` area at the bottom shows one tab, which proves that
//! `egui_dock` works with egui 0.36.2.
//!
//! The example asks for a continuous repaint. It records the time between two
//! frames for a fixed duration, after a short warm-up. It then writes a log
//! file, prints the percentiles, and closes the window.
//!
//! The exit code is 0 on pass, 1 on fail, and 2 when the window cannot open or
//! the run is not valid. A run is not valid when the window is minimized or
//! covered during the measure, or when no frame arrives by the wall-clock
//! deadline. A minimized or covered window stops frame callbacks. Keep the
//! window visible and not covered. A lost focus does not stop the frames. The
//! log records the number of measured frames without focus, as a warning.
//! A frame over 100 ms is a hard fail. A p99 of 33 ms or more is a fail.
//!
//! The log file is written for every run, valid or not. A run that is not
//! valid keeps the frames collected so far.
//!
//! ```text
//! cargo run --release --example stack_spike -p tarragon-app -- \
//!     --renderer wgpu --duration 30 --out frame-time-wgpu.log
//! ```
//!
//! Flags:
//!
//! - `--renderer glow|wgpu` picks the renderer at run time. Default: `wgpu`.
//! - `--duration SECONDS` sets the measured time. Default: 30.
//! - `--warmup SECONDS` sets the warm-up time before the measure. Default: 5.
//! - `--textures COUNT` sets the live texture count. Default: 1000.
//! - `--out PATH` sets the log file. Default: `frame-time-<renderer>.log`.
//!
//! The numeric casts are for a measurement harness, not for the library.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

mod report;
mod settings;
mod textures;

use std::process::ExitCode;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use eframe::egui::{self, Id, TextureHandle, TextureOptions, WidgetText};
use eframe::wgpu::rwh::{HasDisplayHandle, RawDisplayHandle};
use egui_dock::{DockArea, DockState, TabViewer};

use report::{Facts, Run, report};
use settings::{Choice, Settings, parse_args};

/// Side of one drawn tile in points.
const TILE: f32 = 120.0;
/// Gap between tiles in points.
const GAP: f32 = 8.0;
/// Scroll speed in points per second.
const SCROLL_SPEED: f32 = 900.0;
/// Time added to the run length before the watchdog ends the run.
const WATCHDOG_SLACK: Duration = Duration::from_secs(20);
/// Time the watchdog waits for the window to close before it reports and exits.
const WATCHDOG_GRACE: Duration = Duration::from_secs(10);

/// The shared run record.
type Shared = Arc<Mutex<Run>>;

/// Lock the run record. A poisoned lock still gives the data, so the log is kept.
fn lock(run: &Shared) -> MutexGuard<'_, Run> {
    run.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// End the run once. The first end wins, and a later end changes nothing.
/// Return true when this call ended the run.
fn end_run(run: &mut Run, invalid: Option<String>) -> bool {
    if run.finished {
        return false;
    }
    run.finished = true;
    run.invalid = invalid;
    true
}

/// The one tab in the dock area.
struct Stats {
    /// The last frame time in milliseconds.
    last_ms: f64,
    /// The phase text.
    phase: &'static str,
    /// The number of live textures.
    textures: usize,
    /// The renderer name.
    renderer: &'static str,
}

impl TabViewer for Stats {
    type Tab = String;

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        Id::new(tab)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab.as_str().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _tab: &mut Self::Tab) {
        ui.label(format!(
            "{} | {} | {} live textures | last frame {:.1} ms",
            self.renderer, self.phase, self.textures, self.last_ms
        ));
    }
}

/// Start a thread that ends the run when no frame callback does.
///
/// A minimized or covered window can stop frame callbacks, and then the run
/// would hang. The thread uses the wall clock. At the deadline it ends the run
/// as not valid and asks the window to close. A run that already ended keeps
/// its own result. After a grace time, if the main thread has not started the
/// report, the thread writes the report itself and exits with the report code.
/// It skips that step when the main thread already writes the report, so the
/// two never write the log at the same time.
fn spawn_watchdog(ctx: &egui::Context, settings: &Settings, run: &Shared) {
    let (ctx, run, settings) = (ctx.clone(), Arc::clone(run), settings.clone());
    let deadline = settings.warmup + settings.duration + WATCHDOG_SLACK;
    std::thread::spawn(move || {
        std::thread::sleep(deadline);
        let reason = "no frame callback at the deadline (window hidden or stalled)";
        if end_run(&mut lock(&run), Some(reason.to_owned())) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx.request_repaint();
        }
        // Wait for the main thread to report. The run may have ended in the
        // window code, and the close may hang.
        std::thread::sleep(WATCHDOG_GRACE);
        let mut guard = lock(&run);
        if guard.reporting {
            return;
        }
        guard.reporting = true;
        let code = report(&settings, &guard);
        std::process::exit(code.into());
    });
}

/// The spike application.
struct Spike {
    /// The live textures. The handles keep them alive.
    textures: Vec<TextureHandle>,
    /// The dock state with one tab.
    dock: DockState<String>,
    /// The settings.
    settings: Settings,
    /// The start of the run.
    start: Instant,
    /// The time of the last frame.
    last: Option<Instant>,
    /// The last frame time in milliseconds.
    last_ms: f64,
    /// The shared run record.
    run: Shared,
    /// The number of tiles drawn in the last frame.
    tiles_last: usize,
}

impl Spike {
    /// Upload the textures and set up the dock area.
    fn new(cc: &eframe::CreationContext<'_>, settings: Settings, run: Shared) -> Self {
        tarragon_ui::install(&cc.egui_ctx);
        let textures = (0..settings.textures)
            .map(|index| {
                cc.egui_ctx.load_texture(
                    format!("spike-{index}"),
                    textures::make_image(index),
                    TextureOptions::LINEAR,
                )
            })
            .collect();
        spawn_watchdog(&cc.egui_ctx, &settings, &run);
        Self {
            textures,
            dock: DockState::new(vec!["Stats".to_owned()]),
            settings,
            start: Instant::now(),
            last: None,
            last_ms: 0.0,
            run,
            tiles_last: 0,
        }
    }

    /// Read the session type and the device name from the frame.
    fn read_facts(frame: &eframe::Frame) -> Facts {
        let session = match frame.display_handle().map(|handle| handle.as_raw()) {
            Ok(RawDisplayHandle::Wayland(_)) => "wayland",
            Ok(RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)) => "x11",
            _ => "other",
        };
        let device = if let Some(state) = frame.wgpu_render_state() {
            let info = state.adapter.get_info();
            format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type)
        } else if frame.gl().is_some() {
            // The workspace forbids unsafe code, and glow needs it to read the
            // driver name. The run script prints `glxinfo` when it exists.
            "OpenGL through glow (driver name not read)".to_owned()
        } else {
            "unknown".to_owned()
        };
        Facts {
            session: session.to_owned(),
            device,
        }
    }

    /// Draw the auto-scrolling texture grid. Return the tiles drawn.
    fn grid(&self, ui: &mut egui::Ui, elapsed: f32) -> usize {
        let width = ui.available_width();
        let cell = TILE + GAP;
        let columns = ((width / cell).floor() as usize).max(1);
        let rows = self.textures.len().div_ceil(columns);
        let content = rows as f32 * cell;
        let travel = (content - ui.available_height()).max(1.0);
        // Ping-pong over the whole content, so all textures draw in turn.
        let phase = (elapsed * SCROLL_SPEED / travel) % 2.0;
        let offset = if phase < 1.0 { phase } else { 2.0 - phase } * travel;
        let mut drawn = 0;
        egui::ScrollArea::vertical()
            .vertical_scroll_offset(offset)
            .auto_shrink(false)
            .show_rows(ui, cell, rows, |ui, range| {
                for row in range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(GAP, GAP);
                        for column in 0..columns {
                            if let Some(texture) = self.textures.get(row * columns + column) {
                                ui.add(egui::Image::new((texture.id(), egui::vec2(TILE, TILE))));
                                drawn += 1;
                            }
                        }
                    });
                }
            });
        drawn
    }

    /// Record one measured frame and end the run when it is time.
    fn measure(&mut self, ui: &egui::Ui, frame_ms: Option<f64>, elapsed: Duration) {
        // A minimized or covered window gets no frames, and the numbers would
        // be wrong. A lost focus is only counted.
        let (minimized, occluded, focused) = ui.input(|input| {
            let info = input.viewport();
            (info.minimized, info.occluded, info.focused)
        });
        let problem = if minimized == Some(true) {
            Some("the window was minimized during the measure")
        } else if occluded == Some(true) {
            Some("the window was covered or hidden during the measure")
        } else {
            None
        };
        let mut run = lock(&self.run);
        if run.finished {
            return;
        }
        if let Some(ms) = frame_ms {
            run.frames.push(ms);
            run.tiles_sum += self.tiles_last;
            run.tiles_max = run.tiles_max.max(self.tiles_last);
            run.focus_lost += usize::from(focused == Some(false));
        }
        let over = elapsed >= self.settings.warmup + self.settings.duration;
        if (problem.is_some() || over) && end_run(&mut run, problem.map(str::to_owned)) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for Spike {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // A continuous repaint, so frames follow the display and the work.
        ui.ctx().request_repaint();
        let now = Instant::now();
        let elapsed = self.start.elapsed();
        if self.last.is_none() {
            lock(&self.run).facts = Self::read_facts(frame);
        }
        let measuring = elapsed >= self.settings.warmup;
        let frame_ms = self.last.replace(now).map(|last| {
            self.last_ms = now.duration_since(last).as_secs_f64() * 1000.0;
            self.last_ms
        });
        if measuring {
            self.measure(ui, frame_ms, elapsed);
        }

        let mut stats = Stats {
            last_ms: self.last_ms,
            phase: if measuring { "measuring" } else { "warm-up" },
            textures: self.textures.len(),
            renderer: self.settings.renderer.name(),
        };
        egui::Panel::bottom("dock").exact_size(64.0).show(ui, |ui| {
            DockArea::new(&mut self.dock)
                .show_close_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(ui, &mut stats);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            self.tiles_last = self.grid(ui, elapsed.as_secs_f32());
        });
    }
}

fn main() -> ExitCode {
    let settings = match parse_args() {
        Ok(settings) => settings,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("tarragon stack spike")
            .with_inner_size([1280.0, 800.0]),
        renderer: match settings.renderer {
            Choice::Glow => eframe::Renderer::Glow,
            Choice::Wgpu => eframe::Renderer::Wgpu,
        },
        ..Default::default()
    };
    let run: Shared = Arc::default();
    let (for_app, shared) = (settings.clone(), Arc::clone(&run));
    let result = eframe::run_native(
        "tarragon-stack-spike",
        options,
        Box::new(move |cc| Ok(Box::new(Spike::new(cc, for_app, shared)))),
    );
    let reason = match result {
        Ok(()) => "the window closed before the run ended".to_owned(),
        Err(error) => {
            eprintln!("cannot run the window: {error}");
            format!("cannot run the window: {error}")
        }
    };
    let mut guard = lock(&run);
    // A run that ended before keeps its result. Otherwise keep the data and
    // mark the run not valid.
    end_run(&mut guard, Some(reason));
    if guard.reporting {
        // The watchdog writes the report and exits the process. Wait for it.
        drop(guard);
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    guard.reporting = true;
    ExitCode::from(report(&settings, &guard))
}
