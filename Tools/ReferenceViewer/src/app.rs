//! The window: the link to Studio, the panes (Referenced in / Both / Parent of
//! / Gain) for an event, the parameter and snapshot views, and the details
//! panel. Read-only: the one thing it asks Studio for is to open an event.

use crate::bridge::Bridge;
use crate::curve;
use crate::gain::{self, Route, Source, Values, fmt, fmt_db};
use crate::graph::{self, Pick, Side, clip};
use crate::model::{self, Automation, Edge, EventRef, Focus, Graph, Instrument, Modulation, Msg, Node, Strip};
use eframe::egui::{self, Color32, RichText, Ui, collapsing_header::CollapsingState, vec2};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;

pub const ORANGE: Color32 = graph::ORANGE;
const DIM: Color32 = Color32::from_rgb(140, 146, 156);
const GOOD: Color32 = Color32::from_rgb(110, 200, 120);
const BAD: Color32 = Color32::from_rgb(235, 95, 85);
const ON_ORANGE_TEXT: Color32 = Color32::from_rgb(28, 18, 6);

const STORAGE_ON_TOP: &str = "on_top";
const STORAGE_SORT: &str = "by_length";
const STORAGE_CARDS: &str = "inspector_cards";
const STORAGE_FURTHER: &str = "further_chain";
const MIN_SCRIPT: &str = "0.5.0";
const SELECT_HINT: &str = "Select something in Studio: an event, parameter, snapshot, asset, bus, return, send, VCA or shared effect.";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Up,
    Both,
    Down,
}

