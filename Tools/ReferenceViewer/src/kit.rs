//! Small shared pieces of UI: kind tags, pills, folder paths, "Open in…"
//! labels, the right-click menu, and the inspector's card blocks.

use crate::graph::Pick;
use crate::model::{Edge, Graph, Node};
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Ui};
use std::sync::atomic::{AtomicBool, Ordering};

pub const DIM: Color32 = Color32::from_rgb(140, 146, 156);
/// "bank X": assigned to a bank directly.
pub const IN_BANK: Color32 = Color32::from_rgb(110, 200, 120);
/// "no bank": worth noticing, not an error (building blocks don't need one).
pub const NO_BANK: Color32 = Color32::from_rgb(225, 180, 90);
pub const ORANGE: Color32 = Color32::from_rgb(240, 140, 30);
const PILL_BG: Color32 = Color32::from_rgb(44, 48, 56);

/// The inspector's look: cards (trial) or plain sections (the original).
static CARDS: AtomicBool = AtomicBool::new(true);
pub fn set_cards(on: bool) {
    CARDS.store(on, Ordering::Relaxed);
}
pub fn cards() -> bool {
    CARDS.load(Ordering::Relaxed)
}

/// Studio's colour, from its name: the seven palette colours, and their
/// "<name> Light <n>" variants. "Default" (or none) is no colour.
/// The RGB values are close to Studio's palette, not read from it.
pub fn studio_colour(name: &str) -> Option<Color32> {
    let name = name.trim();
    let (base, light) = match name.split_once(" Light ") {
        Some((b, n)) => (b, n.trim().parse::<u8>().unwrap_or(1)),
        None => (name, 0),
    };
    let c = match base {
        "Blue" => Color32::from_rgb(52, 120, 230),
        "Purple" => Color32::from_rgb(140, 90, 220),
        "Magenta" => Color32::from_rgb(215, 70, 190),
        "Red" => Color32::from_rgb(225, 65, 65),
        "Yellow" => Color32::from_rgb(230, 200, 50),
        "Green" => Color32::from_rgb(70, 190, 80),
        "Cyan" => Color32::from_rgb(40, 200, 215),
        "Orange" => Color32::from_rgb(240, 140, 40),
        "Gray" | "Grey" => Color32::from_rgb(150, 150, 150),
        _ => return None,
    };
    // Each "Light" step mixes a third of the way toward white.
    let t = (f32::from(light) / 3.0).min(0.9);
    let mix = |v: u8| (f32::from(v) + (255.0 - f32::from(v)) * t) as u8;
    Some(Color32::from_rgb(mix(c.r()), mix(c.g()), mix(c.b())))
}

/// A small dot in Studio's colour, for rows and headers.
pub fn swatch(ui: &mut Ui, name: &str) {
    if let Some(c) = studio_colour(name) {
        let (r, resp) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 4.5, c);
        resp.on_hover_text(format!("Studio colour: {name}"));
    }
}

/// What a box is, in Studio's words, and its colour.
pub fn kind_tag(n: &Node) -> (&'static str, Color32) {
    match n.kind.as_str() {
        "asset" => ("ASSET", Color32::from_rgb(90, 175, 185)),
        "bus" if n.label == "Master Bus" => ("MASTER BUS", Color32::from_rgb(170, 150, 220)),
        "bus" => ("BUS", Color32::from_rgb(170, 150, 220)),
        "return" => ("RETURN", Color32::from_rgb(120, 200, 150)),
        "vca" => ("VCA", Color32::from_rgb(215, 190, 100)),
        "track" => ("EVENT TRACK", Color32::from_rgb(150, 160, 175)),
        "preset" => ("SHARED EFFECT", Color32::from_rgb(215, 140, 130)),
        _ if n.snapshot => ("SNAPSHOT", Color32::from_rgb(210, 120, 120)),
        _ => ("EVENT", Color32::from_rgb(130, 165, 215)),
    }
}

/// The folder part of a path: "event:/SFX/Weapons/Rifle" → "SFX/Weapons".
pub fn folder(path: &str) -> String {
    let p = path.split_once(":/").map_or(path, |(_, rest)| rest);
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

/// The button text for opening a node: Studio opens it in the window for its kind.
pub fn open_label(n: &Node) -> &'static str {
    match n.kind.as_str() {
        "bus" | "return" | "vca" => "Show in Mixer",
        "asset" => "Show in Assets",
        "preset" => "Show in Effects",
        "track" => "Open event in Event Editor",
        _ if n.snapshot => "Open in Mixer (snapshot)",
        _ => "Open in Event Editor",
    }
}

pub fn open_tip(n: &Node) -> String {
    format!(
        "{}.\nStudio uses its last-used window of that kind, or opens one.",
        match n.kind.as_str() {
            "bus" | "return" | "vca" => "Selects it in the Mixer window",
            "asset" => "Selects it in the Assets browser",
            "preset" => "Selects it in the Effects browser",
            _ if n.snapshot => "Opens the snapshot in the Mixer window",
            _ => "Opens the event in the Event Editor",
        }
    )
}

