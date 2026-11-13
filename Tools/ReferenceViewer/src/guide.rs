//! The built-in guide (the Guide button). Keep in step with README.md's
//! "How to use it" section.

use crate::graph::edge_colour;
use eframe::egui::{self, Color32, RichText, Stroke, Ui, pos2, vec2};

pub const SECTIONS: &[(&str, &[&str])] = &[
    (
        "Selecting",
        &[
            "Select something in Studio and the viewer follows it: events, parameters, snapshots, assets, shared effects (browser); buses, returns, VCAs and events in the Mixer window; event instruments (event editor); sends and shared effects in an effect chain (deck).",
            "Whichever selection you changed last wins. Clicking something the viewer can't show (a track, an audio instrument) leaves the view as it is, and Studio's Script Output names it.",
            "Several items of the same kind are shown together, each outlined as selected. Other kinds selected at the same time are counted in the top bar.",
        ],
    ),
    (
        "References and Gain",
        &[
            "References shows how things connect. Gain estimates how loud the selected event comes out on each route to the Master Bus.",
            "Direction (References only): Parent of / Inputs shows what feeds the selection; Referenced in / Outputs shows where it goes; Both shows both sides.",
            "Layout (References only): Graph or List, and Further chain: past the top-level events, their route through the mixer to the Master Bus and the snapshots they start (dimmer boxes, column 'FURTHER CHAIN').",
        ],
    ),
    (
        "Graph",
        &[
            "Sound flows left to right: sources on the left, the selection in the middle, where it goes on the right. Column headings say how far each column is from the selection.",
            "Drag the background to pan. Scroll to pan. Pinch, or hold Cmd (Ctrl on Windows) and scroll, to zoom. Panning stops a little past the edge of the graph. Fit (bottom left) brings the selection back.",
            "Click a box, or the dot on a link, to see its details in the inspector on the right. Its paths light up (what feeds it and where it goes) and the rest dims; click the background to clear.",
            "A box's border is its colour in Studio (grey when none). In lists the colour is a dot.",
            "Selection is an accent bar on the box's left edge: orange for what's selected in Studio, white for the box you clicked to inspect.",
            "Bank status is a pill at each event's top right: 'bank X' in green, or 'no bank' in amber. Amber isn't an error: events used only inside other events don't need a bank of their own.",
            "Double-click a box to open it in Studio. Right-click a box or a link for more: open it, show details, copy its name or path.",
            "A column with more than 10 boxes shows '+N more'. Click it to show them all, and 'Show fewer' to fold it again. Boxes on the path of the box you've clicked are always shown, even in a folded column. Links to the hidden boxes are drawn as one thicker line from '+N more', labelled '×N'; hover it for their names, click it to unfold.",
        ],
    ),
    (
        "List",
        &[
            "Three columns: what feeds the selection, the selection itself, and where it goes. Only the first layer starts open; each chain folds open and closed with its arrow.",
            "With several items selected, each side column has a section per selected item ('FOR ...'). Several instruments playing the same event show as one row with a count.",
            "Click a row to see the link's details; double-click to open that item in Studio; right-click for more.",
        ],
    ),
    (
        "Inspector",
        &[
            "Shows the selected box or link: instrument settings, trigger conditions (green: met now, red: not met), automation with its current value, modulation, tracks, effects, sends, VCAs, snapshots and the mixer route.",
            "Cards / Plain at the top switches its look. Plain is the original layout.",
            "The open button names where Studio will show it: Event Editor for events, the Mixer for buses, returns, VCAs and snapshots, the Assets or Effects browser for assets and shared effects. Studio uses its last-used window of that kind.",
        ],
    ),
    (
        "Gain",
        &[
            "One card per route: the event played directly, and inside each chain of parent events. Each card runs left to right in stages, source to output: what each stage changes and the running level after it, with every parameter at its initial value (Default). 'now' shows Studio's current values where they differ. The step table below each card lists every change.",
            "Sort loudest first or longest first. A route whose trigger conditions aren't met right now is marked in red.",
            "It is an estimate from the project's settings, not a meter. Modulators, snapshots, sends and level-dependent effects are listed but not counted.",
        ],
    ),
];

/// Link colours, for the legend.
const LEGEND: &[(&str, &str)] = &[
    ("event", "event instrument"),
    ("asset", "asset"),
    ("route", "mixer routing"),
    ("send", "send"),
    ("effect", "shared effect"),
    ("vca", "VCA"),
    ("snapshot", "starts a snapshot"),
];

pub fn show(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Reference Viewer guide")
        .open(open)
        .default_size([520.0, 560.0])
        .vscroll(true)
        .show(ctx, |ui| {
            for (title, lines) in SECTIONS {
                ui.label(RichText::new(*title).strong().color(crate::kit::ORANGE));
                for l in *lines {
                    ui.label(*l);
                }
                ui.add_space(8.0);
            }
            ui.label(RichText::new("Link colours").strong().color(crate::kit::ORANGE));
            for (kind, text) in LEGEND {
                legend_row(ui, kind, text);
            }
        });
}

fn legend_row(ui: &mut Ui, kind: &str, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(28.0, 12.0), egui::Sense::hover());
        ui.painter().line_segment([pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)], Stroke::new(2.0, edge_colour(kind)));
        ui.label(RichText::new(text).color(Color32::LIGHT_GRAY));
    });
}
