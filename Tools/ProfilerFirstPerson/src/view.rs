//! Draws the view with egui's painter: a software-projected wireframe (floor
//! grid, event instances with their distance spheres, the listeners and their
//! probes), labels and a minimap.
//!
//! Unlike the Sandbox, the game's positions are full 3D, so everything sits at
//! its real height. The floor grid goes at the session's "ground" (a low
//! percentile of instance heights) as a reference only. Distances and zones
//! are measured from the **hearing point**: the listener's attenuation
//! position (the probe) when the game sets one, else the listener.
//!
//! With DETACHED CAMERA on, the view is drawn from the free camera, and the
//! listener camera's view (what the attached view would show) is drawn as a
//! frustum, here and on the minimap.

use crate::camera::{Camera, Frustum, V3, add, cross, len, normalize, right_of, scale, sub};
use crate::capture::{Activity, ListenerKey};
use crate::link::{EventInfo, Spatial};
use crate::session::{LiveInstance, Moment};
use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Opts {
    /// View scale: grid spacing and marker sizes (1 = metres).
    pub scale: f32,
    /// Pick the scale from the scene (powers of ten).
    pub auto_scale: bool,
    pub grid: bool,
    pub labels: bool,
    pub min: bool,
    pub max: bool,
    pub size: bool,
    /// Also show instances that exist but aren't playing.
    pub idle: bool,
    /// Also draw the spheres of the events an instance's instruments play
    /// (child events), at the instance.
    pub children: bool,
    pub minimap: bool,
    /// The info card for the instance under the crosshair.
    pub card: bool,
    /// Minimap radius, in grid steps (`scale`).
    pub map_range: f32,
    /// How spheres are drawn: wireframe lines, a shell (the sphere's surface), or both.
    pub style: SphereStyle,
    /// Opacity of every sphere (lines and clouds), 0-1.
    pub opacity: f32,
    /// Seconds a released one-shot is shown after it starts (its real end
    /// isn't in the capture).
    pub oneshot_hold: f32,
}