/// The main view: how things connect, or the gain estimate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    References,
    Gain,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    List,
    Graph,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Link {
    Demo,
    Waiting,
    Linked { studio: String, script: String },
    Lost,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Sel {
    Pick(Pick),
    Use(usize),
    Prop(usize),
    Trigger(usize),
}

/// What a frame's widgets asked for; applied after drawing.
#[derive(Default)]
struct Act {
    select: Option<Sel>,
    clear: bool,
    open: Option<(String, String)>,
}

pub struct App {
    bridge: Option<Bridge>,
    pub link: Link,
    pub focus: Option<Focus>,
    focus_key: String,
    pub cursors: HashMap<String, f64>,
    pub pane: Pane,
    pub tab: Tab,
    pub view: View,
    show_guide: bool,
    pub sel: Option<Sel>,
    on_top: bool,
    applied_on_top: Option<bool>,
    /// What the graph last showed (focus, pane, view): scroll to the focus when it changes.
    graph_shown: String,
    gview: graph::View,
    /// Show the further chain (mixer route, snapshots) past top-level events.
    further: bool,
    /// The focus graph with its further chain, rebuilt on each focus message.
    extended: Option<Graph>,
    by_length: bool,
    /// Lines sent to Studio (kept for tests and demo mode).
    pub sent: Vec<String>,
    note: Option<String>,
    screenshot: Option<PathBuf>,
    frames: u32,
    last_title: String,
}

pub fn apply_theme(ctx: &egui::Context) {
    // Always dark: a hand-tuned dark view under macOS Light mode goes
    // dark-on-dark (learnt on the Curve Library).
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.selection.bg_fill = ORANGE;
    v.selection.stroke = egui::Stroke::new(1.0, ON_ORANGE_TEXT);
    ctx.set_visuals(v);
}

impl App {
    pub fn bare() -> Self {
        App {
            bridge: None,
            link: Link::Waiting,
            focus: None,
            focus_key: String::new(),
            cursors: HashMap::new(),
            pane: Pane::Both,
            tab: Tab::References,
            view: View::Graph,
            show_guide: false,
            sel: None,
            on_top: true,
            applied_on_top: None,
            graph_shown: String::new(),
            gview: graph::View::default(),
            further: true,
            extended: None,
            by_length: false,
            sent: Vec::new(),
            note: None,
            screenshot: None,
            frames: 0,
            last_title: String::new(),
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>, bridge: bool, demo: Option<String>, screenshot: Option<PathBuf>) -> Self {
        apply_theme(&cc.egui_ctx);
        let get = |k: &str| cc.storage.and_then(|s| s.get_string(k));
        let mut app = App::bare();
        app.on_top = get(STORAGE_ON_TOP).map_or(true, |s| s == "1");
        app.by_length = get(STORAGE_SORT).is_some_and(|s| s == "1");
        crate::kit::set_cards(get(STORAGE_CARDS).is_none_or(|s| s == "1"));
        app.further = get(STORAGE_FURTHER).is_none_or(|s| s == "1");
        if std::env::var("RFV_PLAIN").is_ok() {
            crate::kit::set_cards(false);
        }
        if std::env::var("RFV_GUIDE").is_ok() {
            app.show_guide = true;
        }
        app.screenshot = screenshot;
        if bridge {
            let ctx = cc.egui_ctx.clone();
            app.bridge = Some(Bridge::stdio(move || ctx.request_repaint()));
        } else if let Some(which) = demo {
            app.link = Link::Demo;
            let json = match which.as_str() {
                "top" => model::fixtures::EVENT_TOP,
                "parameter" => model::fixtures::PARAMETER,
                "snapshot" => model::fixtures::SNAPSHOT,
                "multi" => model::fixtures::EVENT_MULTI,
                "asset" => model::fixtures::ASSET,
                "preset" => model::fixtures::PRESET,
                "return" => model::fixtures::RETURN,
                "bus" => model::fixtures::BUS,
                _ => model::fixtures::EVENT_LEAF,
            };
            let mut f = model::fixtures::focus(json);
            // Development: RFV_CROWD adds 15 more users of the focus, to show folded columns.
            if std::env::var("RFV_CROWD").is_ok() {
                if let Some(g) = f.graph.as_mut() {
                    let a = g.focus_id.clone();
                    for i in 0..15 {
                        let id = format!("crowd{i}");
                        g.nodes.push(model::Node { id: id.clone(), name: format!("user_{i:02}"), kind: "event".into(), depth: -1, used_by: 1, ..Default::default() });
                        g.edges.push(model::Edge { id: format!("ce{i}"), kind: "event".into(), child_id: a.clone(), parent_id: id, ..Default::default() });
                    }
                }
            }
            app.set_focus(f);
            // Development screenshots.
            match std::env::var("RFV_PANE").as_deref() {
                Ok("up") => app.pane = Pane::Up,
                Ok("down") => app.pane = Pane::Down,
                Ok("gain") => app.tab = Tab::Gain,
                Ok("both") => app.pane = Pane::Both,
                _ => {}
            }
            if std::env::var("RFV_VIEW").as_deref() == Ok("list") {
                app.view = View::List;
            }
            if let Ok(name) = std::env::var("RFV_SELECT_NODE") {
                if let Some(n) = app.extended.as_ref().and_then(|g| g.nodes.iter().find(|n| n.name == name)) {
                    app.sel = Some(Sel::Pick(Pick::Node(n.id.clone())));
                }
            }
            if std::env::var("RFV_SELECT").is_ok() {
                if let Some(e) = app.focus.as_ref().and_then(|f| f.graph.as_ref()).and_then(|g| g.edges.first()) {
                    app.sel = Some(Sel::Pick(Pick::Edge(e.id.clone())));
                }
            }
        }
        app
    }

    fn set_focus(&mut self, f: Focus) {
        let key = format!(
            "{}:{}",
            f.kind,
            f.graph.as_ref().map(|g| g.focus_ids.join(",")).or(f.param.as_ref().map(|p| p.id.clone())).or(f.snapshot.as_ref().map(|s| s.id.clone())).unwrap_or_default()
        );
        if key != self.focus_key {
            self.sel = None;
            self.focus_key = key;
        }
        for (id, p) in &f.params {
            if let Some(c) = p.cursor {
                self.cursors.entry(id.clone()).or_insert(c);
            }
        }
        self.extended = f.graph.as_ref().map(graph::with_further_chain);
        self.focus = Some(f);
    }

    pub fn receive(&mut self, line: &str) {
        match model::parse(line) {
            Ok(Msg::Hello { studio, script }) => self.link = Link::Linked { studio, script },
            Ok(Msg::Focus(f)) => self.set_focus(*f),
            Ok(Msg::Cursors(c)) => self.cursors = c,
            Ok(Msg::Other) => {}
            Err(e) => eprintln!("bad line from Studio: {e}"),
        }
    }

    fn open(&mut self, id: &str, name: &str) {
        let line = serde_json::json!({ "cmd": "open", "id": id }).to_string();
        if let Some(b) = &mut self.bridge {
            b.send(&line);
        }
        self.sent.push(line);
        self.note = Some(if self.link == Link::Demo { format!("Demo: would open '{name}' in Studio") } else { format!("Opening '{name}' in Studio…") });
    }

    fn apply(&mut self, act: Act) {
        if let Some(s) = act.select {
            self.sel = Some(s);
        } else if act.clear {
            self.sel = None;
        }
        if let Some((id, name)) = act.open {
            self.open(&id, &name);
        }
    }

    // ── Bars ─────────────────────────────────────────────────────────────────

    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Reference Viewer").strong());
            ui.separator();
            match &self.focus {
                Some(f) => {
                    let (kind, name, path) = match (f.kind.as_str(), &f.graph) {
                        (_, Some(g)) => {
                            let foci = g.foci();
                            let kind = kind_name(&f.kind, foci.len());
                            match foci.as_slice() {
                                [n] => (kind, n.name.clone(), n.path.clone()),
                                ns => (kind, ns.iter().map(|n| n.name.as_str()).collect::<Vec<_>>().join(", "), String::new()),
                            }
                        }
                        ("parameter", _) => ("Parameter", f.param.as_ref().map(|p| p.name.clone()).unwrap_or_default(), String::new()),
                        ("snapshot", _) => {
                            let s = f.snapshot.as_ref();
                            ("Snapshot", s.map(|s| s.name.clone()).unwrap_or_default(), s.map(|s| s.path.clone()).unwrap_or_default())
                        }
                        ("error", _) => ("Error", f.error.clone().unwrap_or_default(), String::new()),
                        _ => ("", "Nothing selected".to_string(), String::new()),
                    };
                    if !kind.is_empty() {
                        ui.label(RichText::new(kind).color(DIM));
                    }
                    ui.label(RichText::new(clip(&name, 80)).strong().color(ORANGE)).on_hover_text(&name);
                    if !path.is_empty() {
                        ui.label(RichText::new(path).color(DIM).small());
                    }
                    if let Some(sel) = &f.selection {
                        let mut not_shown: Vec<String> = sel.others.iter().map(|(k, n)| kind_name(k, *n as usize).to_lowercase()).map(|k| k.to_string()).collect();
                        not_shown.sort();
                        let counts: Vec<String> = sel.others.iter().map(|(_, n)| fmt(*n)).collect();
                        let mut parts: Vec<String> = not_shown.iter().zip(counts).map(|(k, n)| format!("{n} {k}")).collect();
                        if sel.more > 0 {
                            parts.push(format!("{} more", sel.more));
                        }
                        if !parts.is_empty() {
                            ui.label(RichText::new(format!("(also selected, not shown: {})", parts.join(", "))).color(DIM).small());
                        }
                    }
                }
                None => {
                    ui.label(RichText::new("Nothing selected").color(DIM));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut self.on_top, "On top").on_hover_text("Keep this window above Studio");
                ui.toggle_value(&mut self.show_guide, "Guide").on_hover_text("How to use the viewer");
                let (text, col) = match &self.link {
                    Link::Demo => ("Demo (saved data)".to_string(), DIM),
                    Link::Waiting => ("Waiting for Studio…".to_string(), DIM),
                    Link::Linked { studio, script } => {
                        let old = version_lt(script, MIN_SCRIPT);
                        (format!("Linked · {}{}", short_version(studio), if old { " · script out of date, reload Scripts" } else { "" }), if old { BAD } else { GOOD })
                    }
                    Link::Lost => ("Studio link closed".to_string(), BAD),
                };
                ui.label(RichText::new(text).color(col).small());
                if let Some(n) = &self.note {
                    ui.label(RichText::new(n).color(DIM).small());
                }
            });
        });
    }

    fn pane_bar(&mut self, ui: &mut Ui, g: &Graph) {
        ui.horizontal(|ui| {
            let gain = g.mode() == "event";
            if !gain {
                self.tab = Tab::References;
            }
            // The main view, as two tabs.
            ui.selectable_value(&mut self.tab, Tab::References, RichText::new("References").strong()).on_hover_text("How the selection connects to everything else");
            if gain {
                ui.selectable_value(&mut self.tab, Tab::Gain, RichText::new("Gain").strong()).on_hover_text("Estimated output level on each route to the Master Bus");
            }
            ui.add_space(16.0);
            match self.tab {
                Tab::References => {
                    let foci = g.foci();
                    let ups: usize = foci.iter().map(|n| g.users_of(&n.id).count()).sum();
                    let downs: usize = foci.iter().map(|n| g.used_by(&n.id).count()).sum();
                    let (down, up, down_tip, up_tip) = pane_names(g.mode());
                    if down.is_empty() && self.pane == Pane::Down {
                        self.pane = Pane::Both;
                    }
                    group(ui, "Direction", |ui| {
                        if !down.is_empty() {
                            ui.selectable_value(&mut self.pane, Pane::Down, format!("{down} ({downs})")).on_hover_text(down_tip);
                        }
                        ui.selectable_value(&mut self.pane, Pane::Both, "Both");
                        ui.selectable_value(&mut self.pane, Pane::Up, format!("{up} ({ups})")).on_hover_text(up_tip);
                    });
                    ui.add_space(12.0);
                    group(ui, "Layout", |ui| {
                        ui.selectable_value(&mut self.view, View::Graph, "Graph");
                        ui.selectable_value(&mut self.view, View::List, "List");
                        ui.separator();
                        ui.toggle_value(&mut self.further, "Further chain").on_hover_text("Past the top-level events: their route through the mixer to the Master Bus, and the snapshots they start");
                    });
                }
                Tab::Gain => {
                    group(ui, "Sort", |ui| {
                        ui.selectable_value(&mut self.by_length, false, "Loudest first");
                        ui.selectable_value(&mut self.by_length, true, "Longest first");
                    });
                }
            }
            if g.truncated {
                ui.add_space(12.0);
                ui.label(RichText::new(format!("Cut short: over {} hops or {} items", fmt(g.max_depth.unwrap_or(8.0)), fmt(g.max_nodes.unwrap_or(300.0)))).color(BAD).small());
            }
        });
    }

    // ── Graph panes ──────────────────────────────────────────────────────────

    fn graph_view(&mut self, ui: &mut Ui, g: &Graph, act: &mut Act) {
        let picked = match &self.sel {
            Some(Sel::Pick(p)) => Some(p.clone()),
            _ => None,
        };
        let picked_ref = picked.as_ref();
        if self.view == View::Graph {
            let side = match self.pane {
                Pane::Up => Side::Up,
                Pane::Down => Side::Down,
                Pane::Both => Side::Both,
            };
            // Show the selection whenever it, or the direction, changes.
            let shown = format!("{}|{:?}", self.focus_key, side);
            if shown != self.graph_shown {
                self.gview = graph::fit(g, side, ui.available_size(), &Default::default());
                self.graph_shown = shown;
            }
            let out = graph::show(ui, g, side, picked_ref, &mut self.gview);
            if let Some(p) = out.clicked {
                act.select = Some(Sel::Pick(p));
            } else if out.clear {
                act.clear = true;
            }
            if out.open.is_some() {
                act.open = out.open;
            }
            return;
        }
        // List: what feeds it | the selection | where it goes. Each column scrolls and clips.
        let (down, up, _, _) = pane_names(g.mode());
        let show_down = !down.is_empty() && self.pane != Pane::Up;
        let show_up = !up.is_empty() && self.pane != Pane::Down;
        let mid_w = 250.0f32.min(ui.available_width() / 3.0);
        let sides = usize::from(show_down) + usize::from(show_up);
        let side_w = if sides == 0 { 0.0 } else { (ui.available_width() - mid_w - 16.0 * sides as f32) / sides as f32 };
        let h = ui.available_height();
        ui.horizontal_top(|ui| {
            let column = |ui: &mut Ui, w: f32, title: &str, id: &str, body: &mut dyn FnMut(&mut Ui)| {
                ui.allocate_ui_with_layout(vec2(w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    ui.label(RichText::new(title.to_uppercase()).size(11.0).strong().color(DIM));
                    egui::ScrollArea::both().id_salt(id).auto_shrink(false).show(ui, |ui| body(ui));
                });
            };
            let foci = g.foci();
            let many = foci.len() > 1;
            if show_down {
                column(ui, side_w, down, "list_down", &mut |ui| {
                    for f in &foci {
                        if many {
                            section_head(ui, f);
                        }
                        tree(ui, g, &f.id, false, &mut vec![f.id.clone()], picked_ref, act);
                    }
                });
                ui.separator();
            }
            column(ui, mid_w, "Selected", "list_mid", &mut |ui| {
                for f in &foci {
                    selection_card(ui, g, f, picked_ref, act);
                    ui.add_space(6.0);
                }
            });
            if show_up {
                ui.separator();
                column(ui, side_w, up, "list_up", &mut |ui| {
                    for f in &foci {
                        if many {
                            section_head(ui, f);
                        }
                        tree(ui, g, &f.id, true, &mut vec![f.id.clone()], picked_ref, act);
                    }
                });
            }
        });
    }

    fn gain_view(&self, ui: &mut Ui, f: &Focus) {
        let Some(g) = &f.graph else { return };
        ui.label(
            RichText::new("An estimate from the project's settings, not a meter. Each route runs left to right, source to output; the big number is the level after each stage with every parameter at its initial value (Default). 'now' shows Studio's current values where they differ.")
                .color(DIM)
                .small(),
        );
        ui.add_space(4.0);
        let foci: Vec<&Node> = g.foci().into_iter().filter(|n| n.is_event()).collect();
        egui::ScrollArea::vertical().id_salt("gain").auto_shrink(false).show(ui, |ui| {
            for (k, n) in foci.iter().enumerate() {
                let mut routes = gain::routes_for(f, &self.cursors, &n.id);
                gain::sort(&mut routes, self.by_length);
                let body = |ui: &mut Ui| {
                    for (i, r) in routes.iter().enumerate() {
                        route_card(ui, r, k * 1000 + i);
                        ui.add_space(6.0);
                    }
                };
                if foci.len() == 1 {
                    body(ui);
                } else {
                    let best = routes.first().map(|r| fmt_db(r.now)).unwrap_or_default();
                    egui::CollapsingHeader::new(RichText::new(format!("{}  ·  loudest now {best}", n.name)).strong())
                        .id_salt(("gain_event", &n.id))
                        .default_open(k == 0)
                        .show(ui, body);
                }
            }
        });
    }

    // ── Parameter view ───────────────────────────────────────────────────────

    fn parameter_view(&self, ui: &mut Ui, f: &Focus, act: &mut Act) {
        let Some(p) = &f.param else { return };
        let now = self.cursors.get(&p.id).copied().or(p.cursor);
        egui::Grid::new("param_head").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
            kv(ui, "Range", format!("{} to {}", p.min.map(fmt).unwrap_or("?".into()), p.max.map(fmt).unwrap_or("?".into())));
            kv(ui, "Now in Studio", now.map(|v| label_of(p, v)).unwrap_or("?".into()));
            kv(ui, "Initial value", p.initial.map(|v| label_of(p, v)).unwrap_or("?".into()));
            let mut kind = if p.global { "Global".to_string() } else { "Local".to_string() };
            if p.kind.is_some_and(|k| k != 0.0) {
                kind += &format!(", built-in (type {})", fmt(p.kind.unwrap_or(0.0)));
            }
            if p.read_only {
                kind += ", read-only";
            }
            if p.hold {
                kind += ", hold value";
            }
            kv(ui, "Kind", kind);
            if !p.labels.is_empty() {
                kv(ui, "Labels", p.labels.join(", "));
            }
            if f.sandbox > 0 {
                kv(ui, "Sandbox", format!("set by {} Sandbox control{}", f.sandbox, if f.sandbox == 1 { "" } else { "s" }));
            }
        });
        ui.separator();
        let groups = [
            ("condition", "Trigger conditions"),
            ("automation", "Automation"),
            ("macro", "Event macros"),
            ("modulation", "Modulator settings"),
            ("snapshot", "Snapshot properties"),
            ("passed", "Values passed into events"),
            ("exposed", "Events that have it"),
        ];
        let sel = match &self.sel {
            Some(Sel::Use(i)) => Some(*i),
            _ => None,
        };
        egui::ScrollArea::vertical().id_salt("uses").auto_shrink(false).show(ui, |ui| {
            if f.uses.is_empty() {
                ui.label(RichText::new("Nothing uses this parameter.").color(DIM));
            }
            for (kind, title) in groups {
                let idx: Vec<usize> = (0..f.uses.len()).filter(|&i| f.uses[i].kind == kind).collect();
                if idx.is_empty() {
                    continue;
                }
                ui.add_space(6.0);
                ui.label(RichText::new(format!("{title} ({})", idx.len())).strong());
                egui::Grid::new(("uses", kind)).num_columns(3).striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
                    for i in idx {
                        let u = &f.uses[i];
                        let ev_name = u.event.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| if u.mixer { "Mixer".into() } else { "?".into() });
                        let r = ui.selectable_label(sel == Some(i), RichText::new(&ev_name).strong());
                        if let Some(e) = &u.event {
                            r.clone().on_hover_text(format!("{}\nDouble-click to open in Studio", e.path));
                        }
                        if r.double_clicked() {
                            if let Some(e) = &u.event {
                                act.open = Some((e.id.clone(), e.name.clone()));
                            }
                        } else if r.clicked() {
                            act.select = Some(Sel::Use(i));
                        }
                        ui.label(&u.target);
                        let detail = match u.kind.as_str() {
                            "condition" => format!(
                                "{}–{}{}{}",
                                u.min.map(fmt).unwrap_or_default(),
                                u.max.map(fmt).unwrap_or_default(),
                                if u.inverted { ", inverted" } else { "" },
                                now.map(|v| {
                                    let inside = u.min.is_none_or(|m| v >= m) && u.max.is_none_or(|m| v <= m);
                                    if inside != u.inverted { "  · met now".to_string() } else { "  · not met now".to_string() }
                                })
                                .unwrap_or_default()
                            ),
                            "passed" => format!("passes {} into '{}'", u.value.map(fmt).unwrap_or("?".into()), u.child.as_ref().map(|c| c.name.as_str()).unwrap_or("?")),
                            "exposed" => String::new(),
                            _ => {
                                let mut s = u.property.clone();
                                if let Some(m) = &u.modulates {
                                    s = format!("{s} of the modulator on {m}");
                                }
                                if let Some(sp) = &u.snapshot_property {
                                    s = format!("{s} ({sp})");
                                }
                                if let Some(v) = now.and_then(|x| curve::value_at(&u.curve().points(), x, curve::is_db(&u.property))) {
                                    s += &format!("  · now {}", value_text(&u.property, v));
                                }
                                s
                            }
                        };
                        ui.label(RichText::new(detail).color(DIM));
                        ui.end_row();
                    }
                });
            }
        });
    }

    // ── Snapshot view ────────────────────────────────────────────────────────

    fn snapshot_view(&self, ui: &mut Ui, f: &Focus, act: &mut Act) {
        let Some(s) = &f.snapshot else { return };
        let vals = Values::new(f, &self.cursors);
        egui::ScrollArea::vertical().id_salt("snap").auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("snap_head").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
                kv(ui, "Intensity", s.intensity.map(|v| format!("{}%", fmt(v))).unwrap_or("?".into()));
                kv(ui, "Behaviour code", s.behavior.map(fmt).unwrap_or("?".into()));
            });
            if let Some(m) = &s.macros {
                let drives: Vec<&Automation> = m.automation.iter().filter(|a| a.property == "snapshotIntensity").collect();
                let mods: Vec<&Modulation> = m.modulation.iter().filter(|x| x.property == "snapshotIntensity").collect();
                if !drives.is_empty() || !mods.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new("What drives its intensity").strong());
                    for a in drives {
                        automation_line(ui, a, &vals);
                    }
                    for x in mods {
                        modulation_line(ui, x);
                    }
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Started by ({})", s.triggers.len())).strong());
            if s.triggers.is_empty() {
                ui.label(RichText::new("No snapshot instruments start it (it may be started by the game).").color(DIM));
            }
            let sel_t = match &self.sel {
                Some(Sel::Trigger(i)) => Some(*i),
                _ => None,
            };
            egui::Grid::new("snap_triggers").num_columns(4).striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
                for (i, t) in s.triggers.iter().enumerate() {
                    let ev = t.event.clone().unwrap_or_default();
                    let r = ui.selectable_label(sel_t == Some(i), RichText::new(&ev.name).strong());
                    if r.double_clicked() {
                        act.open = Some((ev.id.clone(), ev.name.clone()));
                    } else if r.clicked() {
                        act.select = Some(Sel::Trigger(i));
                    }
                    ui.label(t.track.as_ref().map(|t| format!("track '{}'", t.name)).unwrap_or_default());
                    ui.label(format!("intensity {}%", t.intensity.map(fmt).unwrap_or("?".into())));
                    ui.label(RichText::new(if t.conditions.is_empty() { String::new() } else { format!("{} condition(s)", t.conditions.len()) }).color(DIM));
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Properties it changes ({})", s.properties.len())).strong());
            let sel_p = match &self.sel {
                Some(Sel::Prop(i)) => Some(*i),
                _ => None,
            };
            egui::Grid::new("snap_props").num_columns(4).striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
                ui.label(RichText::new("Target").color(DIM));
                ui.label(RichText::new("Property").color(DIM));
                ui.label(RichText::new("Normal, then in snapshot").color(DIM));
                ui.label(RichText::new("At full intensity").color(DIM));
                ui.end_row();
                for (i, p) in s.properties.iter().enumerate() {
                    if ui.selectable_label(sel_p == Some(i), &p.target).clicked() {
                        act.select = Some(Sel::Prop(i));
                    }
                    ui.label(&p.property);
                    ui.label(format!(
                        "{} to {}",
                        p.base.map(|v| value_text(&p.property, v)).unwrap_or("?".into()),
                        p.value.map(|v| value_text(&p.property, v)).unwrap_or("?".into())
                    ));
                    let change = match (p.base, p.value) {
                        (Some(b), Some(v)) if curve::is_db(&p.property) => fmt_db(v - b).replace("dB", "dB change"),
                        (Some(b), Some(v)) => format!("{:+}", (v - b) * 100.0_f64.round() / 100.0),
                        _ => String::new(),
                    };
                    ui.label(RichText::new(change).color(DIM));
                    ui.end_row();
                }
            });
            if !s.scoped.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Scoped strips").strong());
                for x in &s.scoped {
                    ui.label(&x.strip);
                }
            }
        });
    }

    // ── Details panel ────────────────────────────────────────────────────────

    fn details(&self, ui: &mut Ui, f: &Focus, shown: Option<&Graph>, act: &mut Act) {
        let vals = Values::new(f, &self.cursors);
        // The trial card look, with the original plain look as a fallback.
        ui.horizontal(|ui| {
            ui.label(RichText::new("INSPECTOR").size(10.5).strong().color(DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut cards = crate::kit::cards();
                ui.selectable_value(&mut cards, false, "Plain");
                ui.selectable_value(&mut cards, true, "Cards");
                crate::kit::set_cards(cards);
            });
        });
        ui.separator();
        match (shown, &self.sel) {
            (Some(g), Some(Sel::Pick(Pick::Edge(id)))) => {
                if let Some(e) = g.edges.iter().find(|e| &e.id == id) {
                    edge_details(ui, g, e, &vals, act);
                }
            }
            (Some(g), Some(Sel::Pick(Pick::Node(id)))) => {
                if let Some(n) = g.node(id) {
                    node_details(ui, g, n, &vals, act);
                }
            }
            (Some(g), _) => {
                if let Some(n) = g.focus() {
                    node_details(ui, g, n, &vals, act);
                }
            }
            (None, Some(Sel::Use(i))) => {
                if let Some(u) = f.uses.get(*i) {
                    heading(ui, &u.target);
                    if let Some(e) = &u.event {
                        event_link(ui, "In", e, act);
                    }
                    if let Some(c) = &u.child {
                        event_link(ui, "Into", c, act);
                    }
                    if !u.property.is_empty() {
                        ui.label(format!("Property: {}", u.property));
                    }
                    let pts = u.curve().points();
                    if !pts.is_empty() {
                        section(ui, "Curve points");
                        for p in pts {
                            ui.label(format!("at {}: {}{}", fmt(p.x), value_text(&u.property, p.y), shape_text(p.shape, p.s)));
                        }
                    }
                    automation_list(ui, &u.automation, &vals);
                }
            }
            (None, Some(Sel::Prop(i))) => {
                if let Some(p) = f.snapshot.as_ref().and_then(|s| s.properties.get(*i)) {
                    heading(ui, &format!("{} {}", p.target, p.property));
                    ui.label(format!("Normal {}, in the snapshot {}", p.base.map(|v| value_text(&p.property, v)).unwrap_or("?".into()), p.value.map(|v| value_text(&p.property, v)).unwrap_or("?".into())));
                    automation_list(ui, &p.automation, &vals);
                    modulation_list(ui, &p.modulation);
                }
            }
            (None, Some(Sel::Trigger(i))) => {
                if let Some(t) = f.snapshot.as_ref().and_then(|s| s.triggers.get(*i)) {
                    heading(ui, "Snapshot instrument");
                    if let Some(e) = &t.event {
                        event_link(ui, "In", e, act);
                    }
                    instrument_ui(ui, t, &vals);
                }
            }
            _ => {
                ui.label(RichText::new("Click a row for details.").color(DIM));
            }
        }
    }

    fn dev_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frames += 1;
        ctx.request_repaint();
        if self.frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Err(e) = crate::png::write_rgba(&path, image.size[0] as u32, image.size[1] as u32, &rgba) {
                eprintln!("screenshot failed: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

// ── List rows ────────────────────────────────────────────────────────────────

/// The selection, in the list's middle column.
fn selection_card(ui: &mut Ui, g: &Graph, n: &Node, picked: Option<&Pick>, act: &mut Act) {
    let sel = picked == Some(&Pick::Node(n.id.clone()));
    selection_card_inner(ui, g, n, sel, act);
}

fn selection_card_inner(ui: &mut Ui, g: &Graph, n: &Node, sel: bool, act: &mut Act) {
    // Border: the item's Studio colour (grey when none). Selection: the orange tag.
    let border = crate::kit::studio_colour(&n.color)
        .map_or(egui::Stroke::new(if sel { 1.5 } else { 1.0 }, Color32::from_gray(if sel { 150 } else { 80 })), |c| egui::Stroke::new(2.0, c));
    let resp = egui::Frame::new()
        .fill(Color32::from_rgb(40, 36, 30))
        .stroke(border)
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                crate::kit::tag(ui, n);
                ui.label(RichText::new("· SELECTED").size(10.0).strong().color(ORANGE));
            });
            ui.label(RichText::new(&n.name).strong().size(14.0));
            let f = crate::kit::folder(&n.path);
            if !f.is_empty() {
                ui.label(RichText::new(f).color(DIM).small());
            }
            ui.horizontal_wrapped(|ui| {
                for fact in crate::kit::node_facts(g, n) {
                    crate::kit::pill(ui, fact, Color32::LIGHT_GRAY);
                }
                if let Some((bank, col)) = crate::kit::bank_fact(n) {
                    crate::kit::pill(ui, bank, col);
                }
            });
        })
        .response
        .interact(egui::Sense::click());
    if resp.double_clicked() {
        act.open = Some(n.open_id());
    } else if resp.clicked() {
        act.select = Some(Sel::Pick(Pick::Node(n.id.clone())));
    }
    menu_into(crate::kit::node_menu(&resp, n, None), act);
    resp.on_hover_text(format!("{}\nClick for details. Double-click: {}. Right-click for more.", n.path, crate::kit::open_label(n)));
}

