//! Curve Library: the companion app of the Automation Curve Library for FMOD
//! Studio 2.03.
//!
//! Keeps libraries of automation curves (normalised 0-1, like FMOD 2.04's
//! curve presets), edits them with the mouse or trackpad, and applies them to
//! whatever is selected in Studio through the Studio script
//! `Scripts/DAT-audio Utilities/Open Automation Curve Library.js`.
//!
//! Usage:
//!   curve_library [--libraries <folder>] [--bridge]
//!
//! `--libraries`: the folder of curve libraries. Default: `Libraries` next to
//! this program, which is where it lives when installed in FMOD's Tools folder.
//! `--bridge`: started by the Studio script; talk to Studio over stdin/stdout
//! (see bridge.rs). Nothing else may print to stdout in this mode.
//! Development: `--screenshot out.png` renders a few frames, saves the window
//! as a PNG and quits.

// No console window on Windows. Piped stdin/stdout (the bridge) still work.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod bridge;
mod canvas;
mod curve;
mod library;
mod png;
mod studio;
mod theme;

use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn value_after(args: &[String], flag: &str) -> Option<PathBuf> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(PathBuf::from)
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bridge = args.iter().any(|a| a == "--bridge");
    let screenshot = value_after(&args, "--screenshot");
    let libraries = value_after(&args, "--libraries").unwrap_or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("Libraries")))
            .unwrap_or_else(|| PathBuf::from("Libraries"))
    });
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Curve Library")
            .with_inner_size([1280.0, 760.0])
            // The side panels at their minimum, plus a usable graph.
            .with_min_inner_size([1060.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "DAT Curve Library", // fixed: it names the settings folder
        options,
        Box::new(move |cc| Ok(Box::new(app::EditorApp::new(cc, libraries, screenshot, bridge)))),
    )
}
