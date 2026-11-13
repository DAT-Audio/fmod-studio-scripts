//! The window: input, the sync with Studio, and the HUD around the 3D view.
//!
//! Sync rules:
//! - Studio is the source of truth for the listener. The camera's X/Z/yaw
//!   follow every listener update from Studio, except while you are moving
//!   here (and for `ADOPT_AFTER` after), so your own moves don't fight their
//!   echoes.
//! - Moves here are sent at most every `SEND_EVERY` (the script writes at most
//!   once per timer tick anyway). The Sandbox has no height: with Elevation
//!   on (off by default), height is faked by moving distance-reactive emitters
//!   along the ground (lift.rs); with it off the camera stays at ear height.

use crate::auto::{self, Centre, Circle, Face, Spin};
use crate::bridge::Bridge;
use crate::camera::{Camera, wrap_deg};
use crate::lift;
use crate::scene::{Listener, Msg, Scene, parse};
use crate::view::{self, Opts};
use eframe::egui::{self, Align2, Color32, FontId, Key, RichText, Sense, pos2, vec2};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SEND_EVERY: Duration = Duration::from_millis(33);
const ADOPT_AFTER: Duration = Duration::from_millis(400);
/// Studio's pose within this of what we sent is Studio adjusting our own
/// write (e.g. after the undo fold), not someone moving the listener.
/// Studio's echoes measure 0.000 m / 0.00 deg (rounding only), so this is
/// tiny. Projects with small units make 0.25 m a real move.
const OWN_POSE_M: f32 = 0.01;
const OWN_POSE_DEG: f32 = 0.5;
/// A Studio move further than this while elevated is an outside move (a
/// Sandbox drag, an Undo): back to ear height.
const OUTSIDE_MOVE_M: f32 = 0.01;
const LOOK_DEG_PER_PX: f32 = 0.15;
const SPEED_MIN: f32 = 0.01;
const SPEED_MAX: f32 = 100.0;
const TURN_DEG_PER_S: f32 = 90.0;

const STORAGE_OPTS: &str = "opts";
const STORAGE_SPEED: &str = "speed";
const STORAGE_ON_TOP: &str = "on_top";
const STORAGE_FOV: &str = "fov";
const STORAGE_HELP: &str = "help";
const STORAGE_AUTO: &str = "auto";

/// Active (on) buttons are orange, with dark text.
pub const ORANGE: Color32 = Color32::from_rgb(240, 140, 30);
const ON_ORANGE_TEXT: Color32 = Color32::from_rgb(28, 18, 6);
const SECTION: Color32 = Color32::from_rgb(130, 136, 146);

/// Shown before Elevation is turned on, so its side effects are explicit.
const ELEVATION_NOTES: &[&str] = &[
    "You'll see emitters move in the Sandbox window while you're up. They go back to their real positions when you return to ear height (R), turn Elevation off, switch scene or close this view.",
    "Only emitters that react to distance move: a spatializer with a distance curve, or something automated on the built-in Distance parameter. 2D events, curve-Off spatializers and snapshots stay put.",
    "Custom height parameters (e.g. CameraAltitude) aren't changed.",
    "Don't save the project while you're elevated: the moved positions would be saved. Undo puts them back.",
    "The listener and emitter moves of a session are one undo step.",
];

#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Demo,
    Waiting,
    Linked { studio: String },
    Lost,
}

pub struct App {
    bridge: Option<Bridge>,
    pub link: Link,
    pub scene: Option<Scene>,
    pub scene_known: bool,
    pub cam: Camera,
    synced: bool,
    last_local_move: Option<Instant>,
    dirty: bool,
    last_sent: Instant,
    pub sent: Vec<String>,
    opts: Opts,
    speed: f32,
    on_top: bool,
    applied_on_top: Option<bool>,
    help: bool,
    last_title: String,
    screenshot: Option<PathBuf>,
    frames: u32,
    /// Development: SFP_TEST_MOVE="x,z,rot" is sent once when linked to a
    /// listener (for driving the bridge by hand from a shell pipe).
    test_move: Option<(f32, f32, f32)>,
    // Automatic movements (auto.rs). Any manual move stops them.
    pub spin: Spin,
    pub circle: Option<Circle>,
    auto: auto::Settings,
    /// The emitter under the crosshair last frame (X, Z), to orbit.
    focus_pos: Option<(f32, f32, String)>,
    auto_hint: Option<String>,
    /// The pose last sent to (or adopted from) Studio.
    pose_sent: Option<(f32, f32, f32)>,
    /// How many of Studio's small adjustments were ignored (logged, capped).
    ignored_adjustments: u32,
    /// Elevation: front/back memory per emitter (lift.rs), where emitters
    /// should be now, what Studio was last told, and whether to tell it again.
    lift_sides: lift::Sides,
    lift_target: Vec<(String, f32, f32)>,
    lift_sent: Vec<(String, f32, f32)>,
    lift_dirty: bool,
    /// The explanation shown before Elevation is turned on.
    confirm_elevation: bool,
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
    pub fn new(cc: &eframe::CreationContext<'_>, bridge: bool, demo: bool, screenshot: Option<PathBuf>) -> Self {
        apply_theme(&cc.egui_ctx);
        let get = |k: &str| cc.storage.and_then(|s| s.get_string(k));
        let mut app = App::bare();
        app.opts = get(STORAGE_OPTS).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        app.speed = get(STORAGE_SPEED).and_then(|s| s.parse().ok()).unwrap_or(4.0);
        app.on_top = get(STORAGE_ON_TOP).map_or(true, |s| s == "1");
        app.auto = get(STORAGE_AUTO).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        app.help = get(STORAGE_HELP).map_or(true, |s| s == "1");
        app.cam.fov = get(STORAGE_FOV).and_then(|s| s.parse().ok()).unwrap_or(70.0);
        app.screenshot = screenshot;
        app.test_move = std::env::var("SFP_TEST_MOVE").ok().and_then(|s| {
            let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
            (v.len() == 3).then(|| (v[0], v[1], v[2]))
        });
        if bridge {
            let ctx = cc.egui_ctx.clone();
            app.bridge = Some(Bridge::stdio(move || ctx.request_repaint()));
            app.link = Link::Waiting;
        } else if demo {
            app.link = Link::Demo;
            app.scene = Some(crate::scene::demo());
            app.scene_known = true;
            app.synced = true;
            // Development screenshots: SFP_ELEVATION=1 turns Elevation on,
            // SFP_ELEVATION=dialog shows its explanation.
            match std::env::var("SFP_ELEVATION").as_deref() {
                Ok("dialog") => app.confirm_elevation = true,
                Ok(_) => app.opts.elevation = true,
                Err(_) => {}
            }
            // SFP_CAM="x,y,z,yaw,pitch" places the camera (development screenshots).
            if let Some(v) = std::env::var("SFP_CAM").ok().map(|s| s.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>()) {
                if v.len() == 5 {
                    app.cam = Camera { x: v[0], y: v[1], z: v[2], yaw: v[3], pitch: v[4], fov: app.cam.fov };
                }
            }
        }
        app
    }