fn menu_into(m: Option<crate::kit::MenuAct>, act: &mut Act) {
    match m {
        Some(crate::kit::MenuAct::Open(id, name)) => act.open = Some((id, name)),
        Some(crate::kit::MenuAct::Select(p)) => act.select = Some(Sel::Pick(p)),
        None => {}
    }
}

/// A labelled group of toolbar buttons, so groups read as separate.
fn group(ui: &mut Ui, label: &str, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(Color32::from_rgb(34, 37, 43))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(10.5).color(DIM));
                body(ui);
            });
        });
}

/// The chain toward destinations (`up`: events that use it, outputs) or toward
/// sources (events it uses, inputs), collapsible.
fn tree(ui: &mut Ui, g: &Graph, id: &str, up: bool, path: &mut Vec<String>, picked: Option<&Pick>, act: &mut Act) {
    let edges: Vec<&Edge> = if up { g.users_of(id).collect() } else { g.used_by(id).collect() };
    if edges.is_empty() && path.len() == 1 {
        ui.label(RichText::new("Nothing here.").color(DIM));
    }
    // One row per item: several instruments playing the same event merge.
    let mut groups: Vec<(&String, Vec<&Edge>)> = Vec::new();
    for e in edges {
        let other = if up { &e.parent_id } else { &e.child_id };
        match groups.iter_mut().find(|(o, _)| *o == other) {
            Some((_, es)) => es.push(e),
            None => groups.push((other, vec![e])),
        }
    }
    for (other_id, es) in groups {
        let e = es[0];
        let Some(other) = g.node(other_id) else { continue };
        let looped = path.contains(other_id);
        let more = !looped && (if up { g.users_of(other_id).next().is_some() } else { g.used_by(other_id).next().is_some() });
        let state_id = ui.make_persistent_id(("tree", up, &e.id, path.len()));
        if more {
            // Only the first layer starts open (its children show, folded).
            let state = CollapsingState::load_with_default_open(ui.ctx(), state_id, path.len() == 1);
            state
                .show_header(ui, |ui| row(ui, e, other, up, looped, es.len(), picked, act))
                .body(|ui| {
                    path.push(other_id.clone());
                    tree(ui, g, other_id, up, path, picked, act);
                    path.pop();
                });
        } else {
            ui.horizontal(|ui| {
                ui.add_space(ui.spacing().indent);
                row(ui, e, other, up, looped, es.len(), picked, act);
            });
        }
    }
}

