//! Reference Viewer: a live window beside FMOD Studio that follows the
//! selection and shows what is connected to it (events that use it and that
//! it uses, a parameter's uses, a snapshot's effect, and an event's estimated
//! output level), through the Studio script `Scripts/DAT-audio Utilities/Open Reference Viewer.js`.
//!
//! Usage:
//!   reference_viewer [--bridge] [--demo event|top|parameter|snapshot] [--screenshot out.png]
//!
//! `--bridge`: started by the Studio script; talk to Studio over stdin/stdout
//! (see bridge.rs). Nothing else may print to stdout in this mode.
//! `--demo`: show a saved focus message, no Studio (development).
//! `--screenshot out.png`: renders a few frames, saves the window and quits
//! (development; `RFV_PANE=up|both|down|gain`, `RFV_VIEW=graph`,
//! `RFV_SELECT=1` selects the first link).

// No console window on Windows. Piped stdin/stdout (the bridge) still work.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod bridge;
mod curve;
mod gain;
mod graph;
mod guide;
mod kit;
mod model;
mod png;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bridge = args.iter().any(|a| a == "--bridge");
    let demo = args.iter().position(|a| a == "--demo").map(|i| args.get(i + 1).cloned().unwrap_or_else(|| "event".into()));
    let screenshot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Reference Viewer")
            .with_inner_size([1180.0, 720.0])
            .with_min_inner_size([560.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DAT Reference Viewer", // fixed: it names the settings folder
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, bridge, demo, screenshot)))),
    )
}