    fn bare() -> Self {
        App {
            bridge: None,
            link: Link::Waiting,
            scene: None,
            scene_known: false,
            cam: Camera::default(),
            synced: false,
            last_local_move: None,
            dirty: false,
            last_sent: Instant::now(),
            sent: Vec::new(),
            opts: Opts::default(),
            speed: 4.0,
            on_top: true,
            applied_on_top: None,
            help: true,
            last_title: String::new(),
            screenshot: None,
            frames: 0,
            test_move: None,
            spin: Spin::Off,
            circle: None,
            auto: auto::Settings::default(),
            focus_pos: None,
            auto_hint: None,
            pose_sent: None,
            ignored_adjustments: 0,
            lift_sides: Default::default(),
            lift_target: Vec::new(),
            lift_sent: Vec::new(),
            lift_dirty: false,
            confirm_elevation: false,
        }
    }

    /// Handles one line from Studio.
    pub fn receive(&mut self, line: &str) {
        match parse(line) {
            Some(Msg::Hello { studio, .. }) => self.link = Link::Linked { studio },
            Some(Msg::Scene(sc)) => {
                if sc.as_ref().map(|s| &s.id) != self.scene.as_ref().map(|s| &s.id) {
                    self.synced = false; // a different scene: jump to its listener
                }
                self.scene = sc;
                self.scene_known = true;
            }
            Some(Msg::Listener(l)) => self.adopt(l),
            None => {}
        }
    }

    fn driving(&self) -> bool {
        self.dirty || self.last_local_move.is_some_and(|t| t.elapsed() < ADOPT_AFTER)
    }

    fn adopt(&mut self, l: Listener) {
        if self.synced && self.driving() {
            return;
        }
        let rot = wrap_deg(l.rot);
        if let (true, Some((px, pz, pr))) = (self.synced, self.pose_sent) {
            let moved = ((l.x - px).powi(2) + (l.z - pz).powi(2)).sqrt();
            let turned = angle_diff(rot, pr);
            if moved < OWN_POSE_M && turned < OWN_POSE_DEG {
                // Studio settling our own write (grid snap, auto-rotate after
                // the undo fold): keep our pose. Seen as random snaps to ear
                // height in 0.1.4.
                if self.ignored_adjustments < 20 {
                    eprintln!("ignored a Studio adjustment of our listener pose: {moved:.3} m, {turned:.2} deg");
                }
                self.ignored_adjustments += 1;
                return;
            }
            if self.elevated() {
                if moved <= OUTSIDE_MOVE_M {
                    // Turned in Studio, not moved: take the facing, keep height and position.
                    eprintln!("Studio turned the listener by {turned:.1} deg while elevated: facing adopted, height kept");
                    self.cam.yaw = rot;
                    self.pose_sent = Some((self.cam.x, self.cam.z, rot));
                    return;
                }
                // Moved in Studio (a Sandbox drag, an Undo) while elevated: we
                // can't tell where you'd be above it, so land (emitters go back).
                eprintln!("Studio moved the listener {moved:.2} m while elevated: back to ear height");
                self.cam.y = 0.0;
            }
        }
        self.cam.x = l.x;
        self.cam.z = l.z;
        self.cam.yaw = rot;
        self.pose_sent = Some((self.cam.x, self.cam.z, self.cam.yaw));
        self.synced = true;
    }

    /// Above or below the plane with Elevation on.
    pub fn elevated(&self) -> bool {
        self.opts.elevation && self.cam.y.abs() >= lift::MIN_HEIGHT
    }

    /// Works out where emitters should be for the current height, and marks
    /// them for sending when that changed. Also locks you to ear height while
    /// Elevation is off.
    fn track_lift(&mut self) {
        if !self.opts.elevation {
            self.cam.y = 0.0;
        }
        if !self.synced || !self.can_move() {
            return;
        }
        let target = match (&self.scene, self.opts.elevation) {
            (Some(sc), true) => lift::targets(sc, self.cam.x, self.cam.z, self.cam.y, self.cam.yaw, &mut self.lift_sides),
            _ => Vec::new(),
        };
        let same = target.len() == self.lift_sent.len()
            && target.iter().zip(&self.lift_sent).all(|(a, b)| a.0 == b.0 && (a.1 - b.1).abs() < 1e-3 && (a.2 - b.2).abs() < 1e-3);
        self.lift_target = target;
        if !same && !self.lift_dirty {
            self.lift_dirty = true;
            self.last_local_move = Some(Instant::now());
        }
    }