/// A heading per selected item, when several are selected.
fn section_head(ui: &mut Ui, n: &Node) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("FOR").size(10.0).color(DIM));
        ui.label(RichText::new(&n.name).size(12.0).strong().color(ORANGE));
    });
    ui.separator();
}

/// How a link reads from one side: "in B" (B uses it), "uses A" (it uses A)…
fn verb(kind: &str, up: bool) -> &'static str {
    match (kind, up) {
        ("asset", true) => "in",
        ("asset", false) => "plays",
        ("route", true) => "out to",
        ("route", false) => "in from",
        ("send", true) => "sends to",
        ("send", false) => "sent from",
        ("effect", true) => "on",
        ("effect", false) => "shared effect",
        ("vca", true) => "controlled by",
        ("vca", false) => "controls",
        ("snapshot", true) => "starts",
        ("snapshot", false) => "started by",
        (_, true) => "in",
        (_, false) => "uses",
    }
}

#[allow(clippy::too_many_arguments)]
fn row(ui: &mut Ui, e: &Edge, other: &Node, up: bool, looped: bool, count: usize, picked: Option<&Pick>, act: &mut Act) {
    use crate::kit::{pill, tag};
    pill(ui, verb(&e.kind, up), graph::edge_colour(&e.kind));
    crate::kit::swatch(ui, &other.color);
    tag(ui, other);
    let sel = picked == Some(&Pick::Edge(e.id.clone()));
    let r = ui.selectable_label(sel, RichText::new(&other.name).strong());
    if r.double_clicked() {
        act.open = Some(other.open_id());
    } else if r.clicked() {
        act.select = Some(Sel::Pick(Pick::Edge(e.id.clone())));
    }
    menu_into(crate::kit::node_menu(&r, other, Some(e)), act);
    r.on_hover_text(format!(
        "{}\nClick for the link's details. Double-click: {}. Right-click for more.",
        if other.path.is_empty() { &other.label } else { &other.path },
        crate::kit::open_label(other)
    ));
    let f = crate::kit::folder(&other.path);
    if !f.is_empty() {
        ui.label(RichText::new(f).color(DIM).small());
    }
    if count > 1 {
        pill(ui, format!("×{count} instruments"), ORANGE);
    }
    let summary = crate::kit::link_summary(e);
    if !summary.is_empty() {
        pill(ui, summary, Color32::LIGHT_GRAY);
    }
    for b in graph::edge_badges(e).into_iter().filter(|b| !b.starts_with("send ") && b != "Multi" && b != "Scatterer") {
        pill(ui, b, DIM);
    }
    if other.is_event() {
        if up && other.used_by == 0 {
            pill(ui, "top-level", GOOD);
        }
        if let Some((bank, col)) = crate::kit::bank_fact(other) {
            pill(ui, bank, col);
        }
    }
    if other.further {
        pill(ui, "further chain", DIM);
    }
    if looped {
        pill(ui, "loops back", BAD);
    }
}

