//! The window: the link with Studio, the session at the playhead, and the
//! toolbar and HUD around the 3D view.
//!
//! The camera is the game's listener (the one chosen in CAMERA) at the
//! profiler's playhead, plus the CAMERA offset, because the game's real camera
//! isn't in the capture. Nothing moves it, except DETACHED CAMERA: a camera of
//! your own to fly around (as in Sandbox First Person) and see the listener's
//! view, drawn as a frustum, from outside. "Reset to listener" ends it.
//!
//! Time: the script sends the session timeline's cursor and playhead. While
//! the playhead moves (playback or recording) the view follows it, otherwise
//! it follows the cursor (clicking or scrubbing the timeline).

use crate::bridge::Bridge;
use crate::camera::{Camera, Free, Offset, len, sub};
use crate::capture::Activity;
use crate::events_window::{self, EventFilter};
use crate::link::{EventInfo, Msg, Session, parse};
use crate::session::{Loaded, Moment};
use crate::view::{self, Opts, SphereStyle};
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, pos2, vec2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How often the session folder is checked for new or growing recordings.
const REFRESH_EVERY: Duration = Duration::from_millis(500);
const EVENTS_PER_REQUEST: usize = 200;

/// "opts_v2" since 0.4.0: view options saved before then are dropped once,
/// so everyone starts from the new defaults.
const STORAGE_OPTS: &str = "opts_v2";
const STORAGE_PANEL: &str = "settings_open";
const STORAGE_OFFSET: &str = "offset";
const STORAGE_ON_TOP: &str = "on_top";
const STORAGE_FOV: &str = "fov";
const STORAGE_FILTER: &str = "event_filter";
const STORAGE_FLY: &str = "detached_fly";

/// DETACHED CAMERA controls (as in Sandbox First Person).
const LOOK_DEG_PER_PX: f32 = 0.15;
const TURN_DEG_PER_S: f32 = 90.0;
/// Flying speed, in grid squares (Scale) per second.
const SPEED_MIN: f32 = 0.25;
const SPEED_MAX: f32 = 1000.0;

/// DETACHED CAMERA settings that are kept between runs. Speed and cast
/// length are in grid squares (Scale), so they suit any game's units.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FlyOpts {
    pub speed: f32,
    /// The free camera's own field of view (the VIEW FOV is the listener's).
    pub fov: f32,
    /// How far the listener's view is drawn.
    pub cast: f32,
}

impl Default for FlyOpts {
    fn default() -> Self {
        FlyOpts { speed: 10.0, fov: 70.0, cast: 25.0 }
    }
}

/// Active (on) buttons are orange, with dark text (as in Sandbox First Person).
pub const ORANGE: Color32 = Color32::from_rgb(240, 140, 30);
const ON_ORANGE_TEXT: Color32 = Color32::from_rgb(28, 18, 6);
const SECTION: Color32 = Color32::from_rgb(130, 136, 146);

#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    /// Opened on a session folder from the command line (no Studio).
    Standalone,
    Waiting,
    Linked { studio: String },
    Lost,
}

/// Which of the script's two times the view follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Cursor,
    Playhead,
}

pub struct App {
    bridge: Option<Bridge>,
    pub link: Link,
    pub session: Option<Session>,
    session_known: bool,
    pub loaded: Option<Loaded>,
    last_refresh: Instant,
    pub events: HashMap<String, EventInfo>,
    requested: HashSet<String>,
    pub sent: Vec<String>,
    /// Session time shown, and where it came from.
    pub t: f64,
    pub source: Source,
    last_cursor: Option<f64>,
    last_playhead: Option<f64>,
    /// Ground height per recording (None until computed or when empty).
    grounds: Vec<Option<f32>>,
    /// The recording the auto scale was last picked for.
    scaled_for: Option<(usize, usize)>,
    pub opts: Opts,
    pub offset: Offset,
    pub filter: EventFilter,
    /// Events of the profiler tracks selected in Studio: while any are
    /// selected, only they are drawn (overrides the event filter).
    pub selected: Vec<String>,
    /// Instances per event in the session, and the total it was counted at.
    counts: HashMap<String, usize>,
    counted_at: usize,
    /// The listener the camera follows; None: Auto, the busiest one.
    pub follow: Option<usize>,
    /// DETACHED CAMERA: on, and the free camera (None: place it behind the
    /// listener's view on the next frame).
    pub detached: bool,
    pub free: Option<Free>,
    pub fly: FlyOpts,
    fov: f32,
    on_top: bool,
    /// The settings side panel.
    settings_open: bool,
    applied_on_top: Option<bool>,
    last_title: String,
    // Standalone playback.
    playing: bool,
    last_frame: Option<Instant>,
    screenshot: Option<PathBuf>,
    frames: u32,
}

