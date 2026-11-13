//! Profiler First Person: a first-person 3D view of an FMOD Studio profiler
//! session, seen from the game's listener at the profiler's playhead, through
//! the Studio script `Scripts/DAT-audio Utilities/Open Profiler First Person View.js`.
//!
//! Usage:
//!   profiler_first_person [--bridge] [--session <dir>] [--time <s>] [--screenshot out.png]
//!
//! `--bridge`: started by the Studio script; talk to Studio over stdin/stdout
//! (see bridge.rs). Nothing else may print to stdout in this mode.
//! `--session <dir>`: open a profiler session folder directly, with a time
//! slider (no Studio; event names and ranges are then unknown).
//! `--screenshot out.png`: renders a few frames, saves the window and quits
//! (development; PFP_OFFSET="right,up,back,yaw,pitch", PFP_FOLLOW=n, PFP_IDLE=1).

// No console window on Windows. Piped stdin/stdout (the bridge) still work.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod bridge;
mod camera;
mod capture;
mod events_window;
mod link;
mod png;
mod session;
mod view;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let bridge = args.iter().any(|a| a == "--bridge");
    let session = value("--session").map(PathBuf::from);
    let time = value("--time").and_then(|t| t.parse().ok());
    let screenshot = value("--screenshot").map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Profiler First Person")
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([480.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DAT Profiler First Person", // fixed: it names the settings folder
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, bridge, session, time, screenshot)))),
    )
}