/// Pane names per graph mode: (toward sources, toward destinations, tips).
fn pane_names(mode: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match mode {
        "asset" => ("", "Used in", "", "Events that play this asset, and the events that use those"),
        "preset" => ("", "Used on", "", "Tracks, buses and events this shared effect is on"),
        "bus" | "return" | "send" => ("Inputs", "Outputs", "What feeds it: buses, events and sends", "Where it goes: its output chain, and its sends"),
        "vca" => ("Controls", "VCA", "The strips this VCA controls", ""),
        _ => ("Parent of", "Referenced in", "Events this one uses, chained down", "Events that use this one, chained up to top-level events"),
    }
}

fn kind_name(kind: &str, n: usize) -> &'static str {
    match (kind, n == 1) {
        ("event", true) => "Event",
        ("event", false) => "Events",
        ("asset", true) => "Asset",
        ("asset", false) => "Assets",
        ("preset", true) => "Shared effect",
        ("preset", false) => "Shared effects",
        ("bus", true) => "Bus",
        ("bus", false) => "Buses",
        ("return", true) => "Return",
        ("return", false) => "Returns",
        ("send", true) => "Send",
        ("send", false) => "Sends",
        ("vca", true) => "VCA",
        ("vca", false) => "VCAs",
        ("parameter", true) => "Parameter",
        ("parameter", false) => "Parameters",
        ("snapshot", true) => "Snapshot",
        ("snapshot", false) => "Snapshots",
        _ => "",
    }
}

// ── Gain cards ───────────────────────────────────────────────────────────────

/// One route as a strip of stages, source to output: what each stage changes
/// and the running level after it (Default values first, Now underneath when
/// it differs). The full step table folds open below.
fn route_card(ui: &mut Ui, r: &Route, i: usize) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(r.title()).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("Now {}", fmt_db(r.now))).color(DIM));
                ui.label(RichText::new(format!("Default {}", fmt_db(r.default))).strong().color(ORANGE).size(15.0));
            });
        });
        if !r.plays_now {
            ui.label(RichText::new(format!("Won't play at the current values: {}", r.blocked_by.join("; "))).color(BAD).small());
        }
        if r.truncated {
            ui.label(RichText::new("Incomplete: the chain was cut short or its mixer route is missing.").color(BAD).small());
        }
        egui::ScrollArea::horizontal().id_salt(("stages", i)).auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                stage_box(ui, "Source", &[], 0.0, 0.0, 0.0, 0.0, &[], &[]);
                for st in &r.stages {
                    connector(ui);
                    let lines: Vec<String> = st.steps.iter().map(|s| format!("{} {}", s.what, fmt_db(s.default))).collect();
                    stage_box(ui, &st.place, &lines, st.default, st.now, st.default_total, st.now_total, &st.snapshots, &st.uncounted);
                }
                connector(ui);
                stage_box(ui, "Output", &[], r.default, r.now, r.default, r.now, &[], &[]);
            });
        });
        let id = ui.make_persistent_id(("route", i, r.title()));
        CollapsingState::load_with_default_open(ui.ctx(), id, false)
            .show_header(ui, |ui| {
                ui.label(RichText::new(format!("Step table ({} change{})", r.steps.len(), if r.steps.len() == 1 { "" } else { "s" })).color(DIM));
            })
            .body(|ui| {
                if r.steps.is_empty() {
                    ui.label(RichText::new("No level changes on this route.").color(DIM));
                    return;
                }
                egui::Grid::new(("steps", i)).num_columns(4).striped(true).spacing([14.0, 2.0]).show(ui, |ui| {
                    ui.label(RichText::new("Where").color(DIM));
                    ui.label(RichText::new("What").color(DIM));
                    ui.label(RichText::new("Default").color(DIM));
                    ui.label(RichText::new("Now").color(DIM));
                    ui.end_row();
                    for s in &r.steps {
                        ui.label(&s.place);
                        let w = ui.label(&s.what);
                        if !s.notes.is_empty() {
                            w.on_hover_text(s.notes.join("\n"));
                        }
                        ui.label(fmt_db(s.default));
                        ui.label(RichText::new(fmt_db(s.now)).color(DIM));
                        ui.end_row();
                    }
                });
                for s in &r.steps {
                    for n in &s.notes {
                        ui.label(RichText::new(format!("• {}: {n}", s.place)).color(DIM).small());
                    }
                }
            });
    });
}

