//! Sandbox First Person: a floating first-person 3D view of an FMOD Studio
//! Sandbox scene, seen from its listener and kept in sync with it, through the
//! Studio script `Open Sandbox First Person View.js`.
//!
//! Usage:
//!   sandbox_first_person [--bridge] [--demo] [--screenshot out.png]
//!
//! `--bridge`: started by the Studio script; talk to Studio over stdin/stdout
//! (see bridge.rs). Nothing else may print to stdout in this mode.
//! `--demo`: a made-up scene, no Studio (development).
//! `--screenshot out.png`: renders a few frames, saves the window and quits
//! (development; `SFP_CAM="x,y,z,yaw,pitch"` places the camera in demo mode).

// No console window on Windows. Piped stdin/stdout (the bridge) still work.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod auto;
mod bridge;
mod camera;
mod lift;
mod png;
mod scene;
mod view;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bridge = args.iter().any(|a| a == "--bridge");
    let demo = args.iter().any(|a| a == "--demo");
    let screenshot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("First Person")
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([480.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DAT Sandbox First Person", // fixed: it names the settings folder
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, bridge, demo, screenshot)))),
    )
}