impl Default for Opts {
    fn default() -> Self {
        // Default view settings: Grid, Labels, Map, Auto scale, Max,
        // Min, Idle on; Size and Children off; Lines; 90% opacity.
        Opts {
            scale: 1.0,
            auto_scale: true,
            grid: true,
            labels: true,
            min: true,
            max: true,
            size: false,
            idle: true,
            children: false,
            minimap: true,
            card: true,
            map_range: 30.0,
            style: SphereStyle::Lines,
            opacity: 0.9,
            oneshot_hold: 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SphereStyle {
    Lines,
    /// Was "Cloud" in 0.3.0 (a filled volume). Drawn as the sphere's outline.
    #[serde(alias = "Cloud")]
    Shell,
    Both,
}

impl SphereStyle {
    pub fn lines(self) -> bool {
        self != SphereStyle::Shell
    }
    pub fn shell(self) -> bool {
        self != SphereStyle::Lines
    }
}

/// Events without a Studio colour (or not in the project) are drawn white.
pub const NO_COLOUR: Color32 = Color32::from_rgb(235, 236, 240);

pub const BG: Color32 = Color32::from_rgb(16, 18, 22);
const GRID: Color32 = Color32::from_rgb(70, 80, 95);
const TEXT: Color32 = Color32::from_rgb(225, 228, 232);
const DIM: Color32 = Color32::from_rgb(140, 146, 156);
pub const LISTENER: Color32 = Color32::from_rgb(255, 210, 90);
pub const PROBE: Color32 = Color32::from_rgb(90, 230, 255);
/// The listener camera's view while detached.
pub const VIEW_CAST: Color32 = Color32::from_rgb(240, 140, 30);

/// Studio's Event Browser colours ("Red", "Cyan Light 2", ...). The RGB values
/// are approximations of Studio's swatches; "Light 1/2" mix toward white.
/// None for "Default" (no colour set) or a name not known.
pub fn studio_colour(name: &str) -> Option<Color32> {
    let mut it = name.split(" Light ");
    let base = match it.next()?.trim() {
        "Red" => Color32::from_rgb(230, 70, 75),
        "Orange" => Color32::from_rgb(240, 145, 50),
        "OrangeRed" => Color32::from_rgb(240, 95, 50),
        "Yellow" => Color32::from_rgb(235, 215, 70),
        "YellowGreen" => Color32::from_rgb(170, 215, 70),
        "Green" => Color32::from_rgb(90, 200, 90),
        "Cyan" => Color32::from_rgb(70, 205, 220),
        "Blue" => Color32::from_rgb(75, 135, 235),
        "Purple" => Color32::from_rgb(150, 100, 230),
        "Magenta" => Color32::from_rgb(225, 85, 200),
        "Gray" | "Grey" => Color32::from_rgb(160, 160, 165),
        _ => return None,
    };
    let light: f32 = it.next().and_then(|n| n.trim().parse().ok()).unwrap_or(0.0);
    let k = (light * 0.28).min(0.8);
    let mix = |c: u8| (c as f32 + (255.0 - c as f32) * k).round() as u8;
    Some(Color32::from_rgb(mix(base.r()), mix(base.g()), mix(base.b())))
}

/// An event's colour: its Studio colour, else white.
pub fn event_colour(info: Option<&EventInfo>, _id: &str) -> Color32 {
    info.and_then(|i| studio_colour(&i.color)).unwrap_or(NO_COLOUR)
}

pub fn fade(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// The scale (a power of ten) that suits a typical distance in the scene:
/// about a 25th of it, so markers (0.3 scale) stay small at that distance.
pub fn pick_scale(typical: f32) -> f32 {
    if !(typical.is_finite() && typical > 0.0) {
        return 1.0;
    }
    10f32.powf((typical / 25.0).log10().round()).clamp(0.001, 10000.0)
}

/// The spatial settings in force for an instance: the event's, with the
/// instance's own min/max overrides (setProperty) where a spatializer uses
/// the event's range.
pub fn effective(info: Option<&EventInfo>, li: &LiveInstance) -> Option<Spatial> {
    let mut s = info?.spatial.clone()?;
    let apply = |sp: &mut Spatial| {
        if !sp.override_range {
            if let Some(m) = li.min_override {
                sp.min = m;
            }
            if let Some(m) = li.max_override {
                sp.max = m;
            }
        }
    };
    apply(&mut s);
    s.others.iter_mut().for_each(apply);
    Some(s)
}

/// Where the hearing point is relative to an instance's range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    Inside,
    Attenuating,
    OutOfRange,
    NotSpatial,
    Snapshot,
    /// Not looked up (yet, or no Studio link).
    Unknown,
    /// Looked up: the project has no such event.
    Missing,
}

impl Zone {
    pub fn label(self) -> &'static str {
        match self {
            Zone::Inside => "inside min",
            Zone::Attenuating => "in range",
            Zone::OutOfRange => "out of range",
            Zone::NotSpatial => "2D (no spatializer)",
            Zone::Snapshot => "snapshot",
            Zone::Unknown => "range unknown",
            Zone::Missing => "not in this project",
        }
    }
    fn colour(self) -> Color32 {
        match self {
            Zone::Inside => Color32::from_rgb(120, 230, 140),
            Zone::Attenuating => Color32::from_rgb(240, 210, 110),
            _ => DIM,
        }
    }
}

pub fn zone(info: Option<&EventInfo>, sp: Option<&Spatial>, d: f32) -> Zone {
    match (info, sp) {
        (None, _) => Zone::Unknown,
        (Some(i), _) if i.missing => Zone::Missing,
        (Some(i), _) if i.is_snapshot() => Zone::Snapshot,
        (_, Some(s)) if s.is_3d => {
            if d <= s.min {
                Zone::Inside
            } else if d <= s.max {
                Zone::Attenuating
            } else {
                Zone::OutOfRange
            }
        }
        (_, Some(s)) if !s.distance_driven.is_empty() => match s.distance_range() {
            Some((_, m)) if d <= m => Zone::Attenuating,
            Some(_) => Zone::OutOfRange,
            None => Zone::NotSpatial,
        },
        _ => Zone::NotSpatial,
    }
}

fn polyline_dashed(p: &Painter, cam: &Camera, rect: Rect, pts: &[V3], stroke: Stroke, dashed: bool) {
    for (i, w) in pts.windows(2).enumerate() {
        if dashed && i % 2 == 1 {
            continue;
        }
        if let Some((a, b)) = cam.project_segment(w[0], w[1], rect) {
            p.line_segment([a, b], stroke);
        }
    }
}

fn polyline(p: &Painter, cam: &Camera, rect: Rect, pts: &[V3], stroke: Stroke) {
    polyline_dashed(p, cam, rect, pts, stroke, false);
}

fn dashed_line(p: &Painter, cam: &Camera, rect: Rect, a: V3, b: V3, stroke: Stroke) {
    let n = 16;
    let pts: Vec<V3> = (0..=n).map(|i| add(a, scale(sub(b, a), i as f32 / n as f32))).collect();
    polyline_dashed(p, cam, rect, &pts, stroke, true);
}

fn circle_pts(c: V3, a: V3, b: V3, r: f32, n: usize) -> Vec<V3> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            add(c, add(scale(a, r * t.cos()), scale(b, r * t.sin())))
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum SphereKind {
    Max,
    Min,
    Size,
    /// A further spatializer in the same event: fainter and dashed.
    Other,
    /// A child event's range: dashed, in the child's colour.
    Child,
}

/// How spheres look: lines and/or cloud, and the overall opacity.
#[derive(Clone, Copy)]
struct Look {
    lines: bool,
    shell: bool,
    /// Shell styles: an inactive sphere (not playing, or heard from outside
    /// its max distance) is a light wireframe mesh instead of a shell.
    mesh: bool,
    opacity: f32,
}

/// The outline (silhouette) of a sphere seen from the camera, as world
/// points, or None when the camera is inside it.
fn silhouette(cam: &Camera, c: V3, r: f32, n: usize) -> Option<Vec<V3>> {
    let to_c = sub(c, cam.pos);
    let d = len(to_c);
    if d <= r * 1.001 {
        return None;
    }
    let nrm = scale(to_c, 1.0 / d);
    let pc = add(cam.pos, scale(nrm, d - r * r / d));
    let rs = r * (1.0 - (r * r) / (d * d)).max(0.0).sqrt();
    let up = if nrm[1].abs() > 0.95 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let ax = normalize(cross(nrm, up));
    let bx = cross(nrm, ax);
    Some(circle_pts(pc, ax, bx, rs, n))
}

/// The sphere's surface: where its radius is. Seen from outside, a faint
/// disc with a strong rim (denser toward the edge, like a bubble). From
/// inside, the surface is all around: a light tint and a faint mesh of it.
fn shell(p: &Painter, cam: &Camera, rect: Rect, c: V3, r: f32, col: Color32, a: f32) {
    match silhouette(cam, c, r, 64) {
        None => {
            p.rect_filled(rect, 0.0, fade(col, 0.035 * a));
            mesh(p, cam, rect, c, r, col, SphereKind::Max, a * 0.6);
        }
        Some(pts) => {
            let proj: Option<Vec<Pos2>> = pts[..64].iter().map(|&q| cam.project(q, rect)).collect();
            if let Some(poly) = proj {
                p.add(Shape::convex_polygon(poly, fade(col, 0.06 * a), Stroke::NONE));
            }
            // The rim: the outline plus two softer ones just inside it.
            for (k, w, al) in [(0.955, 4.0, 0.10), (0.982, 3.0, 0.25), (1.0, 2.0, 0.8)] {
                if let Some(q) = silhouette(cam, c, r * k, 64) {
                    polyline(p, cam, rect, &q, Stroke::new(w, fade(col, al * a)));
                }
            }
        }
    }
}

/// A light latitude/longitude wireframe: an inactive sphere in the cloud styles.
fn mesh(p: &Painter, cam: &Camera, rect: Rect, c: V3, r: f32, col: Color32, kind: SphereKind, a: f32) {
    let base = match kind {
        SphereKind::Max => 0.35,
        SphereKind::Min => 0.45,
        SphereKind::Size => 0.35,
        SphereKind::Other | SphereKind::Child => 0.25,
    };
    let st = Stroke::new(1.0, fade(col, base * a));
    let n = 48;
    // Parallels.
    for k in 1..6 {
        let lat = (k as f32 / 6.0 - 0.5) * std::f32::consts::PI;
        let (y, rr) = (r * lat.sin(), r * lat.cos());
        polyline(p, cam, rect, &circle_pts(add(c, [0.0, y, 0.0]), [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], rr, n), st);
    }
    // Meridians.
    for k in 0..6 {
        let t = k as f32 / 6.0 * std::f32::consts::PI;
        polyline(p, cam, rect, &circle_pts(c, [t.cos(), 0.0, t.sin()], [0.0, 1.0, 0.0], r, n), st);
    }
}

/// A sphere: as lines (its horizontal equator, two faint meridians and its
/// outline as seen from the camera) and/or as a cloud. `a` scales it (idle
/// instances are faint), on top of the overall opacity.
fn sphere(p: &Painter, cam: &Camera, rect: Rect, c: V3, r: f32, col: Color32, kind: SphereKind, a: f32, look: Look) {
    if r <= 0.0 {
        return;
    }
    let a = a * look.opacity;
    if look.mesh {
        mesh(p, cam, rect, c, r, col, kind, a);
        return;
    }
    // Shells show the outer boundaries; min spheres stay as lines.
    let as_shell = look.shell && kind != SphereKind::Min;
    if as_shell {
        let k = if matches!(kind, SphereKind::Other | SphereKind::Child) { 0.6 } else { 1.0 };
        shell(p, cam, rect, c, r, col, a * k);
    }
    if !look.lines && as_shell {
        return;
    }
    let (ring_w, ring_a, line_a) = match kind {
        SphereKind::Max => (1.6, 0.8, 0.35),
        SphereKind::Min => (1.4, 0.9, 0.5),
        SphereKind::Size => (1.0, 0.6, 0.4),
        SphereKind::Other => (1.2, 0.55, 0.25),
        SphereKind::Child => (1.3, 0.7, 0.3),
    };
    let dashed = matches!(kind, SphereKind::Other | SphereKind::Child);
    let n = 64;
    polyline_dashed(p, cam, rect, &circle_pts(c, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], r, n), Stroke::new(ring_w, fade(col, ring_a * a)), dashed);
    if kind == SphereKind::Max || kind == SphereKind::Min {
        let faint = Stroke::new(1.0, fade(col, line_a * 0.45 * a));
        polyline(p, cam, rect, &circle_pts(c, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], r, n), faint);
        polyline(p, cam, rect, &circle_pts(c, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], r, n), faint);
    }
    if let Some(pts) = silhouette(cam, c, r, n) {
        if kind == SphereKind::Size && !look.shell {
            let proj: Option<Vec<Pos2>> = pts[..n].iter().map(|&q| cam.project(q, rect)).collect();
            if let Some(poly) = proj {
                p.add(Shape::convex_polygon(poly, fade(col, 0.10 * a), Stroke::NONE));
            }
        }
        polyline_dashed(p, cam, rect, &pts, Stroke::new(1.0, fade(col, line_a * a)), dashed);
    }
}