#[allow(clippy::too_many_arguments)]
fn stage_box(ui: &mut Ui, place: &str, lines: &[String], default: f64, now: f64, default_total: f64, now_total: f64, snaps: &[String], uncounted: &[String]) {
    let changes = !lines.is_empty();
    egui::Frame::new()
        .fill(if changes { Color32::from_rgb(38, 41, 47) } else { Color32::from_rgb(31, 33, 38) })
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(if changes { 80 } else { 55 })))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| ui.vertical(|ui| {
            ui.set_width(150.0);
            ui.label(RichText::new(place).size(10.5).color(DIM));
            for l in lines {
                ui.label(RichText::new(l).size(11.0));
            }
            if changes && (default != 0.0 || now != 0.0) {
                ui.label(RichText::new(format!("stage {}", fmt_db(default))).size(10.5).color(DIM));
            }
            ui.label(RichText::new(fmt_db(default_total)).strong().size(15.0).color(if changes { ORANGE } else { Color32::LIGHT_GRAY }));
            if (now_total - default_total).abs() > 0.05 {
                ui.label(RichText::new(format!("now {}", fmt_db(now_total))).size(10.5).color(DIM));
            }
            for s in snaps {
                crate::kit::pill(ui, "snapshot", Color32::from_rgb(210, 120, 160)).on_hover_text(s);
            }
            for u in uncounted {
                crate::kit::pill(ui, "not counted", DIM).on_hover_text(u);
            }
        }));
}

fn connector(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(18.0, 60.0), egui::Sense::hover());
    let y = r.top() + 30.0;
    let p = ui.painter();
    let c = Color32::from_gray(110);
    p.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right() - 4.0, y)], egui::Stroke::new(1.5, c));
    p.add(egui::Shape::convex_polygon(vec![egui::pos2(r.right(), y), egui::pos2(r.right() - 6.0, y - 4.0), egui::pos2(r.right() - 6.0, y + 4.0)], c, egui::Stroke::NONE));
}

// ── Details ──────────────────────────────────────────────────────────────────

fn heading(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).strong().size(15.0));
}

fn section(ui: &mut Ui, s: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(s).strong().color(ORANGE));
}

fn kv(ui: &mut Ui, k: &str, v: impl Into<String>) {
    ui.label(RichText::new(k).color(DIM));
    ui.label(v.into());
    ui.end_row();
}

fn event_link(ui: &mut Ui, label: &str, e: &EventRef, act: &mut Act) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(DIM));
        if ui.link(&e.name).on_hover_text(format!("{}\nClick to open in Studio", e.path)).clicked() {
            act.open = Some((e.id.clone(), e.name.clone()));
        }
    });
}

/// The inspector's header for a box: what it is, where it lives, facts, and
/// where Studio will open it.
fn node_header(ui: &mut Ui, g: &Graph, n: &Node, act: &mut Act) {
    ui.horizontal(|ui| {
        crate::kit::tag(ui, n);
        if crate::kit::studio_colour(&n.color).is_some() {
            crate::kit::swatch(ui, &n.color);
            ui.label(RichText::new(&n.color).size(10.0).color(DIM));
        }
    });
    heading(ui, &n.name);
    if !n.path.is_empty() {
        ui.label(RichText::new(&n.path).color(DIM).small());
    }
    ui.horizontal_wrapped(|ui| {
        for f in crate::kit::node_facts(g, n) {
            crate::kit::pill(ui, f, Color32::LIGHT_GRAY);
        }
        if let Some((bank, col)) = crate::kit::bank_fact(n) {
            crate::kit::pill(ui, bank, col);
        }
    });
    if ui.button(crate::kit::open_label(n)).on_hover_text(crate::kit::open_tip(n)).clicked() {
        act.open = Some(n.open_id());
    }
}

fn node_details(ui: &mut Ui, g: &Graph, n: &Node, vals: &Values, act: &mut Act) {
    use crate::kit::block;
    node_header(ui, g, n, act);
    if !n.is_event() {
        if let Some(a) = &n.asset {
            block(ui, "Audio file", |ui| {
                ui.label(format!(
                    "{} s, {} channel{}, {} kHz{}",
                    a.length.map(fmt).unwrap_or("?".into()),
                    a.channels.map(fmt).unwrap_or("?".into()),
                    if a.channels == Some(1.0) { "" } else { "s" },
                    a.rate.map(fmt).unwrap_or("?".into()),
                    if a.streaming { ", streaming" } else { "" }
                ));
            });
        }
        if let Some(fx) = &n.effect {
            block(ui, &fx.kind, |ui| settings_ui(ui, &fx.settings));
        }
        if let Some(e) = &n.event {
            event_link(ui, "In event", e, act);
        }
        if let Some(s) = &n.strip {
            block(ui, &n.label, |ui| strip_ui(ui, s, vals));
        }
        return;
    }
    if let Some(m) = &n.master {
        block(ui, "Master track", |ui| strip_ui(ui, m, vals));
    }
    if let Some(m) = &n.macros {
        block(ui, "Macros", |ui| {
            settings_ui(ui, &m.settings);
            automation_list(ui, &m.automation, vals);
            modulation_list(ui, &m.modulation);
        });
    }
    if !n.snapshots.is_empty() {
        block(ui, "Snapshot instruments", |ui| {
            for s in &n.snapshots {
                ui.label(format!(
                    "{} at {}%{}",
                    s.snapshot.as_ref().map(|x| x.name.as_str()).unwrap_or("?"),
                    s.intensity.map(fmt).unwrap_or("?".into()),
                    s.track.as_ref().map(|t| format!(", track '{}'", t.name)).unwrap_or_default()
                ));
                automation_list(ui, &s.automation, vals);
            }
        });
    }
    if let Some(r) = &n.route {
        block(ui, "Route through the mixer", |ui| {
            for (i, s) in r.iter().enumerate() {
                let id = ui.make_persistent_id(("strip", &s.id));
                CollapsingState::load_with_default_open(ui.ctx(), id, false)
                    .show_header(ui, |ui| {
                        ui.label(format!("{}{}  {}", if i == 0 { "" } else { "then " }, if s.entity == "MixerInput" { "Event input".into() } else { s.label() }, s.volume.map(fmt_db).unwrap_or_default()));
                    })
                    .body(|ui| strip_ui(ui, s, vals));
            }
        });
    }
}