/// One line of facts about a box: how many use it, banks, fader…
pub fn node_facts(g: &Graph, n: &Node) -> Vec<String> {
    let mut f = Vec::new();
    match n.kind.as_str() {
        "asset" => {
            if let Some(a) = &n.asset {
                f.push(format!("{} use{}", a.uses.unwrap_or(0.0) as u32, if a.uses == Some(1.0) { "" } else { "s" }));
                if let Some(l) = a.length {
                    f.push(format!("{} s", crate::gain::fmt(l)));
                }
            }
        }
        "bus" | "return" | "track" | "vca" => {
            if let Some(v) = n.strip.as_ref().and_then(|s| s.volume) {
                f.push(crate::gain::fmt_db(v));
            }
            let ins = g.used_by(&n.id).count();
            if ins > 0 {
                f.push(format!("{ins} input{}", if ins == 1 { "" } else { "s" }));
            }
            if let Some(s) = n.slaves {
                f.push(format!("controls {}", s as u32));
            }
        }
        "preset" => f.push(format!("{} use{}", n.uses.unwrap_or(0.0) as u32, if n.uses == Some(1.0) { "" } else { "s" })),
        _ if n.snapshot => f.push("snapshot".into()),
        _ => {
            f.push(if n.used_by == 0 { "top-level".into() } else { format!("used by {}", n.used_by) });
            let uses = g.used_by(&n.id).filter(|e| e.kind.is_empty() || e.kind == "event").count();
            if uses > 0 {
                f.push(format!("uses {uses}"));
            }
        }
    }
    f
}

/// An event's bank status, coloured: green "bank X", amber "no bank".
pub fn bank_fact(n: &Node) -> Option<(String, Color32)> {
    if !n.is_event() || n.snapshot {
        return None;
    }
    Some(if n.banks.is_empty() { ("no bank".into(), NO_BANK) } else { (format!("bank {}", n.banks.join(", ")), IN_BANK) })
}

/// How a link connects: "track Tail · Multi", "send −10 dB"…
pub fn link_summary(e: &Edge) -> String {
    let mut parts = Vec::new();
    if let Some(t) = &e.track {
        parts.push(format!("track {}", t.name));
    }
    if let Some(p) = &e.place {
        parts.push(p.clone());
    }
    let containers: Vec<String> = e.containers.iter().filter_map(|c| c.container.clone()).filter(|k| k != "Scatterer playlist").collect();
    match e.kind.as_str() {
        "send" => parts.push(format!("send {}", e.send.as_ref().and_then(|s| s.level).map(crate::gain::fmt_db).unwrap_or_default())),
        "route" => parts.push("routed".into()),
        "snapshot" => parts.push("snapshot instrument".into()),
        "vca" => parts.push("VCA".into()),
        "effect" => {}
        "asset" => parts.push(if containers.is_empty() { "audio instrument".into() } else { containers.join(" in ") }),
        _ => parts.push(if containers.is_empty() { "event instrument".into() } else { containers.join(" in ").to_lowercase() }),
    }
    parts.join(" · ")
}

pub fn pill(ui: &mut Ui, text: impl Into<String>, fg: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(PILL_BG)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| ui.label(RichText::new(text.into()).size(10.5).color(fg)))
        .response
}

pub fn tag(ui: &mut Ui, n: &Node) {
    let (t, c) = kind_tag(n);
    ui.label(RichText::new(t).size(10.0).strong().color(c));
}

/// What a right-click menu asked for.
pub enum MenuAct {
    Open(String, String),
    Select(Pick),
}

/// The right-click menu for a box (and, when given, the link it was reached by).
pub fn node_menu(resp: &egui::Response, n: &Node, link: Option<&Edge>) -> Option<MenuAct> {
    let mut act = None;
    resp.context_menu(|ui| {
        ui.label(RichText::new(&n.name).strong());
        if ui.button(open_label(n)).on_hover_text(open_tip(n)).clicked() {
            act = Some(MenuAct::Open(n.open_id().0, n.open_id().1));
            ui.close();
        }
        if ui.button("Show details").clicked() {
            act = Some(MenuAct::Select(Pick::Node(n.id.clone())));
            ui.close();
        }
        if let Some(e) = link {
            if ui.button("Show link details").clicked() {
                act = Some(MenuAct::Select(Pick::Edge(e.id.clone())));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Copy name").clicked() {
            ui.ctx().copy_text(n.name.clone());
            ui.close();
        }
        if !n.path.is_empty() && ui.button("Copy path").clicked() {
            ui.ctx().copy_text(n.path.clone());
            ui.close();
        }
    });
    act
}

/// A titled block in the inspector: a card, or a plain heading (the fallback).
pub fn block(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    if cards() {
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(34, 37, 43))
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(55)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(title.to_uppercase()).size(10.5).strong().color(ORANGE));
                ui.add_space(2.0);
                body(ui);
            });
    } else {
        ui.add_space(8.0);
        ui.label(RichText::new(title).strong().color(ORANGE));
        body(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours() {
        assert!(studio_colour("Default").is_none() && studio_colour("").is_none());
        let blue = studio_colour("Blue").unwrap();
        let light = studio_colour("Blue Light 2").unwrap();
        assert!(light.r() > blue.r() && light.g() > blue.g(), "lighter");
    }

    #[test]
    fn folders() {
        assert_eq!(folder("event:/SFX/Weapons/Rifle_Tail"), "SFX/Weapons");
        assert_eq!(folder("event:/Top"), "");
        assert_eq!(folder("snapshot:/Duck"), "");
        assert_eq!(folder("Weapons/rifle.wav"), "Weapons");
    }
}