/// The floor grid at height `y`, around the point below the camera's view.
fn grid(p: &Painter, cam: &Camera, rect: Rect, sc: f32, y: f32) {
    let major = 5.0 * sc;
    // Centre on where the view meets the floor (or below the camera).
    let hit = if cam.f[1] < -0.05 { (cam.pos[1] - y) / -cam.f[1] } else { 0.0 };
    let hit = hit.min(60.0 * sc);
    let (fx, fz) = (cam.pos[0] + cam.f[0] * hit, cam.pos[2] + cam.f[2] * hit);
    let (cx, cz) = ((fx / major).round() * major, (fz / major).round() * major);
    let focus = [fx, y, fz];
    let lines = |step: f32, extent: f32, alpha: f32| {
        let n = (extent / step) as i32;
        for i in -n..=n {
            let k = i as f32 * step;
            if step < major && (k / major - (k / major).round()).abs() < 1e-3 {
                continue; // drawn as a major line
            }
            for axis in 0..2 {
                let mut t = -extent;
                while t < extent {
                    let (a, b) = if axis == 0 { ([cx + k, y, cz + t], [cx + k, y, cz + t + major]) } else { ([cx + t, y, cz + k], [cx + t + major, y, cz + k]) };
                    let mid = scale(add(a, b), 0.5);
                    let f = 1.0 - len(sub(mid, focus)) / extent;
                    if f > 0.0 {
                        if let Some((pa, pb)) = cam.project_segment(a, b, rect) {
                            p.line_segment([pa, pb], Stroke::new(1.0, fade(GRID, alpha * f)));
                        }
                    }
                    t += major;
                }
            }
        }
    };
    lines(sc, 20.0 * sc, 0.35);
    lines(major, 100.0 * sc, 0.8);
}