    /// Queues the camera's ground pose for Studio.
    fn moved(&mut self) {
        self.dirty = true;
        self.last_local_move = Some(Instant::now());
    }

    /// Sends the pose and the moved emitters when due. Returns how long until
    /// the next send is due.
    fn flush(&mut self) -> Option<Duration> {
        if !self.dirty && !self.lift_dirty {
            return None;
        }
        let since = self.last_sent.elapsed();
        if since < SEND_EVERY {
            return Some(SEND_EVERY - since);
        }
        let mut lines = Vec::new();
        if self.dirty {
            let (x, z, rot) = (self.cam.x, self.cam.z, self.cam.yaw);
            self.pose_sent = Some((x, z, rot));
            lines.push(serde_json::json!({ "cmd": "move", "x": x, "z": z, "rot": rot }).to_string());
        }
        if self.lift_dirty {
            let set: Vec<_> = self.lift_target.iter().map(|(id, x, z)| serde_json::json!({ "id": id, "x": x, "z": z })).collect();
            lines.push(serde_json::json!({ "cmd": "emitters", "set": set }).to_string());
            self.lift_sent = self.lift_target.clone();
        }
        for line in lines {
            if let Some(b) = &mut self.bridge {
                b.send(&line);
            }
            self.sent.push(line);
        }
        while self.sent.len() > 8 {
            self.sent.remove(0);
        }
        self.dirty = false;
        self.lift_dirty = false;
        self.last_sent = Instant::now();
        None
    }

    fn can_move(&self) -> bool {
        match &self.link {
            Link::Demo => true,
            Link::Linked { .. } => self.scene.as_ref().is_some_and(|s| s.listeners > 0),
            _ => false,
        }
    }

    fn input(&mut self, ctx: &egui::Context, resp: &egui::Response) -> bool {
        let mut active = false;
        if self.can_move() && resp.dragged() {
            let d = resp.drag_delta();
            if d != egui::Vec2::ZERO {
                let yaw0 = self.cam.yaw;
                self.cam.look(d.x * LOOK_DEG_PER_PX, -d.y * LOOK_DEG_PER_PX);
                if self.cam.yaw != yaw0 {
                    self.stop_auto();
                    self.moved();
                }
            }
            active = true;
        }
        if ctx.egui_wants_keyboard_input() {
            return active;
        }
        let (dt, keys, mods, scroll) = ctx.input(|i| (i.stable_dt.min(0.1), i.keys_down.clone(), i.modifiers, i.smooth_scroll_delta.y));
        if resp.hovered() && scroll != 0.0 {
            self.speed = (self.speed * (1.0 + scroll * 0.004)).clamp(SPEED_MIN, SPEED_MAX);
        }
        let down = |k: Key| keys.contains(&k);
        ctx.input(|i| {
            if i.key_pressed(Key::H) {
                self.help = !self.help;
            }
            if i.key_pressed(Key::Minus) {
                self.speed = (self.speed * 0.5).max(SPEED_MIN);
            }
            if i.key_pressed(Key::Equals) || i.key_pressed(Key::Plus) {
                self.speed = (self.speed * 2.0).min(SPEED_MAX);
            }
            if i.key_pressed(Key::OpenBracket) {
                self.opts.map_range = (self.opts.map_range / 1.5).max(0.5);
            }
            if i.key_pressed(Key::CloseBracket) {
                self.opts.map_range = (self.opts.map_range * 1.5).min(500.0);
            }
        });
        if !self.can_move() {
            return active;
        }
        if ctx.input(|i| i.key_pressed(Key::R)) {
            self.cam.y = 0.0;
            self.cam.pitch = 0.0;
        }
        let axis = |pos: bool, neg: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
        let fwd = axis(down(Key::W) || down(Key::ArrowUp), down(Key::S) || down(Key::ArrowDown));
        let right = axis(down(Key::D), down(Key::A));
        // Up/down only with Elevation on; with it off you stay at ear height.
        let up = if self.opts.elevation { axis(down(Key::E) || down(Key::Space), down(Key::Q) || down(Key::C)) } else { 0.0 };
        let turn = axis(down(Key::ArrowRight), down(Key::ArrowLeft));
        if fwd != 0.0 || right != 0.0 || up != 0.0 || turn != 0.0 {
            let mult = if mods.shift { 3.0 } else if mods.alt { 0.25 } else { 1.0 };
            let step = self.speed * mult * dt;
            let before = (self.cam.x, self.cam.z, self.cam.yaw, self.cam.y);
            if self.opts.elevation {
                self.cam.fly(fwd * step, right * step, up * step);
            } else {
                // Level flight: looking up or down doesn't leave the plane.
                let pitch = self.cam.pitch;
                self.cam.pitch = 0.0;
                self.cam.fly(fwd * step, right * step, 0.0);
                self.cam.pitch = pitch;
            }
            if turn != 0.0 {
                self.cam.look(turn * TURN_DEG_PER_S * dt, 0.0);
            }
            if (self.cam.x, self.cam.z, self.cam.yaw) != (before.0, before.1, before.2) {
                self.stop_auto();
                self.moved();
            } else if self.cam.y != before.3 {
                self.stop_auto();
            }
            active = true;
        }
        active
    }

    pub fn auto_running(&self) -> bool {
        self.spin != Spin::Off || self.circle.is_some()
    }