pub fn apply_theme(ctx: &egui::Context) {
    // Always dark: a hand-tuned dark view under macOS Light mode goes dark-on-dark.
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.selection.bg_fill = ORANGE;
    v.selection.stroke = egui::Stroke::new(1.0, ON_ORANGE_TEXT);
    ctx.set_visuals(v);
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, bridge: bool, session_dir: Option<PathBuf>, time: Option<f64>, screenshot: Option<PathBuf>) -> Self {
        apply_theme(&cc.egui_ctx);
        // Development screenshots start from defaults, not the saved settings.
        let storage = if screenshot.is_some() { None } else { cc.storage };
        let get = |k: &str| storage.and_then(|s| s.get_string(k));
        let mut app = App::bare();
        app.opts = get(STORAGE_OPTS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        app.offset = get(STORAGE_OFFSET).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        app.on_top = get(STORAGE_ON_TOP).map_or(true, |s| s == "1");
        app.fov = get(STORAGE_FOV).and_then(|s| s.parse().ok()).unwrap_or(70.0);
        app.filter = get(STORAGE_FILTER).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        app.fly = get(STORAGE_FLY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        if get(STORAGE_OPTS).is_none() {
            // First run of 0.4.0: "Only ticked" starts off too (the ticks are kept).
            app.filter.on = false;
        }
        app.settings_open = get(STORAGE_PANEL).map_or(true, |s| s == "1");
        app.screenshot = screenshot;
        if bridge {
            let ctx = cc.egui_ctx.clone();
            app.bridge = Some(Bridge::stdio(move || ctx.request_repaint()));
            app.link = Link::Waiting;
        } else if let Some(dir) = session_dir {
            app.link = Link::Standalone;
            let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            app.set_session(Some(Session { id: name.clone(), name, dir: dir.to_string_lossy().to_string(), ..Default::default() }));
            app.t = time.unwrap_or(0.0);
        }
        if app.screenshot.is_some() {
            // Development: PFP_OFFSET="right,up,back,yaw,pitch", PFP_FOLLOW=n, PFP_IDLE=1 for screenshots.
            if let Some(v) = std::env::var("PFP_OFFSET").ok().map(|s| s.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>()) {
                if v.len() == 5 {
                    app.offset = Offset { right: v[0], up: v[1], back: v[2], yaw: v[3], pitch: v[4] };
                }
            }
            if let Some(n) = std::env::var("PFP_FOLLOW").ok().and_then(|s| s.parse().ok()) {
                app.follow = Some(n);
            }
            if std::env::var("PFP_IDLE").is_ok() {
                app.opts.idle = true;
            }
            // PFP_EVENTS=1 opens the events window, drawn inside the main one so it's in the picture;
            // PFP_CHILDREN=1 shows child events.
            if std::env::var("PFP_EVENTS").is_ok() {
                app.filter.open = true;
                cc.egui_ctx.set_embed_viewports(true);
            }
            if std::env::var("PFP_CHILDREN").is_ok() {
                app.opts.children = true;
            }
            // PFP_DETACH=1: DETACHED CAMERA on, placed behind the listener's view.
            if std::env::var("PFP_DETACH").is_ok() {
                app.detached = true;
            }
            // PFP_STYLE=lines|shell|both, PFP_OPACITY=0-1.
            match std::env::var("PFP_STYLE").as_deref() {
                Ok("shell") => app.opts.style = SphereStyle::Shell,
                Ok("both") => app.opts.style = SphereStyle::Both,
                Ok("lines") => app.opts.style = SphereStyle::Lines,
                _ => {}
            }
            if let Some(o) = std::env::var("PFP_OPACITY").ok().and_then(|v| v.parse().ok()) {
                app.opts.opacity = o;
            }
        }
        app
    }

    fn bare() -> Self {
        App {
            bridge: None,
            link: Link::Waiting,
            session: None,
            session_known: false,
            loaded: None,
            last_refresh: Instant::now(),
            events: HashMap::new(),
            requested: HashSet::new(),
            sent: Vec::new(),
            t: 0.0,
            source: Source::Cursor,
            last_cursor: None,
            last_playhead: None,
            grounds: Vec::new(),
            scaled_for: None,
            opts: Opts::default(),
            offset: Offset::default(),
            filter: EventFilter::default(),
            selected: Vec::new(),
            counts: HashMap::new(),
            counted_at: usize::MAX,
            follow: None,
            detached: false,
            free: None,
            fly: FlyOpts::default(),
            fov: 70.0,
            on_top: true,
            settings_open: true,
            applied_on_top: None,
            last_title: String::new(),
            playing: false,
            last_frame: None,
            screenshot: None,
            frames: 0,
        }
    }

    fn markers(&self) -> &[f64] {
        self.session.as_ref().map_or(&[], |s| &s.markers)
    }

    /// Handles one line from Studio.
    pub fn receive(&mut self, line: &str) {
        match parse(line) {
            Some(Msg::Hello { studio, .. }) => self.link = Link::Linked { studio },
            Some(Msg::Session(s)) => self.set_session(s),
            Some(Msg::Time { cursor, playhead }) => self.set_time(cursor, playhead),
            Some(Msg::Selection(ids)) => self.selected = ids,
            Some(Msg::Events(items)) => {
                for e in items {
                    self.events.insert(e.id.clone(), e);
                }
            }
            None => {}
        }
    }

    fn set_session(&mut self, s: Option<Session>) {
        self.session_known = true;
        let same_dir = matches!((&self.session, &s), (Some(a), Some(b)) if a.id == b.id && a.dir == b.dir && a.unsaved == b.unsaved);
        self.session = s;
        if same_dir {
            return; // only the markers or name changed
        }
        self.loaded = self.session.as_ref().filter(|s| !s.dir.is_empty()).map(|s| {
            let dirs = [&s.unsaved, &s.dir].into_iter().filter(|d| !d.is_empty()).map(PathBuf::from).collect();
            Loaded::open_dirs(dirs)
        });
        self.grounds.clear();
        self.scaled_for = None;
        self.last_refresh = Instant::now();
        self.request_events();
    }

    fn set_time(&mut self, cursor: f64, playhead: Option<f64>) {
        let playhead_moved = playhead.is_some() && self.last_playhead.is_some() && playhead != self.last_playhead;
        let cursor_moved = self.last_cursor != Some(cursor);
        if playhead_moved {
            self.source = Source::Playhead;
            self.t = playhead.unwrap_or(cursor);
        } else if self.last_cursor.is_none() {
            // First time: the playhead when there is one (the Profiler's
            // cursor can stay at 0).
            self.source = if playhead.is_some() { Source::Playhead } else { Source::Cursor };
            self.t = playhead.unwrap_or(cursor);
        } else if cursor_moved {
            self.source = Source::Cursor;
            self.t = cursor;
        }
        self.last_cursor = Some(cursor);
        self.last_playhead = playhead;
    }

    /// Asks Studio for the spatial settings of events not asked about yet.
    fn request_events(&mut self) {
        let Some(l) = &self.loaded else { return };
        let mut ids: Vec<String> = l.recs.iter().flat_map(|r| r.parser.rec.events()).filter(|e| !self.requested.contains(e)).collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            self.requested.insert(id.clone());
        }
        if self.link == Link::Standalone {
            return; // no Studio to ask
        }
        for chunk in ids.chunks(EVENTS_PER_REQUEST) {
            let line = serde_json::json!({ "cmd": "events", "ids": chunk }).to_string();
            if let Some(b) = &mut self.bridge {
                b.send(&line);
            }
            self.sent.push(line);
        }
    }

    fn refresh(&mut self) {
        if self.last_refresh.elapsed() < REFRESH_EVERY {
            return;
        }
        self.last_refresh = Instant::now();
        let changed = self.loaded.as_mut().is_some_and(|l| l.refresh());
        if changed {
            self.grounds.clear();
            self.request_events();
        }
    }

    fn ground(&mut self, rec: usize) -> Option<f32> {
        let l = self.loaded.as_ref()?;
        if self.grounds.len() != l.recs.len() {
            self.grounds = l.recs.iter().map(|r| r.parser.rec.ground()).collect();
        }
        self.grounds.get(rec).copied().flatten()
    }

    pub fn moment(&mut self) -> Option<Moment> {
        let (ri, ..) = self.loaded.as_ref()?.locate(self.t, self.markers())?;
        let ground = self.ground(ri);
        let mut m = self.loaded.as_ref()?.moment(self.t, self.markers(), self.opts.oneshot_hold as f64, ground)?;
        let before = m.instances.len();
        if self.selected.is_empty() {
            m.instances.retain(|i| self.filter.shows(&i.event));
        } else {
            // Selected profiler tracks: only those events (or the events
            // whose instruments play a selected child event).
            let sel = &self.selected;
            let events = &self.events;
            m.instances.retain(|i| {
                sel.contains(&i.event) || events.get(&i.event).is_some_and(|e| e.children.iter().any(|c| sel.contains(&c.id)))
            });
        }
        m.hidden = before - m.instances.len();
        Some(m)
    }

    /// Instances per event over the whole session (recounted when it grows).
    fn event_counts(&mut self) -> &HashMap<String, usize> {
        let total = self.loaded.as_ref().map_or(0, |l| l.recs.iter().map(|r| r.parser.rec.instances.len()).sum());
        if total != self.counted_at {
            self.counted_at = total;
            self.counts.clear();
            if let Some(l) = &self.loaded {
                for i in l.recs.iter().flat_map(|r| r.parser.rec.instances.iter()).filter(|i| !i.event.is_empty()) {
                    *self.counts.entry(i.event.clone()).or_default() += 1;
                }
            }
        }
        &self.counts
    }

    /// The event filter's own window, while it's open.
    fn events_window(&mut self, ctx: &egui::Context) {
        if !self.filter.open {
            return;
        }
        let linked = self.link != Link::Standalone;
        self.event_counts();
        let list = events_window::entries(&self.counts, &self.events, linked);
        let title = match &self.session {
            Some(s) => format!("Events to draw — {}", s.name),
            None => "Events to draw".to_string(),
        };
        let level = if self.on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
        let builder = egui::ViewportBuilder::default().with_title(title).with_inner_size([380.0, 560.0]).with_min_inner_size([260.0, 200.0]).with_window_level(level);
        let filter = &mut self.filter;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("pfp_events"), builder, |ui, _class| {
            egui::CentralPanel::default().show(ui, |ui| events_window::contents(ui, filter, &list));
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                filter.open = false;
            }
        });
    }

    /// DETACHED CAMERA controls. True while moving (keep repainting).
    fn fly_input(&mut self, ctx: &egui::Context, resp: &egui::Response, right_handed: bool) -> bool {
        use egui::Key;
        let Some(mut free) = self.free else { return false };
        let mut active = false;
        if resp.dragged() {
            let d = resp.drag_delta();
            free.look(d.x * LOOK_DEG_PER_PX, -d.y * LOOK_DEG_PER_PX);
            active = true;
        }
        if !ctx.egui_wants_keyboard_input() {
            let (dt, keys, mods, scroll) = ctx.input(|i| (i.stable_dt.min(0.1), i.keys_down.clone(), i.modifiers, i.smooth_scroll_delta.y));
            if resp.hovered() && scroll != 0.0 {
                self.fly.speed = (self.fly.speed * (1.0 + scroll * 0.004)).clamp(SPEED_MIN, SPEED_MAX);
            }
            let down = |k: Key| keys.contains(&k);
            let axis = |pos: bool, neg: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
            let fwd = axis(down(Key::W) || down(Key::ArrowUp), down(Key::S) || down(Key::ArrowDown));
            let right = axis(down(Key::D), down(Key::A));
            let up = axis(down(Key::E) || down(Key::Space), down(Key::Q) || down(Key::C));
            let turn = axis(down(Key::ArrowRight), down(Key::ArrowLeft));
            if fwd != 0.0 || right != 0.0 || up != 0.0 || turn != 0.0 {
                let mult = if mods.shift { 3.0 } else if mods.alt { 0.25 } else { 1.0 };
                let step = self.fly.speed * self.opts.scale * mult * dt;
                let cam = free.camera(self.fly.fov, right_handed);
                free.fly(&cam, fwd * step, right * step, up * step);
                free.look(turn * TURN_DEG_PER_S * dt, 0.0);
                active = true;
            }
        }
        self.free = Some(free);
        active
    }

    /// The listener index followed at this moment.
    pub fn follow_in(&self, m: &Moment) -> usize {
        self.follow.filter(|&i| i < m.listeners.len()).unwrap_or(m.busiest)
    }

    /// Auto scale: once per recording and followed listener, from the
    /// distances in view.
    fn auto_scale(&mut self, m: &Moment) {
        let follow = self.follow_in(m);
        if !self.opts.auto_scale || self.scaled_for == Some((m.rec, follow)) {
            return;
        }
        let Some(l) = m.listener(follow) else { return };
        let at = Moment::hearing_point(l);
        let mut d: Vec<f32> = m.instances.iter().map(|i| len(sub(i.pose.pos, at))).collect();
        d.sort_by(f32::total_cmp);
        // The median distance to the instances (a top-down game ~675 -> 10, a
        // metre-scale game ~11 -> 1); with none, the height above the floor.
        let typical = match (d.get(d.len() / 2).copied().filter(|x| *x > 0.0), m.ground) {
            (Some(x), _) => x,
            (None, Some(g)) if (l.pose.pos[1] - g).abs() > 0.0 => (l.pose.pos[1] - g).abs() * 4.0,
            _ => return,
        };
        self.opts.scale = view::pick_scale(typical);
        self.scaled_for = Some((m.rec, follow));
    }

    /// One row: the link status, and the buttons for the two panels.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (txt, col) = match &self.link {
                Link::Standalone => (format!("File: {}", self.session.as_ref().map_or("", |s| &s.name)), Color32::from_rgb(240, 210, 110)),
                Link::Waiting => ("Waiting for Studio…".into(), Color32::GRAY),
                Link::Linked { studio } => (
                    match &self.session {
                        Some(s) => format!("Linked: {}  ·  {}", s.name, short_version(studio)),
                        None => format!("Linked  ·  {}", short_version(studio)),
                    },
                    Color32::from_rgb(120, 230, 140),
                ),
                Link::Lost => ("Link closed".into(), Color32::from_rgb(255, 120, 120)),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut self.settings_open, "Settings").on_hover_text("Show or hide the settings panel");
                ui.toggle_value(&mut self.filter.open, "Events…").on_hover_text("The list of this session's events, to pick which to draw");
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(txt).color(col)).truncate());
                });
            });
        });
    }

    /// The settings, grouped, in a side panel that scrolls (never cut off).
    fn settings_panel(&mut self, ui: &mut egui::Ui, m: Option<&Moment>) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            heading(ui, "VIEW");
            ui.horizontal_wrapped(|ui| {
                ui.toggle_value(&mut self.on_top, "On top").on_hover_text("Keep this window above Studio");
                ui.toggle_value(&mut self.opts.grid, "Grid");
                ui.toggle_value(&mut self.opts.labels, "Labels");
                ui.toggle_value(&mut self.opts.minimap, "Map");
                ui.toggle_value(&mut self.opts.card, "Info card").on_hover_text("The card for the instance nearest the crosshair (centre of the view)");
            });
            egui::Grid::new("view_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("FOV");
                ui.add(egui::Slider::new(&mut self.fov, 20.0..=120.0).suffix("°").fixed_decimals(0));
                ui.end_row();
                ui.label("Scale");
                ui.horizontal(|ui| {
                    let step = (self.opts.scale * 0.01).max(0.0005);
                    let r = ui.add(egui::DragValue::new(&mut self.opts.scale).range(0.001..=10000.0).speed(step).max_decimals(3))
                        .on_hover_text("Grid spacing (1 and 5 steps) and marker sizes, in game units");
                    if r.changed() {
                        self.opts.auto_scale = false;
                    }
                    if ui.toggle_value(&mut self.opts.auto_scale, "Auto").on_hover_text("Pick the scale from the distances in the scene").changed() {
                        self.scaled_for = None;
                    }
                });
                ui.end_row();
            });

            heading(ui, "SHOW");
            ui.horizontal_wrapped(|ui| {
                ui.toggle_value(&mut self.opts.max, "Max").on_hover_text("Max distance spheres");
                ui.toggle_value(&mut self.opts.min, "Min").on_hover_text("Min distance spheres");
                ui.toggle_value(&mut self.opts.size, "Size").on_hover_text("Sound size spheres (spatializer Extent Mode: User)");
                ui.toggle_value(&mut self.opts.idle, "Idle").on_hover_text("Also show instances that exist but aren't playing (hollow)");
                ui.toggle_value(&mut self.opts.children, "Children").on_hover_text(
                    "Also draw the ranges of child events (played by an instance's event instruments, also in multi instruments and scatterers), dashed, at the instance. The capture doesn't say when they play.",
                );
            });

            heading(ui, "SPHERES");
            ui.horizontal_wrapped(|ui| {
                for (st, label, tip) in [
                    (SphereStyle::Lines, "Lines", "Wireframe spheres"),
                    (SphereStyle::Shell, "Shell", "Each sphere's surface, where its radius is, in the event's colour. Inactive ones (not playing, or you're beyond max) are a light mesh"),
                    (SphereStyle::Both, "Both", "Shell and lines"),
                ] {
                    ui.selectable_value(&mut self.opts.style, st, label).on_hover_text(tip);
                }
            });
            egui::Grid::new("sphere_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("Opacity");
                let mut pct = self.opts.opacity * 100.0;
                if ui.add(egui::Slider::new(&mut pct, 5.0..=100.0).suffix("%").fixed_decimals(0)).on_hover_text("Opacity of every sphere").changed() {
                    self.opts.opacity = pct / 100.0;
                }
                ui.end_row();
            });

            heading(ui, "EVENTS");
            ui.horizontal_wrapped(|ui| {
                ui.toggle_value(&mut self.filter.on, "Only ticked").on_hover_text("Draw only the events ticked in the Events window");
                ui.toggle_value(&mut self.filter.open, "Events…").on_hover_text("Open the list of this session's events, to pick which to draw");
            });
            if !self.selected.is_empty() {
                ui.label(RichText::new("Profiler tracks are selected in Studio: only those are drawn.").color(Color32::from_rgb(240, 210, 110)).small());
            }

            heading(ui, "CAMERA");
            let n = m.map_or(0, |m| m.listeners.len());
            if n > 1 {
                let busiest = m.map_or(0, |m| m.busiest);
                let text = match self.follow {
                    None => format!("Listener: Auto ({busiest})"),
                    Some(i) => format!("Listener {i}"),
                };
                egui::ComboBox::from_id_salt("follow")
                    .selected_text(text)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.follow, None, format!("Auto: the busiest ({busiest})"));
                        for i in 0..n {
                            ui.selectable_value(&mut self.follow, Some(i), format!("Listener {i}"));
                        }
                    })
                    .response
                    .on_hover_text("The game sets several listeners; the view follows this one. Auto: the one the game updates most");
            }
            ui.label(RichText::new("Offset from the listener (the game's camera isn't in the capture)").color(SECTION).small());
            let sp = (self.opts.scale * 0.05).max(0.001);
            egui::Grid::new("offset_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                for (label, v, tip) in [
                    ("Right", &mut self.offset.right, "Units to the listener's right"),
                    ("Up", &mut self.offset.up, "Units along the listener's up"),
                    ("Back", &mut self.offset.back, "Units behind the listener"),
                ] {
                    ui.label(label);
                    ui.add(egui::DragValue::new(v).speed(sp).max_decimals(2)).on_hover_text(tip);
                    ui.end_row();
                }
                ui.label("Turn");
                ui.add(egui::DragValue::new(&mut self.offset.yaw).range(-180.0..=180.0).speed(0.5).suffix("°")).on_hover_text("Extra turn to the right");
                ui.end_row();
                ui.label("Tilt");
                ui.add(egui::DragValue::new(&mut self.offset.pitch).range(-89.0..=89.0).speed(0.5).suffix("°")).on_hover_text("Extra tilt up (negative looks down)");
                ui.end_row();
            });
            if ui.add_enabled(!self.offset.is_zero(), egui::Button::new("At listener")).on_hover_text("Reset the offset: the view's camera is the listener").clicked() {
                self.offset = Offset::default();
            }

            heading(ui, "DETACHED CAMERA");
            ui.horizontal_wrapped(|ui| {
                let r = ui.toggle_value(&mut self.detached, "Detach").on_hover_text(
                    "Fly a camera of your own, to see the listener's view (its position and FOV) from outside. The listener's view is drawn in orange",
                );
                if r.changed() {
                    self.free = None;
                }
                if ui.add_enabled(self.detached, egui::Button::new("Reset to listener")).on_hover_text("Couple the view back to the listener").clicked() {
                    self.detached = false;
                    self.free = None;
                }
            });
            if self.detached {
                let sc = self.opts.scale;
                egui::Grid::new("fly_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label("Speed");
                    let mut v = self.fly.speed * sc;
                    let step = (v * 0.01).max(0.0005);
                    if ui.add(egui::DragValue::new(&mut v).speed(step).max_decimals(3).suffix("/s"))
                        .on_hover_text("Flying speed, in game units per second. Also: scroll over the view")
                        .changed()
                    {
                        self.fly.speed = (v / sc).clamp(SPEED_MIN, SPEED_MAX);
                    }
                    ui.end_row();
                    ui.label("Lens");
                    ui.add(egui::Slider::new(&mut self.fly.fov, 20.0..=120.0).suffix("°").fixed_decimals(0))
                        .on_hover_text("This camera's own field of view. VIEW > FOV is the listener's, drawn in orange");
                    ui.end_row();
                    ui.label("Cast");
                    let mut v = self.fly.cast * sc;
                    let step = (v * 0.01).max(0.0005);
                    if ui.add(egui::DragValue::new(&mut v).speed(step).max_decimals(2))
                        .on_hover_text("How far out the listener's view is drawn, in game units")
                        .changed()
                    {
                        self.fly.cast = (v / sc).max(0.1);
                    }
                    ui.end_row();
                });
                if ui.button("Behind the listener").on_hover_text("Put this camera back behind and above the listener, looking at it").clicked() {
                    self.free = None;
                }
                ui.label(
                    RichText::new("Drag the view to look. W A S D fly, E / Q up and down, arrows turn. Shift faster, Option slower; scroll changes speed.")
                        .color(SECTION)
                        .small(),
                );
            }
        });
    }

    fn hud(&self, ui: &egui::Ui, rect: egui::Rect, m: Option<&Moment>, focused: Option<usize>) {
        let p = ui.painter_at(rect);
        let small = FontId::proportional(12.0);
        let dim = Color32::from_rgb(150, 156, 166);
        let loaded = self.loaded.as_ref();
        let msg: Option<String> = match (&self.link, &self.session) {
            (Link::Lost, _) => Some("Studio closed the link.\nReopen it from Scripts > DAT-audio Utilities > Open Profiler First Person View...".into()),
            (Link::Waiting, _) => Some("Waiting for Studio…".into()),
            (Link::Linked { .. }, None) if self.session_known => Some("Open a profiler session in Studio (Window > Profiler) and select it.".into()),
            (_, Some(s)) if loaded.is_none_or(|l| l.recs.is_empty()) => Some(format!(
                "No recordings with API capture in this session yet.\nRecord with the profiler (its API capture saves apicommands-NNNN.txt in\n{}).",
                s.dir
            )),
            _ => match m {
                None => None,
                Some(m) if m.gap => Some("Between recordings: nothing was captured here.".into()),
                Some(m) if m.listener(self.follow_in(m)).is_none() => Some("No listener set yet at this point of the recording.".into()),
                _ => None,
            },
        };
        if let Some(t) = &msg {
            p.text(rect.center(), Align2::CENTER_CENTER, t, FontId::proportional(16.0), Color32::from_rgb(220, 222, 226));
        }
        if let (Some(m), Some(l)) = (m, loaded) {
            let playing = m.instances.iter().filter(|i| i.activity != Activity::Idle).count();
            let idle = m.instances.len() - playing;
            let src = match (&self.link, self.source) {
                (Link::Standalone, _) => "",
                (_, Source::Playhead) => " (playhead)",
                (_, Source::Cursor) => " (cursor)",
            };
            let mut line = format!(
                "{:.2} s{src}  ·  recording {}/{}{} at {:.2} s  ·  {playing} playing, {idle} idle{}",
                self.t,
                m.rec + 1,
                l.recs.len(),
                if l.recs[m.rec].live() { " (live)" } else { "" },
                m.local,
                if m.unplaced > 0 { format!(", {} without a position", m.unplaced) } else { String::new() }
            );
            if !self.selected.is_empty() {
                let n = self.selected.len();
                line += &format!("  ·  only the {n} profiler track{} selected in Studio", if n == 1 { "" } else { "s" });
            } else if m.hidden > 0 {
                line += &format!(", {} hidden by the event filter", m.hidden);
            } else if self.filter.on && m.instances.is_empty() {
                line += "  ·  event filter on: tick events in Events… to draw them";
            }
            if let Some(lk) = m.listener(self.follow_in(m)) {
                let pos = lk.pose.pos;
                let which = if m.listeners.len() > 1 { format!(" {}", self.follow_in(m)) } else { String::new() };
                line += &format!("\nlistener{which} {:.1}, {:.1}, {:.1}", pos[0], pos[1], pos[2]);
                if let Some(at) = lk.atten {
                    let d = len(sub(at, pos));
                    if d > 0.01 * self.opts.scale {
                        line += &format!("  ·  probe {} from the listener: distances are measured from the probe", view::fmt_dist(d));
                    }
                }
            }
            if m.right_handed {
                line += "  ·  right-handed";
            }
            p.text(rect.left_top() + vec2(10.0, 8.0), Align2::LEFT_TOP, line, small.clone(), dim);
        }
        // Info card for the instance under the crosshair.
        if !self.opts.card {
            return;
        }
        let (Some(i), Some(m)) = (focused, m) else { return };
        let li = &m.instances[i];
        let info = self.events.get(&li.event);
        let hearing = m.listener(self.follow_in(m)).map(Moment::hearing_point).unwrap_or(li.pose.pos);
        let dist = len(sub(li.pose.pos, hearing));
        let sp = view::effective(info, li);
        let zone = view::zone(info, sp.as_ref(), dist);
        let state = match li.activity {
            Activity::Playing => "playing",
            Activity::OneShot => "released one-shot (end not captured)",
            Activity::Idle => "idle (not playing)",
        };
        let mut text = format!("{}\n", view::label_for(info, li));
        if let Some(inf) = info.filter(|i| !i.path.is_empty()) {
            text += &format!("{}\n", inf.path);
        }
        text += &format!("{} · {} · {state}\ninstance {} · {}", view::fmt_dist(dist), zone.label(), li.handle, li.event);
        if li.min_override.is_some() || li.max_override.is_some() {
            text += &format!(
                "\ngame overrides: min {} · max {}",
                li.min_override.map_or("-".into(), view::fmt_dist),
                li.max_override.map_or("-".into(), view::fmt_dist)
            );
        }
        if let Some(s) = &sp {
            for d in &s.distance_driven {
                let range = if d.normalized { "Distance (Normalized)".to_string() } else { format!("Distance 0-{}", view::fmt_dist(d.max)) };
                text += &format!("\nfollows {range}: {}", d.what);
            }
        }
        if let Some(inf) = info.filter(|i| !i.children.is_empty()) {
            text += &format!("\nchild events{}:", if self.opts.children { "" } else { " (SHOW Children to draw)" });
            for c in inf.children.iter().take(8) {
                let range = c.spatial.as_ref().filter(|s| s.is_3d).map_or("no own spatializer".to_string(), |s| format!("min {} · max {}", view::fmt_dist(s.min), view::fmt_dist(s.max)));
                let indent = "   ".repeat(c.depth.max(1) as usize);
                text += &format!("\n{indent}{}{} · {range}", c.name, if c.scattered { " (scattered)" } else { "" });
            }
            if inf.children.len() > 8 {
                text += &format!("\n   … and {} more", inf.children.len() - 8);
            }
        }
        if let Some(s) = sp.as_ref().filter(|s| s.is_3d) {
            for (k, x) in std::iter::once(s).chain(s.others.iter()).enumerate() {
                let kind = if x.spatializer == "object" { "object spatializer" } else { "spatializer" };
                let preset = if x.preset.is_empty() { String::new() } else { format!(" (preset {})", x.preset) };
                let range = if x.override_range { "own range" } else { "event range" };
                text += &format!("\n{}{}: {kind}{preset}", if k == 0 { "" } else { "+ " }, if x.track.is_empty() { "?" } else { &x.track });
                text += &format!("\n   min {} · max {} ({range}) · {}", view::fmt_dist(x.min), view::fmt_dist(x.max), x.rolloff_name());
                text += &match x.extent_mode {
                    1 => format!("\n   sound size {} · min extent {:.0}°", view::fmt_dist(x.size), x.min_extent),
                    md => format!("\n   sound size: {}", if md == 0 { "Auto" } else { "Off" }),
                };
            }
        }
        let font = FontId::proportional(13.0);
        let galley = p.layout(text, font, Color32::from_rgb(230, 232, 236), 380.0);
        let at = rect.right_bottom() - vec2(galley.size().x + 22.0, galley.size().y + 22.0);
        let card = egui::Rect::from_min_size(at, galley.size() + vec2(16.0, 16.0));
        p.rect_filled(card, 6.0, Color32::from_rgba_unmultiplied(10, 12, 16, 225));
        p.rect_stroke(card, 6.0, egui::Stroke::new(1.5, view::event_colour(info, &li.event)), egui::StrokeKind::Inside);
        p.galley(card.min + vec2(8.0, 8.0), galley, Color32::WHITE);
    }

    /// Standalone only: a time slider and play button (Studio drives time otherwise).
    fn time_bar(&mut self, ui: &mut egui::Ui) {
        let total = self.loaded.as_ref().map_or(0.0, |l| l.total_len(self.markers()));
        ui.horizontal(|ui| {
            if ui.button(if self.playing { "Pause" } else { "Play" }).clicked() {
                self.playing = !self.playing;
                self.last_frame = None;
            }
            ui.style_mut().spacing.slider_width = (ui.available_width() - 90.0).max(100.0);
            ui.add(egui::Slider::new(&mut self.t, 0.0..=total.max(0.001)).suffix(" s").max_decimals(2));
        });
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
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Err(e) = crate::png::write_rgba(&path, image.size[0] as u32, image.size[1] as u32, &rgba) {
                eprintln!("screenshot failed: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// A small dim label that starts a toolbar section.
/// A section heading in the settings panel.
fn heading(ui: &mut egui::Ui, name: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(name).size(11.0).strong().color(SECTION));
    ui.separator();
}

fn short_version(v: &str) -> String {
    // "Version 2.03.14, 64-bit, ..." -> "FMOD 2.03.14"
    let v = v.trim_start_matches("Version ").split(',').next().unwrap_or(v);
    format!("FMOD {v}")
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
        self.refresh();
        if self.playing {
            let now = Instant::now();
            if let Some(prev) = self.last_frame {
                self.t += now.duration_since(prev).as_secs_f64();
            }
            self.last_frame = Some(now);
            let total = self.loaded.as_ref().map_or(0.0, |l| l.total_len(self.markers()));
            if self.t >= total {
                self.t = total;
                self.playing = false;
            }
            ctx.request_repaint();
        }
        if self.applied_on_top != Some(self.on_top) {
            // Only on change: a viewport command every frame keeps waking the app.
            let level = if self.on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.applied_on_top = Some(self.on_top);
        }

        let moment = self.moment();
        if let Some(m) = &moment {
            self.auto_scale(m);
        }
        let follow = moment.as_ref().map_or(0, |m| self.follow_in(m));
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        if self.settings_open {
            egui::Panel::right("settings").resizable(true).default_size(250.0).min_size(200.0).show(ui, |ui| self.settings_panel(ui, moment.as_ref()));
        }
        if self.link == Link::Standalone {
            egui::Panel::bottom("time").show(ui, |ui| self.time_bar(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(view::BG)).show(ui, |ui| {
            let sense = if self.detached { Sense::click_and_drag() } else { Sense::hover() };
            let (resp, painter) = ui.allocate_painter(ui.available_size(), sense);
            let rect = resp.rect;
            let rh = moment.as_ref().is_some_and(|m| m.right_handed);
            let listener_cam = moment.as_ref().and_then(|m| m.listener(follow)).map(|l| Camera::from_pose(&l.pose, &self.offset, self.fov, rh));
            if self.detached {
                if let (None, Some(lc)) = (self.free, &listener_cam) {
                    // As far back as the cast is long, so the whole of it is in sight.
                    self.free = Some(Free::behind(lc, self.fly.cast * self.opts.scale));
                }
                if self.fly_input(&ctx, &resp, rh) {
                    ctx.request_repaint();
                }
            }
            let free_cam = self.free.filter(|_| self.detached).map(|f| f.camera(self.fly.fov, rh));
            let cam = free_cam.or(listener_cam);
            // The listener's view as the attached view would show it in this window.
            let cast = free_cam.and(listener_cam).map(|lc| lc.frustum(rect.width() / rect.height().max(1.0), self.fly.cast * self.opts.scale));
            let mut focused = None;
            if let (Some(cam), Some(m)) = (cam, &moment) {
                let scene = view::Scene { moment: m, events: &self.events, follow, offset: self.offset.moves() || free_cam.is_some(), cast };
                focused = view::draw(&painter, rect, &cam, Some(&scene), &self.opts).focused;
                if self.opts.minimap && !m.gap {
                    let side = (rect.width().min(rect.height()) * 0.3).clamp(120.0, 220.0);
                    let map = egui::Rect::from_min_size(pos2(rect.right() - side - 10.0, rect.top() + 10.0), vec2(side, side));
                    view::minimap(&painter, map, &cam, Some(&scene), &self.opts);
                }
            } else {
                painter.rect_filled(rect, 0.0, view::BG);
            }
            self.hud(ui, rect, moment.as_ref(), focused);
            if free_cam.is_some() {
                let p = ui.painter_at(rect);
                p.text(
                    rect.left_bottom() + vec2(10.0, -8.0),
                    Align2::LEFT_BOTTOM,
                    "Detached camera: drag to look, W A S D fly, E / Q up and down. Reset to listener in the settings.",
                    FontId::proportional(12.0),
                    view::VIEW_CAST,
                );
            }
        });
        self.events_window(&ctx);
        // Keep checking the session folder for new or growing recordings.
        if self.loaded.is_some() {
            ctx.request_repaint_after(REFRESH_EVERY);
        }

        let title = match &self.session {
            Some(s) => format!("Profiler First Person — {}", s.name),
            None => "Profiler First Person".to_string(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        self.dev_screenshot(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.screenshot.is_some() {
            return; // development screenshots never touch the saved settings
        }
        storage.set_string(STORAGE_OPTS, serde_json::to_string(&self.opts).unwrap_or_default());
        storage.set_string(STORAGE_OFFSET, serde_json::to_string(&self.offset).unwrap_or_default());
        storage.set_string(STORAGE_ON_TOP, if self.on_top { "1" } else { "0" }.into());
        storage.set_string(STORAGE_FOV, self.fov.to_string());
        storage.set_string(STORAGE_PANEL, if self.settings_open { "1" } else { "0" }.into());
        storage.set_string(STORAGE_FILTER, serde_json::to_string(&self.filter).unwrap_or_default());
        storage.set_string(STORAGE_FLY, serde_json::to_string(&self.fly).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linked_with(dir: &std::path::Path) -> App {
        let mut a = App::bare();
        a.receive(r#"{"type":"hello","studio":"Version 2.03.14, 64-bit","script":"0.1.0","project":""}"#);
        a.receive(&serde_json::json!({"type":"session","id":"{s}","name":"S","dir":dir,"markers":[0.0]}).to_string());
        a
    }

    /// A session folder per test (tests run in parallel).
    fn session_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pfp_app_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("apicommands-0000.txt"),
            "System::getEvent, {e}, 1\nSystem::setListenerAttributes, 0, {{0,0,0},{0,0,0},{0,0,1},{0,1,0}}, {0,0,0}, false\n\
             EventDescription::createInstance, 1, 10\nEventInstance::set3DAttributes, 10, {{0,0,5},{0,0,0},{0,0,1},{0,1,0}}\nEventInstance::start, 10\nPlayback::frame, 96000\n",
        )
        .unwrap();
        d
    }

    #[test]
    fn a_session_loads_its_recordings_and_asks_for_its_events_once() {
        let mut a = linked_with(&session_dir("load"));
        assert_eq!(a.loaded.as_ref().unwrap().recs.len(), 1);
        assert_eq!(a.sent, vec![r#"{"cmd":"events","ids":["{e}"]}"#.to_string()]);
        a.request_events();
        assert_eq!(a.sent.len(), 1, "not asked twice");
        a.receive(r#"{"type":"events","items":[{"id":"{e}","name":"Boom","spatial":{"min":1,"max":20,"is3D":true}}]}"#);
        assert_eq!(a.events["{e}"].name, "Boom");
        a.receive(r#"{"type":"time","cursor":1.0,"playhead":0}"#);
        let m = a.moment().unwrap();
        assert_eq!(m.instances.len(), 1);
    }

    #[test]
    fn follows_the_playhead_while_it_moves_else_the_cursor() {
        let mut a = linked_with(&session_dir("time"));
        a.receive(r#"{"type":"time","cursor":1.0,"playhead":0}"#);
        assert_eq!((a.t, a.source), (0.0, Source::Playhead), "starts at the playhead");
        a.receive(r#"{"type":"time","cursor":1.0,"playhead":0.5}"#);
        assert_eq!((a.t, a.source), (0.5, Source::Playhead));
        a.receive(r#"{"type":"time","cursor":1.0,"playhead":0.6}"#);
        assert_eq!(a.t, 0.6);
        a.receive(r#"{"type":"time","cursor":1.7,"playhead":0.6}"#);
        assert_eq!((a.t, a.source), (1.7, Source::Cursor), "a click on the timeline");
    }

    #[test]
    fn a_marker_update_keeps_the_loaded_session() {
        let d = session_dir("markers");
        let mut a = linked_with(&d);
        a.loaded.as_mut().unwrap().recs.clear();
        a.receive(&serde_json::json!({"type":"session","id":"{s}","name":"S","dir":d,"markers":[0.0, 5.0]}).to_string());
        assert!(a.loaded.as_ref().unwrap().recs.is_empty(), "not reloaded");
        assert_eq!(a.markers(), &[0.0, 5.0]);
    }

    #[test]
    fn the_event_filter_hides_unticked_events_and_counts_them() {
        let mut a = linked_with(&session_dir("filter"));
        a.receive(r#"{"type":"time","cursor":1.0,"playhead":0}"#);
        assert_eq!(a.moment().unwrap().instances.len(), 1);
        a.filter.on = true;
        let m = a.moment().unwrap();
        assert_eq!((m.instances.len(), m.hidden), (0, 1), "on with nothing ticked: nothing drawn");
        a.filter.checked.insert("{e}".into());
        assert_eq!(a.moment().unwrap().instances.len(), 1);
        assert_eq!(a.event_counts().get("{e}"), Some(&1));
    }

    #[test]
    fn selected_profiler_tracks_override_the_filter() {
        let mut a = linked_with(&session_dir("selection"));
        a.receive(r#"{"type":"time","cursor":1.0,"playhead":0}"#);
        a.filter.on = true; // nothing ticked
        a.receive(r#"{"type":"selection","ids":["{e}"]}"#);
        assert_eq!(a.moment().unwrap().instances.len(), 1, "selected: drawn even though not ticked");
        a.receive(r#"{"type":"selection","ids":["{other}"]}"#);
        assert_eq!(a.moment().unwrap().instances.len(), 0, "only the selected tracks");
        a.receive(r#"{"type":"events","items":[{"id":"{e}","children":[{"id":"{other}","parent":"{e}"}]}]}"#);
        assert_eq!(a.moment().unwrap().instances.len(), 1, "a selected child event shows its parent instance");
        a.receive(r#"{"type":"selection","ids":[]}"#);
        assert_eq!(a.moment().unwrap().instances.len(), 0, "selection cleared: back to the filter");
    }

    #[test]
    fn theme_is_always_dark() {
        let ctx = egui::Context::default();
        apply_theme(&ctx);
        assert_eq!(ctx.options(|o| o.theme_preference), egui::ThemePreference::Dark);
    }
}