/// A listener: a ring around its position across its forward/right plane,
/// a forward arrow and an up tick. `main`: the one the view follows.
fn listener_marker(p: &Painter, cam: &Camera, rect: Rect, l: &ListenerKey, sc: f32, right_handed: bool, main: bool, label: &str) {
    let pos = l.pose.pos;
    let f = normalize(l.pose.fwd);
    let r = right_of(f, l.pose.up, right_handed);
    let u = normalize(l.pose.up);
    let col = if main { LISTENER } else { fade(LISTENER, 0.55) };
    let rad = 0.6 * sc;
    polyline(p, cam, rect, &circle_pts(pos, f, r, rad, 32), Stroke::new(2.0, col));
    if let Some((a, b)) = cam.project_segment(pos, add(pos, scale(f, 2.0 * sc)), rect) {
        p.line_segment([a, b], Stroke::new(2.0, col));
    }
    if let Some((a, b)) = cam.project_segment(pos, add(pos, scale(u, 0.8 * sc)), rect) {
        p.line_segment([a, b], Stroke::new(1.0, col));
    }
    if let Some(sp) = cam.project(pos, rect) {
        p.text(sp + vec2(0.0, 10.0), Align2::CENTER_TOP, label, FontId::proportional(12.0), col);
    }
}

/// The probe (attenuation position): a diamond, joined to its listener.
fn probe_marker(p: &Painter, cam: &Camera, rect: Rect, l: &ListenerKey, at: V3, sc: f32, ground: Option<f32>) {
    dashed_line(p, cam, rect, l.pose.pos, at, Stroke::new(1.5, fade(PROBE, 0.7)));
    if let Some(g) = ground {
        if let Some((a, b)) = cam.project_segment([at[0], g, at[2]], at, rect) {
            p.line_segment([a, b], Stroke::new(1.0, fade(PROBE, 0.4)));
        }
    }
    let v = cam.to_view(at);
    if v[2] <= crate::camera::NEAR {
        return;
    }
    let c = cam.project_view(v, rect);
    let s = (0.5 * sc * cam.focal(rect) / v[2]).clamp(5.0, 30.0);
    let pts = vec![c + vec2(0.0, -s), c + vec2(s, 0.0), c + vec2(0.0, s), c + vec2(-s, 0.0)];
    p.add(Shape::convex_polygon(pts, fade(PROBE, 0.35), Stroke::new(2.0, PROBE)));
    p.text(c + vec2(0.0, s + 3.0), Align2::CENTER_TOP, "probe", FontId::proportional(12.0), PROBE);
}