    pub fn stop_auto(&mut self) {
        self.spin = Spin::Off;
        self.circle = None;
    }

    /// Starts (dir = ±1) or stops (0) a circle walk.
    pub fn set_circle(&mut self, dir: i8) {
        self.auto_hint = None;
        if dir == 0 {
            self.circle = None;
            return;
        }
        let around = match self.auto.centre {
            Centre::Here => None,
            Centre::Emitter => match &self.focus_pos {
                Some((x, z, _)) => Some((*x, *z)),
                None => {
                    self.auto_hint = Some("Put an emitter under the crosshair first.".into());
                    return;
                }
            },
        };
        self.circle = Some(Circle::start(&self.cam, f32::from(dir), self.auto.radius, around));
    }

    /// Runs the automatic movements for one frame. Returns true while running.
    fn run_auto(&mut self, dt: f32) -> bool {
        if !self.auto_running() {
            return false;
        }
        if !self.can_move() {
            self.stop_auto();
            return false;
        }
        if let Some(c) = &mut self.circle {
            c.r = self.auto.radius.max(0.1);
        }
        auto::apply(&mut self.cam, self.spin, self.circle.as_mut(), &self.auto, dt);
        self.moved();
        true
    }

    /// Turns Elevation on (after the explanation) or off (back to ear height).
    pub fn set_elevation(&mut self, on: bool) {
        self.opts.elevation = on;
        if !on {
            self.cam.y = 0.0;
        }
    }