fn edge_details(ui: &mut Ui, g: &Graph, e: &Edge, vals: &Values, act: &mut Act) {
    use crate::kit::block;
    let (p, c) = (g.node(&e.parent_id), g.node(&e.child_id));
    ui.label(RichText::new("LINK").size(10.0).strong().color(graph::edge_colour(&e.kind)));
    heading(ui, &graph::link_text(g, e));
    let summary = crate::kit::link_summary(e);
    if !summary.is_empty() {
        crate::kit::pill(ui, summary, Color32::LIGHT_GRAY);
    }
    for n in [c, p].into_iter().flatten() {
        if ui.button(format!("{}: {}", clip(&n.name, 22), crate::kit::open_label(n))).on_hover_text(crate::kit::open_tip(n)).clicked() {
            act.open = Some(n.open_id());
        }
    }
    match e.kind.as_str() {
        "send" => {
            if let Some(fx) = &e.send {
                block(ui, "Send", |ui| {
                    ui.label(format!("Level {}{}", fx.level.map(fmt_db).unwrap_or("?".into()), if fx.bypass { ", bypassed" } else { "" }));
                    if let Some(pl) = &e.place {
                        ui.label(RichText::new(format!("On {pl}")).color(DIM));
                    }
                    settings_ui(ui, &fx.settings);
                    automation_list(ui, &fx.automation, vals);
                    modulation_list(ui, &fx.modulation);
                });
            }
            return;
        }
        "effect" => {
            if let Some(fx) = &e.effect {
                block(ui, &format!("{}{}", fx.kind, if fx.bypass { " (bypassed here)" } else { "" }), |ui| {
                    if let Some(pl) = &e.place {
                        ui.label(RichText::new(format!("On {pl}")).color(DIM));
                    }
                    settings_ui(ui, &fx.settings);
                    automation_list(ui, &fx.automation, vals);
                    modulation_list(ui, &fx.modulation);
                });
            }
            return;
        }
        "route" | "vca" => {
            for n in [c, p].into_iter().flatten() {
                if let Some(s) = &n.strip {
                    block(ui, &n.name, |ui| strip_ui(ui, s, vals));
                }
            }
            return;
        }
        _ => {}
    }
    if let Some(s) = &e.sheet {
        let where_ = match s.kind.as_str() {
            "timeline" => "on the timeline".to_string(),
            "parameter" => format!("on the '{}' parameter sheet", s.param.as_ref().map(|p| p.name.as_str()).unwrap_or("?")),
            k => format!("on a {k} sheet"),
        };
        ui.label(RichText::new(format!("{where_}, from {} for {}", s.start.map(fmt).unwrap_or("?".into()), s.length.map(fmt).unwrap_or("?".into()))).color(DIM));
    }
    block(ui, if e.kind == "asset" { "Audio instrument" } else { "Event instrument" }, |ui| instrument_ui(ui, &e.instrument, vals));
    if !e.instrument.params.is_empty() {
        block(ui, "Parameter values it passes in", |ui| {
            for p in &e.instrument.params {
                ui.label(format!("{} = {}", p.param.as_ref().map(|x| x.name.as_str()).unwrap_or("?"), p.value.map(fmt).unwrap_or("?".into())));
                automation_list(ui, &p.automation, vals);
                modulation_list(ui, &p.modulation);
            }
        });
    }
    for c in &e.containers {
        block(ui, &format!("Inside a {}", c.container.as_deref().unwrap_or("container")), |ui| {
            if let Some(n) = c.entries {
                ui.label(format!("{} entries{}", fmt(n), c.entry_percentage.map(|p| format!(", this one {}%", fmt(p))).unwrap_or_default()));
            }
            instrument_ui(ui, c, vals);
            settings_ui(ui, &c.settings);
        });
    }
    if let Some(t) = &e.track {
        block(ui, &format!("Track '{}'", t.name), |ui| match &t.strip {
            Some(s) => strip_ui(ui, s, vals),
            None => {
                ui.label(RichText::new("The parent's master track (see the parent event).").color(DIM));
            }
        });
    }
}

fn instrument_ui(ui: &mut Ui, i: &Instrument, vals: &Values) {
    egui::Grid::new(("inst", &i.id, i.container.clone().unwrap_or_default())).num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        if let Some(v) = i.volume {
            kv(ui, "Volume", fmt_db(v));
        }
        if let Some(v) = i.pitch {
            if v != 0.0 {
                kv(ui, "Pitch", format!("{} st", fmt(v)));
            }
        }
        if let Some(v) = i.intensity {
            kv(ui, "Intensity", format!("{}%", fmt(v)));
        }
        if i.probability_on {
            kv(ui, "Probability", format!("{}%", i.probability.map(fmt).unwrap_or("?".into())));
        }
        if let Some(d) = &i.delay {
            if d.max.is_some_and(|m| m > 0.0) {
                kv(ui, "Delay", format!("{}–{}", d.min.map(fmt).unwrap_or_default(), d.max.map(fmt).unwrap_or_default()));
            }
        }
        if i.start_offset.is_some_and(|v| v != 0.0) {
            kv(ui, "Start offset", format!("{}%", fmt(i.start_offset.unwrap_or(0.0))));
        }
        let flags: Vec<&str> = [(i.is_async, "async"), (i.cutoff, "cut-off"), (i.looping, "looping")].iter().filter(|x| x.0).map(|x| x.1).collect();
        if !flags.is_empty() {
            kv(ui, "Playback", flags.join(", "));
        }
        if i.quantization.is_some_and(|q| q != 0.0) {
            kv(ui, "Quantization", format!("code {}", fmt(i.quantization.unwrap_or(0.0))));
        }
    });
    if !i.conditions.is_empty() {
        ui.label(RichText::new(format!("Trigger conditions (mode code {})", i.condition_mode.map(fmt).unwrap_or("?".into()))).color(DIM));
        for c in &i.conditions {
            match (c.kind.as_str(), &c.param) {
                ("parameter", Some(p)) => {
                    let (now, _) = vals.param(p, Source::Now);
                    let met = now.map(|v| (c.min.is_none_or(|m| v >= m) && c.max.is_none_or(|m| v <= m)) != c.inverted);
                    let txt = format!(
                        "  {} {}–{}{}{}",
                        p.name,
                        c.min.map(fmt).unwrap_or_default(),
                        c.max.map(fmt).unwrap_or_default(),
                        if c.inverted { " (inverted)" } else { "" },
                        now.map(|v| format!("  · now {}", fmt(v))).unwrap_or_default()
                    );
                    ui.label(RichText::new(txt).color(match met {
                        Some(true) => GOOD,
                        Some(false) => BAD,
                        None => Color32::GRAY,
                    }));
                }
                ("event", _) => {
                    ui.label(format!("  Event state code {}", c.event_state.map(fmt).unwrap_or("?".into())));
                }
                (k, _) => {
                    ui.label(format!("  {k}"));
                }
            }
        }
    }
    automation_list(ui, &i.automation, vals);
    modulation_list(ui, &i.modulation);
}

fn strip_ui(ui: &mut Ui, s: &Strip, vals: &Values) {
    ui.label(format!("Fader {}{}", s.volume.map(fmt_db).unwrap_or("?".into()), s.pitch.filter(|p| *p != 0.0).map(|p| format!(", pitch {} st", fmt(p))).unwrap_or_default()));
    automation_list(ui, &s.automation, vals);
    modulation_list(ui, &s.modulation);
    for fx in s.effects.iter().filter(|f| !f.fader) {
        let mut t = if fx.name.is_empty() { fx.kind.clone() } else { format!("{} '{}'", fx.kind, fx.name) };
        if let Some(p) = &fx.preset {
            t += &format!(" (preset '{p}')");
        }
        if let Some(g) = fx.gain {
            t += &format!(" {}", fmt_db(g));
        }
        if let Some(l) = fx.level {
            t += &format!(" to '{}' at {}", fx.to.clone().unwrap_or_default(), fmt_db(l));
        }
        if let Some(sp) = &fx.spatial {
            t += &format!(" {}–{}{}", sp.min.map(fmt).unwrap_or_default(), sp.max.map(fmt).unwrap_or_default(), if sp.overrides { "" } else { " (event min/max)" });
        }
        let r = ui.label(RichText::new(format!("• {t}{}", if fx.bypass { "  (bypassed)" } else { "" })).color(if fx.bypass { DIM } else { Color32::LIGHT_GRAY }));
        if !fx.settings.is_empty() {
            r.on_hover_text(settings_text(&fx.settings));
        }
        automation_list(ui, &fx.automation, vals);
        modulation_list(ui, &fx.modulation);
    }
    for v in &s.vcas {
        ui.label(format!("• VCA '{}' {}", v.name, v.volume.map(fmt_db).unwrap_or_default()));
        automation_list(ui, &v.automation, vals);
    }
    for sn in &s.snapshots {
        let name = sn.snapshot.as_ref().map(|x| x.name.as_str()).unwrap_or("?");
        if sn.scoped {
            ui.label(RichText::new(format!("• Scoped by snapshot '{name}'")).color(DIM));
        } else {
            ui.label(RichText::new(format!("• Snapshot '{name}' sets {} to {}", sn.property, sn.value.map(|v| value_text(&sn.property, v)).unwrap_or("?".into()))).color(DIM));
        }
    }
}

fn automation_list(ui: &mut Ui, autos: &[Automation], vals: &Values) {
    for a in autos {
        automation_line(ui, a, vals);
    }
}