pub struct Drawn {
    /// Index (in the moment's instances) of the one nearest the crosshair.
    pub focused: Option<usize>,
}

pub struct Scene<'a> {
    pub moment: &'a Moment,
    pub events: &'a HashMap<String, EventInfo>,
    /// The listener index the camera follows.
    pub follow: usize,
    /// The camera is moved off the listener (draw the listener itself).
    pub offset: bool,
    /// DETACHED CAMERA: the listener camera's view, drawn as a frustum.
    pub cast: Option<Frustum>,
}

pub fn label_for(info: Option<&EventInfo>, li: &LiveInstance) -> String {
    match info {
        Some(i) if !i.name.is_empty() => i.name.clone(),
        _ if li.event.is_empty() => format!("instance {}", li.handle),
        _ => li.event.clone(),
    }
}

pub fn draw(p: &Painter, rect: Rect, cam: &Camera, scene: Option<&Scene>, opts: &Opts) -> Drawn {
    p.rect_filled(rect, 0.0, BG);
    let sc = opts.scale;
    let base_look = Look { lines: opts.style.lines(), shell: opts.style.shell(), mesh: false, opacity: opts.opacity.clamp(0.0, 1.0) };
    let mut focused = None;
    let Some(sc_) = scene else { return Drawn { focused } };
    let m = sc_.moment;
    let floor = m.ground.unwrap_or(cam.pos[1] - 1.7 * sc);
    if opts.grid {
        grid(p, cam, rect, sc, floor);
    }
    let hearing = m.listener(sc_.follow).map(Moment::hearing_point).unwrap_or(cam.pos);

    // Listeners and probes.
    for (i, l) in m.listeners.iter().enumerate() {
        let Some(l) = l else { continue };
        let main = i == sc_.follow;
        if !main || sc_.offset {
            let label = if m.listeners.len() > 1 { format!("listener {i}") } else { "listener".into() };
            listener_marker(p, cam, rect, l, sc, m.right_handed, main, &label);
        }
        if let Some(at) = l.atten {
            if len(sub(at, l.pose.pos)) > 0.01 * sc {
                probe_marker(p, cam, rect, l, at, sc, m.ground);
            }
        }
    }

    let shown: Vec<usize> = (0..m.instances.len()).filter(|&i| opts.idle || m.instances[i].activity != Activity::Idle).collect();
    // Far to near, so near things draw on top.
    let mut order: Vec<(usize, f32)> = shown.iter().map(|&i| (i, cam.to_view(m.instances[i].pose.pos)[2])).collect();
    order.sort_by(|a, b| b.1.total_cmp(&a.1));

    let centre = rect.center();
    let mut best = 60.0_f32;
    for &(i, _) in &order {
        if let Some(sp) = cam.project(m.instances[i].pose.pos, rect) {
            let d = sp.distance(centre);
            if d < best && rect.contains(sp) {
                best = d;
                focused = Some(i);
            }
        }
    }

    for &(i, _) in &order {
        let li = &m.instances[i];
        let info = sc_.events.get(&li.event);
        let col = event_colour(info, &li.event);
        let c = li.pose.pos;
        let sp = effective(info, li);
        let dist = len(sub(c, hearing));
        let zone = zone(info, sp.as_ref(), dist);
        let a = if li.activity == Activity::Idle { 0.35 } else { 1.0 };
        // Shell styles: a shell only while playing and heard within range, else a mesh.
        let playing = li.activity != Activity::Idle;
        let look = Look { mesh: base_look.shell && !(playing && matches!(zone, Zone::Inside | Zone::Attenuating)), ..base_look };
        if let Some(s) = sp.as_ref().filter(|s| s.is_3d) {
            if opts.max {
                sphere(p, cam, rect, c, s.max, col, SphereKind::Max, a, look);
            }
            if opts.min {
                sphere(p, cam, rect, c, s.min, col, SphereKind::Min, a, look);
            }
            if opts.size {
                if let Some(r) = s.size_radius() {
                    sphere(p, cam, rect, c, r, col, SphereKind::Size, a, look);
                }
            }
            for o in &s.others {
                if opts.max {
                    sphere(p, cam, rect, c, o.max, col, SphereKind::Other, a, look);
                }
                if opts.min {
                    sphere(p, cam, rect, c, o.min, col, SphereKind::Other, a, look);
                }
            }
        }
        // Child events play at the instance: their own ranges, dashed.
        let kids = if opts.children { info.map_or(&[][..], |i| &i.children[..]) } else { &[] };
        for k in kids {
            if let Some(ks) = k.spatial.as_ref().filter(|s| s.is_3d) {
                let kc = studio_colour(&k.color).unwrap_or(col);
                let klook = Look { mesh: base_look.shell && !(playing && dist <= ks.max), ..base_look };
                if opts.max {
                    sphere(p, cam, rect, c, ks.max, kc, SphereKind::Child, a, klook);
                }
                if opts.min {
                    sphere(p, cam, rect, c, ks.min, kc, SphereKind::Child, a, klook);
                }
            }
        }
        // Stalk down to the floor.
        if let Some((pa, pb)) = cam.project_segment([c[0], floor, c[2]], c, rect) {
            p.line_segment([pa, pb], Stroke::new(1.0, fade(col, 0.5 * a)));
        }
        let v = cam.to_view(c);
        if v[2] <= crate::camera::NEAR {
            continue;
        }
        let spt = cam.project_view(v, rect);
        let rad = (0.3 * sc * cam.focal(rect) / v[2]).clamp(3.0, 40.0);
        let hot = focused == Some(i);
        match li.activity {
            Activity::Idle => {
                p.circle_stroke(spt, rad, Stroke::new(1.5, fade(col, 0.6)));
            }
            _ if matches!(zone, Zone::NotSpatial | Zone::Snapshot | Zone::Unknown | Zone::Missing) => {
                p.circle_stroke(spt, rad, Stroke::new(2.0, col));
            }
            Activity::OneShot => {
                p.circle_filled(spt, rad, fade(col, 0.7));
            }
            Activity::Playing => {
                p.circle_filled(spt, rad, col);
            }
        }
        if hot {
            p.circle_stroke(spt, rad + 4.0, Stroke::new(2.0, Color32::WHITE));
        }
        if opts.labels && rect.expand(40.0).contains(spt) {
            let size = (16.0 - v[2] / sc * 0.08).clamp(11.0, 16.0);
            let top = pos2(spt.x, spt.y - rad - 4.0);
            let name = label_for(info, li);
            p.text(pos2(top.x, top.y - size - 2.0), Align2::CENTER_BOTTOM, name, FontId::proportional(size), if hot { Color32::WHITE } else { fade(TEXT, a.max(0.6)) });
            p.text(top, Align2::CENTER_BOTTOM, format!("{} · {}", fmt_dist(dist), zone.label()), FontId::proportional(size - 2.0), fade(zone.colour(), a.max(0.6)));
            if !kids.is_empty() {
                let n = kids.len();
                p.text(pos2(spt.x, spt.y + rad + 3.0), Align2::CENTER_TOP, format!("+ {n} child event{}", if n == 1 { "" } else { "s" }), FontId::proportional(size - 3.0), fade(TEXT, 0.6 * a.max(0.6)));
            }
        }
    }

    if let Some(f) = &sc_.cast {
        cast(p, cam, rect, f);
    }

    // Crosshair.
    let s = Stroke::new(1.0, fade(Color32::WHITE, 0.5));
    p.line_segment([centre - vec2(7.0, 0.0), centre + vec2(7.0, 0.0)], s);
    p.line_segment([centre - vec2(0.0, 7.0), centre + vec2(0.0, 7.0)], s);
    Drawn { focused }
}