    /// Row 2: how you move. MOVE (speed, elevation), SPIN, CIRCLE.
    fn move_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            section(ui, "MOVE");
            ui.label("Speed");
            let step = (self.speed * 0.01).max(0.0005); // proportional: fine at small speeds
            ui.add(egui::DragValue::new(&mut self.speed).range(SPEED_MIN..=SPEED_MAX).speed(step).max_decimals(3).suffix("/s"))
                .on_hover_text("Flying speed (WASD, E/Q) in Sandbox units per second. Also: scroll over the view, - and = halve/double, Shift fast, Option slow.");
            let el = ui.selectable_label(self.opts.elevation, "Elevation (beta)").on_hover_text(
                "Fakes height by moving emitters away from the listener along the ground in the Sandbox while you're above or below \
                 ear height. Off: you stay at ear height. Turning it on explains how it works first.",
            );
            if el.clicked() {
                if self.opts.elevation {
                    self.set_elevation(false);
                } else {
                    self.confirm_elevation = true;
                }
            }
            ui.separator();
            section(ui, "SPIN");
            if ui.selectable_label(false, "Off").on_hover_text("Stop spinning").clicked() {
                self.spin = Spin::Off;
            }
            for (v, t) in [(Spin::CwSlow, "CW slow"), (Spin::CwFast, "CW fast"), (Spin::CcwSlow, "CCW slow"), (Spin::CcwFast, "CCW fast")] {
                if ui.selectable_label(self.spin == v, t).clicked() {
                    self.spin = if self.spin == v { Spin::Off } else { v };
                }
            }
            ui.add(egui::DragValue::new(&mut self.auto.slow_dps).range(1.0..=360.0).speed(0.5).suffix("°/s")).on_hover_text("Slow spin rate");
            ui.add(egui::DragValue::new(&mut self.auto.fast_dps).range(1.0..=720.0).speed(1.0).suffix("°/s")).on_hover_text("Fast spin rate");
            ui.separator();
            section(ui, "CIRCLE");
            let cur: i8 = self.circle.map_or(0, |c| if c.dir > 0.0 { 1 } else { -1 });
            if ui.selectable_label(false, "Off").on_hover_text("Stop walking the circle").clicked() && cur != 0 {
                self.set_circle(0);
            }
            for (v, t) in [(1i8, "CW"), (-1, "CCW")] {
                if ui.selectable_label(cur == v, t).clicked() {
                    self.set_circle(if cur == v { 0 } else { v });
                }
            }
            ui.add(egui::DragValue::new(&mut self.auto.radius).range(0.01..=500.0).speed(0.01).max_decimals(2).prefix("r ")).on_hover_text("Circle radius");
            ui.add(egui::DragValue::new(&mut self.auto.circle_speed).range(0.01..=50.0).speed(0.01).max_decimals(2).suffix("/s")).on_hover_text("Walking speed round the circle");
            egui::ComboBox::from_id_salt("centre")
                .selected_text(match self.auto.centre { Centre::Here => "beside me", Centre::Emitter => "round emitter" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.auto.centre, Centre::Here, "beside me (start where I stand)");
                    ui.selectable_value(&mut self.auto.centre, Centre::Emitter, "round the emitter under the crosshair");
                })
                .response
                .on_hover_text("Where the circle's centre is (used when a circle starts)");
            egui::ComboBox::from_id_salt("face")
                .selected_text(match self.auto.face { Face::Travel => "face travel", Face::Centre => "face centre", Face::Keep => "keep facing" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.auto.face, Face::Travel, "face where I'm walking");
                    ui.selectable_value(&mut self.auto.face, Face::Centre, "face the centre");
                    ui.selectable_value(&mut self.auto.face, Face::Keep, "keep facing");
                })
                .response
                .on_hover_text("Facing while walking the circle (a spin overrides it)");
            if let Some((_, _, name)) = self.focus_pos.as_ref().filter(|_| self.auto.centre == Centre::Emitter && self.circle.is_none()) {
                ui.label(RichText::new(format!("round {name}")).color(Color32::GRAY));
            }
            if let Some(h) = &self.auto_hint {
                ui.label(RichText::new(h).color(Color32::from_rgb(240, 210, 110)));
            }
        });
    }

    /// Row 1: the link, then VIEW and SHOW, then Controls.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let (txt, col) = match &self.link {
                Link::Demo => ("Demo scene (not linked)".to_string(), Color32::from_rgb(240, 210, 110)),
                Link::Waiting => ("Waiting for Studio…".into(), Color32::GRAY),
                Link::Linked { studio } => (
                    match &self.scene {
                        Some(s) => format!("● {}  ·  {}", s.name, short_version(studio)),
                        None => format!("● Linked  ·  {}", short_version(studio)),
                    },
                    Color32::from_rgb(120, 230, 140),
                ),
                Link::Lost => ("Link closed".into(), Color32::from_rgb(255, 120, 120)),
            };
            ui.label(RichText::new(txt).color(col));
            ui.separator();
            section(ui, "VIEW");
            ui.toggle_value(&mut self.on_top, "On top").on_hover_text("Keep this window above Studio");
            ui.toggle_value(&mut self.opts.grid, "Grid");
            ui.toggle_value(&mut self.opts.labels, "Labels");
            ui.toggle_value(&mut self.opts.minimap, "Map");
            ui.label("FOV");
            ui.add(egui::DragValue::new(&mut self.cam.fov).range(30.0..=120.0).speed(0.5).suffix("°"));
            ui.label("Scale");
            let step = (self.opts.scale * 0.01).max(0.0005);
            ui.add(egui::DragValue::new(&mut self.opts.scale).range(0.01..=100.0).speed(step).max_decimals(3).suffix("×"))
                .on_hover_text("View scale: grid spacing (1 and 5 units at 1×), marker sizes and floor depth. For projects whose units aren't metres.");
            ui.separator();
            section(ui, "SHOW");
            ui.toggle_value(&mut self.opts.max, "Max").on_hover_text("Max distance spheres");
            ui.toggle_value(&mut self.opts.min, "Min").on_hover_text("Min distance spheres");
            ui.toggle_value(&mut self.opts.size, "Size").on_hover_text("Sound size spheres (spatializer Extent Mode: User)");
            ui.separator();
            ui.toggle_value(&mut self.help, "Controls").on_hover_text("Show or hide the controls (H)");
        });
    }

    /// The explanation shown before Elevation is turned on.
    fn elevation_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_elevation {
            return;
        }
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("elevation_explainer")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.heading("Elevation (beta)");
            ui.add_space(6.0);
            ui.label(
                "The Sandbox has no height. To fake it, while you're above or below ear height this tool moves emitters away \
                 from the listener along the ground, in the Sandbox view. Each one keeps its left/right offset from you and is \
                 placed at your true 3D distance from it, so FMOD hears the right distance and attenuation for every emitter.",
            );
            ui.add_space(6.0);
            for line in ELEVATION_NOTES {
                ui.horizontal_top(|ui| {
                    ui.label("•");
                    ui.add(egui::Label::new(*line).wrap());
                });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Turn Elevation on").color(ON_ORANGE_TEXT)).fill(ORANGE)).clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(false);
                }
            });
        });
        if modal.should_close() && choice.is_none() {
            choice = Some(false);
        }
        if let Some(on) = choice {
            self.confirm_elevation = false;
            self.set_elevation(on);
        }
    }

    fn hud(&self, ui: &egui::Ui, rect: egui::Rect, focused: Option<usize>) {
        let p = ui.painter_at(rect);
        let small = FontId::proportional(12.0);
        let dim = Color32::from_rgb(150, 156, 166);
        let msg = match (&self.link, &self.scene) {
            (Link::Lost, _) => Some("Studio closed the link.\nReopen it from Scripts > DAT-audio Utilities > Open Sandbox First Person View..."),
            (Link::Waiting, _) => Some("Waiting for Studio…"),
            (Link::Linked { .. }, None) if self.scene_known => Some("Open a Sandbox scene in Studio (Window > Sandbox) and click into it."),
            (Link::Linked { .. }, Some(s)) if s.listeners == 0 => Some("This Sandbox scene has no listener. Add one in the Sandbox."),
            _ => None,
        };
        if let Some(m) = msg {
            p.text(rect.center(), Align2::CENTER_CENTER, m, FontId::proportional(16.0), Color32::from_rgb(220, 222, 226));
        }
        let height = if self.elevated() {
            let n = self.lift_target.len();
            format!("  ·  height {:+.2}  ·  Elevation: {n} emitter{} moved in the Sandbox (back in place at ear height)", self.cam.y, if n == 1 { "" } else { "s" })
        } else if self.opts.elevation {
            "  ·  Elevation on: fly up or down (E / Q)".to_string()
        } else {
            String::new()
        };
        p.text(
            rect.left_top() + vec2(10.0, 8.0),
            Align2::LEFT_TOP,
            format!("X {:.3}   Z {:.3}   facing {:.0}°{height}", self.cam.x, self.cam.z, self.cam.yaw),
            small.clone(),
            dim,
        );
        if self.help {
            controls_card(&p, rect);
        }
        // Info card for the emitter under the crosshair.
        let (Some(i), Some(scene)) = (focused, &self.scene) else { return };
        let e = &scene.emitters[i];
        let h = if self.elevated() { self.cam.y } else { 0.0 };
        let (dist, zone) = view::audible(e, self.cam.x, self.cam.z, h);
        let how_far = if h != 0.0 && lift::reacts_to_distance(e) { format!("{dist:.2} m (3D; moved in the Sandbox)") } else { format!("{dist:.2} m") };
        let mut text = format!("{}\n{}\n{how_far} · {}", e.name, if e.path.is_empty() { "" } else { &e.path }, zone.label());
        if let Some(s) = &e.spatial {
            for d in &s.distance_driven {
                let range = if d.normalized { "Distance (Normalized)".to_string() } else { format!("Distance 0-{:.1} m", d.max) };
                text += &format!("\nfollows {range}: {}", d.what);
            }
        }
        if e.is_snapshot() {
            text += "\nsnapshot: not positional, never moved by Elevation";
        }
        if let Some(s) = e.spatial.as_ref().filter(|s| s.is_3d) {
            for (k, sp) in std::iter::once(s).chain(s.others.iter()).enumerate() {
                let kind = if sp.spatializer == "object" { "object spatializer" } else { "spatializer" };
                let preset = if sp.preset.is_empty() { String::new() } else { format!(" (preset {})", sp.preset) };
                let range = if sp.override_range { "own range" } else { "event range" };
                text += &format!("\n{}{}: {kind}{preset}", if k == 0 { "" } else { "+ " }, if sp.track.is_empty() { "?" } else { &sp.track });
                text += &format!("\n   min {:.2} m · max {:.2} m ({range}) · {}", sp.min, sp.max, sp.rolloff_name());
                if !sp.has_curve() && self.opts.elevation {
                    text += "\n   no distance curve: not moved by Elevation";
                }
                text += &match sp.extent_mode {
                    1 => format!("\n   sound size {:.2} m · min extent {:.0}°", sp.size, sp.min_extent),
                    m => format!("\n   sound size: {}", if m == 0 { "Auto" } else { "Off" }),
                };
            }
        }
        let font = FontId::proportional(13.0);
        let galley = p.layout(text, font, Color32::from_rgb(230, 232, 236), 360.0);
        let at = rect.right_bottom() - vec2(galley.size().x + 22.0, galley.size().y + 22.0);
        let card = egui::Rect::from_min_size(at, galley.size() + vec2(16.0, 16.0));
        p.rect_filled(card, 6.0, Color32::from_rgba_unmultiplied(10, 12, 16, 225));
        p.rect_stroke(card, 6.0, egui::Stroke::new(1.5, view::colour(&e.id)), egui::StrokeKind::Inside);
        p.galley(card.min + vec2(8.0, 8.0), galley, Color32::WHITE);
    }

    fn dev_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        // In bridge mode, wait for Studio's listener (and the test move) so they're in the picture.
        if self.bridge.is_some() && self.link != Link::Lost && (!self.synced || self.test_move.is_some() || self.dirty) {
            ctx.request_repaint();
            return;
        }
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