fn automation_line(ui: &mut Ui, a: &Automation, vals: &Values) {
    for c in &a.curves {
        let Some(p) = &c.param else { continue };
        let pts = c.points();
        if pts.is_empty() {
            ui.label(RichText::new(format!("  {} automated on {} (no points)", a.property, p.name)).color(DIM).small());
            continue;
        }
        let db = curve::is_db(&a.property);
        let at = |src| vals.param(p, src).0.and_then(|x| curve::value_at(&pts, x, db));
        let now = at(Source::Now).map(|v| value_text(&a.property, v)).unwrap_or("?".into());
        let def = at(Source::Default).map(|v| value_text(&a.property, v)).unwrap_or("?".into());
        let r = ui.label(RichText::new(format!("  {} automated on {}: now {now}, default {def}", a.property, p.name)).color(ORANGE).small());
        r.on_hover_text(pts.iter().map(|q| format!("at {}: {}{}", fmt(q.x), value_text(&a.property, q.y), shape_text(q.shape, q.s))).collect::<Vec<_>>().join("\n"));
    }
}

fn modulation_list(ui: &mut Ui, mods: &[Modulation]) {
    for m in mods {
        modulation_line(ui, m);
    }
}

fn modulation_line(ui: &mut Ui, m: &Modulation) {
    ui.label(RichText::new(format!("  {} on {}: {}", m.kind, m.property, settings_text(&m.settings).replace('\n', ", "))).color(Color32::from_rgb(150, 180, 230)).small());
}

fn settings_ui(ui: &mut Ui, s: &Map<String, Value>) {
    if s.is_empty() {
        return;
    }
    ui.label(RichText::new(settings_text(s).replace('\n', ", ")).color(DIM).small());
}

fn settings_text(s: &Map<String, Value>) -> String {
    s.iter()
        .map(|(k, v)| match v {
            Value::Number(n) => format!("{k} {}", n.as_f64().map(fmt).unwrap_or_default()),
            Value::Bool(b) => format!("{k} {}", if *b { "on" } else { "off" }),
            x => format!("{k} {x}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn value_text(property: &str, v: f64) -> String {
    if curve::is_db(property) { fmt_db(v) } else { fmt(v) }
}

fn shape_text(shape: f64, s: bool) -> String {
    match (shape == 0.0, s) {
        (true, false) => String::new(),
        (_, true) => format!(" (S-curve {})", fmt(shape)),
        (false, false) => format!(" (bend {})", fmt(shape)),
    }
}

fn label_of(p: &model::ParamInfo, v: f64) -> String {
    let i = v.round();
    if !p.labels.is_empty() && (v - i).abs() < 1e-6 && i >= 0.0 && (i as usize) < p.labels.len() { format!("{} ({})", fmt(v), p.labels[i as usize]) } else { fmt(v) }
}

fn short_version(v: &str) -> String {
    // "Version 2.03.14, 64-bit, ..." -> "FMOD 2.03.14"
    let v = v.trim_start_matches("Version ").split(',').next().unwrap_or(v);
    format!("FMOD {v}")
}

fn version_lt(a: &str, b: &str) -> bool {
    let p = |s: &str| s.split('.').map(|x| x.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    !a.is_empty() && p(a) < p(b)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(b) = &mut self.bridge {
            let lines = b.poll();
            let closed = b.closed;
            for l in lines {
                self.receive(&l);
            }
            if closed {
                self.link = Link::Lost;
            }
        }
        if self.applied_on_top != Some(self.on_top) {
            // Only on change: a viewport command every frame keeps waking the app.
            let level = if self.on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.applied_on_top = Some(self.on_top);
        }

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        // Draw from a taken focus, so widgets can ask for changes (applied after).
        let focus = self.focus.take();
        let extended = self.extended.take();
        let shown: Option<&Graph> = if self.further { extended.as_ref() } else { None }.or(focus.as_ref().and_then(|f| f.graph.as_ref()));
        let mut act = Act::default();
        if let Some(f) = &focus {
            if let Some(g) = &f.graph {
                egui::Panel::top("panes").show(ui, |ui| self.pane_bar(ui, g));
            }
            if !(f.graph.is_some() && self.tab == Tab::Gain) && f.kind != "none" {
                egui::Panel::right("details").resizable(true).default_size(380.0).min_size(260.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("details").auto_shrink(false).show(ui, |ui| self.details(ui, f, shown, &mut act));
                });
            }
        }
        egui::CentralPanel::default().show(ui, |ui| match &focus {
            None => {
                let msg = match self.link {
                    Link::Waiting if self.bridge.is_none() => "Open this from FMOD Studio: Scripts > DAT-audio Utilities > Open Reference Viewer...",
                    Link::Waiting => "Waiting for Studio…",
                    Link::Lost => "The link to Studio closed. Open the viewer again from Studio's Scripts menu.",
                    _ => SELECT_HINT,
                };
                ui.centered_and_justified(|ui| ui.label(RichText::new(msg).color(DIM)));
            }
            Some(f) => match (f.kind.as_str(), &f.graph) {
                (_, Some(g)) if self.tab == Tab::Gain && g.mode() == "event" => self.gain_view(ui, f),
                (_, Some(g)) => self.graph_view(ui, shown.unwrap_or(g), &mut act),
                ("parameter", _) => self.parameter_view(ui, f, &mut act),
                ("snapshot", _) => self.snapshot_view(ui, f, &mut act),
                ("error", _) => {
                    ui.label(RichText::new(format!("Studio couldn't read the selection: {}", f.error.clone().unwrap_or_default())).color(BAD));
                }
                _ => {
                    ui.centered_and_justified(|ui| ui.label(RichText::new(SELECT_HINT).color(DIM)));
                }
            },
        });
        self.focus = focus;
        self.extended = extended;
        self.apply(act);
        crate::guide::show(&ctx, &mut self.show_guide);

        let title = match self.focus.as_ref() {
            Some(f) => match (f.graph.as_ref().and_then(Graph::focus), &f.param, &f.snapshot) {
                (Some(n), _, _) => format!("Reference Viewer — {}", n.name),
                (_, Some(p), _) => format!("Reference Viewer — {}", p.name),
                (_, _, Some(s)) => format!("Reference Viewer — {}", s.name),
                _ => "Reference Viewer".into(),
            },
            None => "Reference Viewer".into(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        self.dev_screenshot(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(STORAGE_ON_TOP, if self.on_top { "1" } else { "0" }.into());
        storage.set_string(STORAGE_SORT, if self.by_length { "1" } else { "0" }.into());
        storage.set_string(STORAGE_CARDS, if crate::kit::cards() { "1" } else { "0" }.into());
        storage.set_string(STORAGE_FURTHER, if self.further { "1" } else { "0" }.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::fixtures::*;

    fn line(json: &str) -> String {
        serde_json::to_string(&serde_json::from_str::<Value>(json).unwrap()).unwrap()
    }

    #[test]
    fn messages_update_the_state() {
        let mut a = App::bare();
        a.receive(r#"{"type":"hello","studio":"Version 2.03.14, 64-bit","script":"0.1.0"}"#);
        assert!(matches!(&a.link, Link::Linked { studio, .. } if studio.contains("2.03.14")));
        a.receive(&line(EVENT_LEAF));
        assert_eq!(a.focus.as_ref().unwrap().kind, "event");
        a.sel = Some(Sel::Pick(Pick::Node("x".into())));
        // The same focus again (a re-read) keeps the selection; a new one clears it.
        a.receive(&line(EVENT_LEAF));
        assert!(a.sel.is_some());
        a.receive(&line(PARAMETER));
        assert!(a.sel.is_none());
        let id = a.focus.as_ref().unwrap().param.as_ref().unwrap().id.clone();
        a.receive(&format!(r#"{{"type":"cursors","values":{{"{id}":7.5}}}}"#));
        assert_eq!(a.cursors.get(&id), Some(&7.5));
    }

    #[test]
    fn open_sends_the_command() {
        let mut a = App::bare();
        a.open("{abc}", "A");
        assert_eq!(a.sent, vec![r#"{"cmd":"open","id":"{abc}"}"#.to_string()]);
    }

    #[test]
    fn versions_compare() {
        assert!(version_lt("0.0.9", "0.1.0"));
        assert!(!version_lt("0.1.0", "0.1.0"));
        assert!(!version_lt("", "0.1.0"));
    }
}