/// The listener camera's view: edges from its position to the corners of its
/// picture, that picture's outline (filled faintly), and one at half way.
fn cast(p: &Painter, cam: &Camera, rect: Rect, f: &Frustum) {
    let edge = Stroke::new(1.5, fade(VIEW_CAST, 0.85));
    for c in f.corners {
        if let Some((a, b)) = cam.project_segment(f.apex, c, rect) {
            p.line_segment([a, b], edge);
        }
    }
    let mid: Vec<V3> = f.corners.iter().map(|&c| add(f.apex, scale(sub(c, f.apex), 0.5))).collect();
    for (ring, stroke) in [(f.corners.to_vec(), Stroke::new(2.0, VIEW_CAST)), (mid, Stroke::new(1.0, fade(VIEW_CAST, 0.45)))] {
        for k in 0..4 {
            if let Some((a, b)) = cam.project_segment(ring[k], ring[(k + 1) % 4], rect) {
                p.line_segment([a, b], stroke);
            }
        }
    }
    let pts: Option<Vec<Pos2>> = f.corners.iter().map(|&c| cam.project(c, rect)).collect();
    if let Some(pts) = pts {
        p.add(Shape::convex_polygon(pts, fade(VIEW_CAST, 0.08), Stroke::NONE));
    }
    if let Some(sp) = cam.project(f.apex, rect) {
        p.circle_filled(sp, 4.0, VIEW_CAST);
    }
    // Labelled at the far edge, clear of the listener's own label.
    if let Some(sp) = cam.project(f.corners[0], rect) {
        p.text(sp + vec2(4.0, 4.0), Align2::LEFT_TOP, format!("listener view · FOV {:.0}°", f.fov), FontId::proportional(12.0), VIEW_CAST);
    }
}