/// The controls, readable, bottom-left. Plain words: egui's default font has
/// no arrow glyphs.
pub const CONTROLS: &[(&str, &str)] = &[
    ("Drag (any mouse button)", "look around"),
    ("W / S  or  Up / Down", "fly forward / back (where you look)"),
    ("A / D", "strafe left / right"),
    ("Left / Right", "turn"),
    ("E or Space  /  Q or C", "up / down (Elevation on)"),
    ("Shift  /  Option", "fast / slow"),
    ("Scroll  /  -  /  =", "speed: fine / halve / double"),
    ("R", "back to ear height, level"),
    ("[  /  ]", "zoom the map in / out"),
    ("H", "show / hide these controls"),
    ("SPIN / CIRCLE (top)", "auto moves; any manual move stops them"),
];

fn controls_card(p: &egui::Painter, rect: egui::Rect) {
    let key_font = FontId::proportional(13.0);
    let row_h = 18.0;
    let key_w = 190.0;
    let h = row_h * (CONTROLS.len() as f32 + 2.4) + 12.0;
    let card = egui::Rect::from_min_size(rect.left_bottom() + vec2(10.0, -h - 10.0), vec2(470.0, h));
    p.rect_filled(card, 6.0, Color32::from_rgba_unmultiplied(10, 12, 16, 225));
    p.rect_stroke(card, 6.0, egui::Stroke::new(1.0, Color32::from_rgb(60, 66, 76)), egui::StrokeKind::Inside);
    let mut y = card.top() + 10.0;
    p.text(pos2(card.left() + 10.0, y), Align2::LEFT_TOP, concat!("Sandbox First Person v", env!("CARGO_PKG_VERSION"), "  ·  Controls (click the view first)"), FontId::proportional(14.0), Color32::WHITE);
    y += row_h + 4.0;
    for (k, v) in CONTROLS {
        p.text(pos2(card.left() + 10.0, y), Align2::LEFT_TOP, *k, key_font.clone(), Color32::from_rgb(255, 210, 90));
        p.text(pos2(card.left() + 10.0 + key_w, y), Align2::LEFT_TOP, *v, key_font.clone(), Color32::from_rgb(225, 228, 232));
        y += row_h;
    }
    p.text(
        pos2(card.left() + 10.0, y + 2.0),
        Align2::LEFT_TOP,
        "No height in the Sandbox: Elevation (beta) moves emitters along the ground to fake it.",
        FontId::proportional(12.0),
        Color32::from_rgb(150, 156, 166),
    );
}

/// The smaller angle between two headings, degrees.
fn angle_diff(a: f32, b: f32) -> f32 {
    ((a - b + 540.0).rem_euclid(360.0) - 180.0).abs()
}

/// A small dim label that starts a toolbar section.
fn section(ui: &mut egui::Ui, name: &str) {
    ui.label(RichText::new(name).size(11.0).strong().color(SECTION));
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
        if self.synced && self.can_move() {
            if let Some((x, z, rot)) = self.test_move.take() {
                (self.cam.x, self.cam.z, self.cam.yaw) = (x, z, rot);
                self.moved();
            }
        }
        if self.applied_on_top != Some(self.on_top) {
            // Only on change: a viewport command every frame keeps waking the app.
            let level = if self.on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.applied_on_top = Some(self.on_top);
        }

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::top("move").show(ui, |ui| self.move_bar(ui));
        self.elevation_dialog(&ctx);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(view::BG)).show(ui, |ui| {
            let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = resp.rect;
            let mut active = self.input(&ctx, &resp);
            let dt = ctx.input(|i| i.stable_dt.min(0.1));
            active |= self.run_auto(dt);
            self.track_lift();
            let h = if self.elevated() { self.cam.y } else { 0.0 };
            let drawn = view::draw(&painter, rect, &self.cam, self.scene.as_ref(), &self.opts, h);
            self.focus_pos = drawn.focused.and_then(|i| self.scene.as_ref().map(|s| (s.emitters[i].x, s.emitters[i].z, s.emitters[i].name.clone())));
            if let Some(c) = &self.circle {
                view::circle_path(&painter, rect, &self.cam, c.cx, c.cz, c.r);
            }
            if self.opts.minimap {
                let side = (rect.width().min(rect.height()) * 0.3).clamp(120.0, 220.0);
                let map = egui::Rect::from_min_size(pos2(rect.right() - side - 10.0, rect.top() + 10.0), vec2(side, side));
                view::minimap(&painter, map, &self.cam, self.scene.as_ref(), self.opts.map_range);
            }
            self.hud(ui, rect, drawn.focused);
            if active {
                ctx.request_repaint();
            }
        });
        if let Some(wait) = self.flush() {
            ctx.request_repaint_after(wait);
        }

        let title = match &self.scene {
            Some(s) => format!("First Person — {}", s.name),
            None => "First Person".to_string(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        self.dev_screenshot(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(STORAGE_OPTS, serde_json::to_string(&self.opts).unwrap_or_default());
        storage.set_string(STORAGE_SPEED, self.speed.to_string());
        storage.set_string(STORAGE_ON_TOP, if self.on_top { "1" } else { "0" }.into());
        storage.set_string(STORAGE_FOV, self.cam.fov.to_string());
        storage.set_string(STORAGE_AUTO, serde_json::to_string(&self.auto).unwrap_or_default());
        storage.set_string(STORAGE_HELP, if self.help { "1" } else { "0" }.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linked() -> App {
        let mut a = App::bare();
        a.receive(r#"{"type":"hello","studio":"Version 2.03.14, 64-bit","script":"0.1.0","project":""}"#);
        a.receive(r#"{"type":"scene","id":"s1","name":"S","listeners":1,"emitters":[]}"#);
        a
    }

    #[test]
    fn follows_studio_until_you_move_then_ignores_echoes() {
        let mut a = linked();
        assert_eq!(a.link, Link::Linked { studio: "Version 2.03.14, 64-bit".into() });
        a.receive(r#"{"type":"listener","x":3,"z":4,"rot":-90}"#);
        assert_eq!((a.cam.x, a.cam.z, a.cam.yaw), (3.0, 4.0, 270.0), "adopts Studio's pose, yaw wrapped");
        // You move: the next Studio update (an echo or a stale pose) is ignored.
        a.cam.x = 10.0;
        a.moved();
        a.receive(r#"{"type":"listener","x":3,"z":4,"rot":0}"#);
        assert_eq!(a.cam.x, 10.0);
        // Once sent and idle for ADOPT_AFTER, Studio wins again (e.g. a Sandbox drag or Undo).
        a.last_sent = Instant::now() - SEND_EVERY;
        assert!(a.flush().is_none());
        assert!(a.sent.last().unwrap().contains("\"cmd\":\"move\""));
        a.last_local_move = Some(Instant::now() - ADOPT_AFTER - Duration::from_millis(1));
        a.receive(r#"{"type":"listener","x":-1,"z":2,"rot":45}"#);
        assert_eq!((a.cam.x, a.cam.z, a.cam.yaw), (-1.0, 2.0, 45.0));
    }

    #[test]
    fn sends_are_rate_limited() {
        let mut a = linked();
        a.last_sent = Instant::now();
        a.moved();
        assert!(a.flush().is_some(), "too soon: waits");
        assert!(a.sent.is_empty());
        a.last_sent = Instant::now() - SEND_EVERY;
        assert!(a.flush().is_none());
        assert_eq!(a.sent.len(), 1);
        assert!(a.flush().is_none(), "nothing new to send");
        assert_eq!(a.sent.len(), 1);
    }

    #[test]
    fn switching_scene_jumps_to_its_listener() {
        let mut a = linked();
        a.receive(r#"{"type":"listener","x":1,"z":1,"rot":0}"#);
        a.moved();
        a.receive(r#"{"type":"scene","id":"s2","name":"Other","listeners":1,"emitters":[]}"#);
        a.receive(r#"{"type":"listener","x":7,"z":8,"rot":0}"#);
        assert_eq!((a.cam.x, a.cam.z), (7.0, 8.0));
    }

    #[test]
    fn no_moves_without_a_listener() {
        let mut a = linked();
        assert!(a.can_move());
        a.receive(r#"{"type":"scene","id":"s1","name":"S","listeners":0,"emitters":[]}"#);
        assert!(!a.can_move());
        a.receive(r#"{"type":"scene","none":true}"#);
        assert!(!a.can_move());
    }

    #[test]
    fn auto_moves_send_and_manual_moves_stop_them() {
        let mut a = linked();
        a.receive(r#"{"type":"listener","x":0,"z":0,"rot":0}"#);
        a.spin = Spin::CwSlow;
        a.set_circle(1);
        assert!(a.auto_running());
        assert!(a.run_auto(0.5));
        assert!(a.dirty, "auto movement is sent to Studio");
        assert!(a.cam.yaw > 0.0 && a.cam.x > 0.0);
        a.stop_auto();
        assert!(!a.auto_running());
        // No listener: autos stop by themselves.
        a.spin = Spin::CcwFast;
        a.receive(r#"{"type":"scene","id":"s1","name":"S","listeners":0,"emitters":[]}"#);
        assert!(!a.run_auto(0.1));
        assert_eq!(a.spin, Spin::Off);
    }

    #[test]
    fn orbiting_needs_an_emitter_under_the_crosshair() {
        let mut a = linked();
        a.auto.centre = Centre::Emitter;
        a.set_circle(1);
        assert!(a.circle.is_none() && a.auto_hint.is_some());
        a.focus_pos = Some((0.0, 10.0, "Waterfall".into()));
        a.set_circle(-1);
        let c = a.circle.unwrap();
        assert_eq!((c.cx, c.cz, c.dir), (0.0, 10.0, -1.0));
    }

    fn sent_emitters(a: &App) -> Vec<(String, f64, f64)> {
        let line = a.sent.iter().rev().find(|l| l.contains("\"emitters\"")).expect("an emitters line was sent");
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        v["set"].as_array().unwrap().iter().map(|e| (e["id"].as_str().unwrap().to_string(), e["x"].as_f64().unwrap(), e["z"].as_f64().unwrap())).collect()
    }

    fn send_now(a: &mut App) {
        a.last_sent = Instant::now() - SEND_EVERY;
        a.flush();
    }

    #[test]
    fn elevation_moves_emitters_and_landing_puts_them_back() {
        let mut a = linked();
        a.receive(r#"{"type":"scene","id":"s1","name":"S","listeners":1,"emitters":[
            {"id":"e","name":"E","x":0,"z":10,"spatial":{"min":1,"max":30,"is3D":true}},
            {"id":"off","name":"Off","x":3,"z":10,"spatial":{"min":1,"max":30,"is3D":true,"rolloff":4}}]}"#);
        a.receive(r#"{"type":"listener","x":0,"z":0,"rot":0}"#);
        a.set_elevation(true);
        a.cam.y = 10.0;
        a.track_lift();
        send_now(&mut a);
        let set = sent_emitters(&a);
        assert_eq!(set.len(), 1, "only the emitter with a distance curve moves");
        assert_eq!(set[0].0, "e");
        assert!((set[0].2 - 200f64.sqrt()).abs() < 1e-3 && set[0].1.abs() < 1e-4, "moved to the 3D distance, straight ahead: {set:?}");
        assert!(!a.sent.iter().any(|l| l.contains("\"move\"")), "the listener didn't move");
        // Turning Elevation off lands you and puts them back (empty set).
        a.set_elevation(false);
        a.track_lift();
        send_now(&mut a);
        assert_eq!(a.cam.y, 0.0);
        assert!(sent_emitters(&a).is_empty());
    }

    #[test]
    fn a_sandbox_drag_while_elevated_lands_you() {
        let mut a = linked();
        a.receive(r#"{"type":"listener","x":0,"z":0,"rot":0}"#);
        a.set_elevation(true);
        a.cam.y = 5.0;
        a.last_local_move = Some(Instant::now() - ADOPT_AFTER - Duration::from_millis(1));
        // A rounding echo: ignored, height kept.
        a.receive(r#"{"type":"listener","x":0.003,"z":-0.002,"rot":0.2}"#);
        assert_eq!(a.cam.y, 5.0);
        // Turned in Studio: facing adopted, height kept.
        a.receive(r#"{"type":"listener","x":0,"z":0,"rot":90}"#);
        assert_eq!((a.cam.y, a.cam.yaw), (5.0, 90.0));
        // Moved 5 cm in Studio: a real move, back to ear height.
        a.receive(r#"{"type":"listener","x":0.05,"z":0,"rot":90}"#);
        assert_eq!(a.cam.y, 0.0);
    }

    #[test]
    fn elevation_is_off_by_default_even_for_old_saved_options() {
        assert!(!Opts::default().elevation);
        let o: Opts = serde_json::from_str(r#"{"height_fit":true,"grid":false,"labels":true,"min":true,"max":true,"size":true,"minimap":true,"map_range":30.0}"#).unwrap();
        assert!(!o.elevation && !o.grid);
    }

    #[test]
    fn with_elevation_off_you_stay_at_ear_height() {
        let mut a = linked();
        a.receive(r#"{"type":"listener","x":0,"z":0,"rot":0}"#);
        a.cam.y = 3.0; // e.g. left over
        a.track_lift();
        assert_eq!(a.cam.y, 0.0);
        a.cam.pitch = 45.0;
        // Level flight: the same step as the input code takes.
        let pitch = a.cam.pitch;
        a.cam.pitch = 0.0;
        a.cam.fly(1.0, 0.0, 0.0);
        a.cam.pitch = pitch;
        assert_eq!(a.cam.y, 0.0);
        assert!((a.cam.z - 1.0).abs() < 1e-5);
    }

    #[test]
    fn angle_diff_wraps() {
        assert_eq!(angle_diff(359.0, 1.0), 2.0);
        assert_eq!(angle_diff(90.0, 270.0), 180.0);
    }

    #[test]
    fn theme_is_always_dark() {
        let ctx = egui::Context::default();
        apply_theme(&ctx);
        assert_eq!(ctx.options(|o| o.theme_preference), egui::ThemePreference::Dark);
        assert_eq!(ctx.style_of(egui::Theme::Dark).visuals.selection.bg_fill, ORANGE, "active buttons are orange");
    }
}