/// A distance with sensible decimals for any unit size.
pub fn fmt_dist(d: f32) -> String {
    if d >= 100.0 {
        format!("{d:.0}")
    } else if d >= 10.0 {
        format!("{d:.1}")
    } else {
        format!("{d:.2}")
    }
}

/// A heading-up minimap around the camera's ground position.
pub fn minimap(p: &Painter, rect: Rect, cam: &Camera, scene: Option<&Scene>, opts: &Opts) {
    p.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(10, 12, 16, 220));
    p.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::from_rgb(60, 66, 76)), egui::StrokeKind::Inside);
    let clip = p.with_clip_rect(rect.shrink(1.0));
    let c = rect.center();
    let range = opts.map_range * opts.scale;
    let px = rect.width() * 0.5 / range;
    let (hx, hz) = cam.ground_heading();
    // Screen right on the ground: for left-handed (up × fwd) = (hz, -hx); mirrored when right-handed.
    let (rx, rz) = if cam.right_handed { (-hz, hx) } else { (hz, -hx) };
    let to_map = |q: V3| {
        let (dx, dz) = (q[0] - cam.pos[0], q[2] - cam.pos[2]);
        pos2(c.x + (dx * rx + dz * rz) * px, c.y - (dx * hx + dz * hz) * px)
    };
    if let Some(s) = scene {
        let m = s.moment;
        for li in m.instances.iter().filter(|li| opts.idle || li.activity != Activity::Idle) {
            let info = s.events.get(&li.event);
            let col = event_colour(info, &li.event);
            let q = to_map(li.pose.pos);
            let a = if li.activity == Activity::Idle { 0.4 } else { 1.0 };
            match effective(info, li).filter(|s| s.is_3d) {
                Some(sp) => {
                    clip.circle_stroke(q, sp.max * px, Stroke::new(1.0, fade(col, 0.5 * a)));
                    clip.circle_stroke(q, sp.min * px, Stroke::new(1.0, fade(col, 0.85 * a)));
                    clip.circle_filled(q, 3.5, fade(col, a));
                }
                None => {
                    clip.circle_stroke(q, 3.5, Stroke::new(1.5, fade(DIM, a)));
                }
            }
        }
        for (i, l) in m.listeners.iter().enumerate() {
            let Some(l) = l else { continue };
            let q = to_map(l.pose.pos);
            let col = if i == s.follow { LISTENER } else { fade(LISTENER, 0.5) };
            clip.circle_stroke(q, 5.0, Stroke::new(2.0, col));
            if let Some(at) = l.atten {
                let pq = to_map(at);
                if pq.distance(q) > 1.0 {
                    clip.line_segment([q, pq], Stroke::new(1.0, fade(PROBE, 0.6)));
                }
                clip.add(Shape::convex_polygon(vec![pq + vec2(0.0, -5.0), pq + vec2(5.0, 0.0), pq + vec2(0.0, 5.0), pq + vec2(-5.0, 0.0)], PROBE, Stroke::NONE));
            }
        }
    }
    if let Some(f) = scene.and_then(|s| s.cast.as_ref()) {
        let a = to_map(f.apex);
        for c in f.corners {
            clip.line_segment([a, to_map(c)], Stroke::new(1.0, fade(VIEW_CAST, 0.8)));
        }
        let ring: Vec<Pos2> = f.corners.iter().map(|&c| to_map(c)).collect();
        for k in 0..4 {
            clip.line_segment([ring[k], ring[(k + 1) % 4]], Stroke::new(1.0, fade(VIEW_CAST, 0.5)));
        }
        clip.circle_filled(a, 3.0, VIEW_CAST);
    }
    // The camera: a triangle pointing up (its heading).
    clip.add(Shape::convex_polygon(vec![c + vec2(0.0, -8.0), c + vec2(5.5, 6.0), c + vec2(-5.5, 6.0)], Color32::WHITE, Stroke::NONE));
    p.text(rect.left_bottom() + vec2(6.0, -4.0), Align2::LEFT_BOTTOM, format!("radius {}", fmt_dist(range)), FontId::proportional(11.0), DIM);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::Pose;

    fn li(min: Option<f32>, max: Option<f32>) -> LiveInstance {
        LiveInstance {
            index: 0,
            handle: 1,
            event: "{e}".into(),
            pose: Pose { pos: [0.0; 3], fwd: [0.0, 0.0, 1.0], up: [0.0, 1.0, 0.0] },
            activity: Activity::Playing,
            min_override: min,
            max_override: max,
        }
    }

    #[test]
    fn instance_overrides_replace_the_event_range_only() {
        let info = EventInfo {
            id: "{e}".into(),
            spatial: Some(Spatial {
                min: 1.0,
                max: 20.0,
                is_3d: true,
                others: vec![Spatial { min: 0.5, max: 4.0, is_3d: true, override_range: true, ..Default::default() }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let s = effective(Some(&info), &li(None, Some(250.0))).unwrap();
        assert_eq!((s.min, s.max), (1.0, 250.0));
        assert_eq!(s.others[0].max, 4.0, "a spatializer with its own range keeps it");
    }

    #[test]
    fn studio_colours_and_light_variants() {
        let red = studio_colour("Red").unwrap();
        let light = studio_colour("Red Light 2").unwrap();
        assert!(light.g() > red.g() && light.r() >= red.r(), "Light mixes toward white");
        assert_eq!(studio_colour("Default"), None);
        let info = EventInfo { color: "Default".into(), ..Default::default() };
        assert_eq!(event_colour(Some(&info), "{x}"), NO_COLOUR, "no colour set: white");
        assert_eq!(event_colour(None, "{x}"), NO_COLOUR);
    }

    #[test]
    fn old_saved_cloud_style_reads_as_shell() {
        let o: Opts = serde_json::from_str(r#"{"style":"Cloud"}"#).unwrap();
        assert_eq!(o.style, SphereStyle::Shell);
    }

    #[test]
    fn zones() {
        let info = EventInfo { id: "e".into(), spatial: Some(Spatial { min: 2.0, max: 18.0, is_3d: true, ..Default::default() }), ..Default::default() };
        let s = info.spatial.as_ref();
        assert_eq!(zone(Some(&info), s, 1.0), Zone::Inside);
        assert_eq!(zone(Some(&info), s, 10.0), Zone::Attenuating);
        assert_eq!(zone(Some(&info), s, 30.0), Zone::OutOfRange);
        assert_eq!(zone(None, None, 1.0), Zone::Unknown);
        let snap = EventInfo { kind: "Snapshot".into(), ..info.clone() };
        assert_eq!(zone(Some(&snap), s, 1.0), Zone::Snapshot);
    }

    #[test]
    fn scale_is_a_power_of_ten_near_a_25th_of_typical() {
        assert_eq!(pick_scale(25.0), 1.0);
        assert_eq!(pick_scale(675.0), 10.0, "top-down game: sounds ~675 away");
        assert_eq!(pick_scale(11.0), 1.0, "a metre-scale game");
        assert_eq!(pick_scale(0.0), 1.0);
    }
}
