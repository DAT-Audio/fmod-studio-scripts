//! Draws the first-person view with egui's painter: a software-projected
//! wireframe (floor grid, emitters, distance spheres), labels and a minimap.
//!
//! Emitters and the listener sit on the Sandbox plane (y = 0, "ear height").
//! The floor grid is drawn below it, at -FLOOR, as a visual reference only.
//! Everything drawn in metres (grid, floor, markers) is multiplied by
//! `Opts::scale`, for projects whose units are much smaller or larger than
//! a metre.

use crate::camera::{Camera, V3, add, cross, len, normalize, scale, sub};
use crate::scene::{Emitter, Scene};
use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

pub const FLOOR: f32 = 1.7;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Opts {
    /// View scale: grid spacing, marker sizes and floor depth (1 = metres).
    pub scale: f32,
    /// Elevation (beta): fake height by moving emitters (lift.rs). Off by
    /// default; saved options from the old "height_fit" are ignored.
    pub elevation: bool,
    pub grid: bool,
    pub labels: bool,
    pub min: bool,
    pub max: bool,
    pub size: bool,
    pub minimap: bool,
    pub map_range: f32,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { scale: 1.0, elevation: false, grid: true, labels: true, min: true, max: true, size: true, minimap: true, map_range: 30.0 }
    }
}

pub const BG: Color32 = Color32::from_rgb(16, 18, 22);
const GRID: Color32 = Color32::from_rgb(70, 80, 95);
const TEXT: Color32 = Color32::from_rgb(225, 228, 232);
const DIM: Color32 = Color32::from_rgb(140, 146, 156);
const LISTENER: Color32 = Color32::from_rgb(255, 210, 90);

const PALETTE: [Color32; 8] = [
    Color32::from_rgb(90, 180, 255),
    Color32::from_rgb(255, 140, 90),
    Color32::from_rgb(120, 220, 140),
    Color32::from_rgb(230, 120, 220),
    Color32::from_rgb(250, 220, 100),
    Color32::from_rgb(110, 220, 220),
    Color32::from_rgb(255, 120, 140),
    Color32::from_rgb(170, 150, 255),
];

/// A stable colour per emitter id.
pub fn colour(id: &str) -> Color32 {
    let h = id.bytes().fold(2166136261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
    PALETTE[(h % PALETTE.len() as u32) as usize]
}

fn fade(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Where the listener is relative to an emitter's range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    Inside,
    Attenuating,
    OutOfRange,
    NotSpatial,
    Snapshot,
}

impl Zone {
    pub fn label(self) -> &'static str {
        match self {
            Zone::Inside => "inside min",
            Zone::Attenuating => "in range",
            Zone::OutOfRange => "out of range",
            Zone::NotSpatial => "2D (no spatializer)",
            Zone::Snapshot => "snapshot (not positional)",
        }
    }
    fn colour(self) -> Color32 {
        match self {
            Zone::Inside => Color32::from_rgb(120, 230, 140),
            Zone::Attenuating => Color32::from_rgb(240, 210, 110),
            Zone::OutOfRange => DIM,
            Zone::NotSpatial | Zone::Snapshot => DIM,
        }
    }
}

/// The distance FMOD hears and the zone it falls in. With Elevation (`h` not
/// 0), distance-reactive emitters are moved to your true 3D distance;
/// everything else is heard on the plane.
pub fn audible(e: &Emitter, lx: f32, lz: f32, h: f32) -> (f32, Zone) {
    let mut d = ((e.x - lx).powi(2) + (e.z - lz).powi(2)).sqrt();
    if h != 0.0 && crate::lift::reacts_to_distance(e) {
        d = (d * d + h * h).sqrt();
    }
    if e.is_snapshot() {
        return (d, Zone::Snapshot);
    }
    let zone = match &e.spatial {
        // No spatializer, but something follows built-in Distance.
        Some(s) if !s.is_3d && !s.distance_driven.is_empty() => match s.distance_range() {
            Some((_, m)) if d <= m => Zone::Attenuating,
            Some(_) => Zone::OutOfRange,
            None => Zone::NotSpatial,
        },
        Some(s) if s.is_3d => {
            if d <= s.min {
                Zone::Inside
            } else if d <= s.max {
                Zone::Attenuating
            } else {
                Zone::OutOfRange
            }
        }
        _ => Zone::NotSpatial,
    };
    (d, zone)
}

fn polyline(p: &Painter, cam: &Camera, rect: Rect, pts: &[V3], stroke: Stroke) {
    polyline_dashed(p, cam, rect, pts, stroke, false);
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
}

/// A wireframe sphere: the ring on the Sandbox plane (where the listener can
/// actually be), its outline as seen from the camera, and two faint meridians.
fn sphere(p: &Painter, cam: &Camera, rect: Rect, c: V3, r: f32, col: Color32, kind: SphereKind) {
    if r <= 0.0 {
        return;
    }
    let (ring_w, ring_a, line_a) = match kind {
        SphereKind::Max => (1.6, 0.85, 0.35),
        SphereKind::Min => (1.4, 0.9, 0.5),
        SphereKind::Size => (1.0, 0.6, 0.4),
        SphereKind::Other => (1.2, 0.55, 0.25),
    };
    let dashed = kind == SphereKind::Other;
    let n = 64;
    // The ring on the plane. Seen edge-on from ear height it's just a line
    // across the horizon, so it fades in as the view rises above the plane.
    let edge_on = ((cam.y - c[1]).abs() / 1.5).clamp(0.12, 1.0);
    polyline_dashed(p, cam, rect, &circle_pts(c, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], r, n), Stroke::new(ring_w, fade(col, ring_a * edge_on)), dashed);
    // Meridians (vertical great circles).
    if kind == SphereKind::Max || kind == SphereKind::Min {
        let faint = Stroke::new(1.0, fade(col, line_a * 0.45));
        polyline(p, cam, rect, &circle_pts(c, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], r, n), faint);
        polyline(p, cam, rect, &circle_pts(c, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], r, n), faint);
    }
    // Outline (silhouette) when the camera is outside the sphere.
    let to_c = sub(c, cam.pos());
    let d = len(to_c);
    if d > r * 1.001 {
        let nrm = scale(to_c, 1.0 / d);
        let pc = add(cam.pos(), scale(nrm, d - r * r / d));
        let rs = r * (1.0 - (r * r) / (d * d)).max(0.0).sqrt();
        let up = if nrm[1].abs() > 0.95 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let a = normalize(cross(nrm, up));
        let b = cross(nrm, a);
        let pts = circle_pts(pc, a, b, rs, n);
        if kind == SphereKind::Size {
            let proj: Option<Vec<Pos2>> = pts[..n].iter().map(|&q| cam.project(q, rect)).collect();
            if let Some(poly) = proj {
                p.add(Shape::convex_polygon(poly, fade(col, 0.10), Stroke::NONE));
            }
        }
        polyline_dashed(p, cam, rect, &pts, Stroke::new(1.0, fade(col, line_a)), dashed);
    }
}

fn grid(p: &Painter, cam: &Camera, rect: Rect, sc: f32) {
    let y = -FLOOR * sc;
    let major = 5.0 * sc;
    let (cx, cz) = ((cam.x / major).round() * major, (cam.z / major).round() * major);
    let lines = |step: f32, extent: f32, alpha: f32, width: f32| {
        let n = (extent / step) as i32;
        for i in -n..=n {
            let k = i as f32 * step;
            if step < major && (k / major - (k / major).round()).abs() < 1e-3 {
                continue; // drawn as a major line
            }
            for axis in 0..2 {
                // Chunks of one major step so far parts can fade.
                let mut t = -extent;
                while t < extent {
                    let (a, b) = if axis == 0 {
                        ([cx + k, y, cz + t], [cx + k, y, cz + t + major])
                    } else {
                        ([cx + t, y, cz + k], [cx + t + major, y, cz + k])
                    };
                    let mid = scale(add(a, b), 0.5);
                    let f = 1.0 - len(sub(mid, cam.pos())) / extent;
                    if f > 0.0 {
                        if let Some((pa, pb)) = cam.project_segment(a, b, rect) {
                            p.line_segment([pa, pb], Stroke::new(width, fade(GRID, alpha * f)));
                        }
                    }
                    t += major;
                }
            }
        }
    };
    lines(sc, 20.0 * sc, 0.35, 1.0);
    lines(major, 100.0 * sc, 0.8, 1.0);
}

pub struct Drawn {
    /// Index of the emitter nearest the crosshair, if any is close to it.
    pub focused: Option<usize>,
}

/// `h`: the height Elevation is faking (0 when it's off or you're at ear height).
pub fn draw(p: &Painter, rect: Rect, cam: &Camera, scene: Option<&Scene>, opts: &Opts, h: f32) -> Drawn {
    p.rect_filled(rect, 0.0, BG);
    if opts.grid {
        grid(p, cam, rect, opts.scale);
    }
    let (lx, lz) = (cam.x, cam.z);
    let sc = opts.scale;
    let floor = -FLOOR * sc;

    // Where the listener really is, when the view has left the plane.
    if cam.y.abs() > 0.3 * sc {
        let at = [lx, 0.0, lz];
        polyline(p, cam, rect, &circle_pts(at, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5 * sc, 32), Stroke::new(2.0, LISTENER));
        let (s, c) = cam.yaw.to_radians().sin_cos();
        if let Some((a, b)) = cam.project_segment(at, [lx + s * 1.2 * sc, 0.0, lz + c * 1.2 * sc], rect) {
            p.line_segment([a, b], Stroke::new(2.0, LISTENER));
        }
        if let Some((a, b)) = cam.project_segment([lx, floor, lz], at, rect) {
            p.line_segment([a, b], Stroke::new(1.0, fade(LISTENER, 0.5)));
        }
    }

    let mut focused = None;
    let Some(scene) = scene else { return Drawn { focused } };

    // Far to near, so near things draw on top.
    let mut order: Vec<(usize, f32)> = scene.emitters.iter().enumerate().map(|(i, e)| (i, cam.to_view([e.x, 0.0, e.z])[2])).collect();
    order.sort_by(|a, b| b.1.total_cmp(&a.1));

    let centre = rect.center();
    let mut best = 60.0_f32;
    for &(i, _) in &order {
        let e = &scene.emitters[i];
        if let Some(sp) = cam.project([e.x, 0.0, e.z], rect) {
            let d = sp.distance(centre);
            if d < best && rect.contains(sp) {
                best = d;
                focused = Some(i);
            }
        }
    }

    for &(i, _) in &order {
        let e = &scene.emitters[i];
        let col = colour(&e.id);
        let c = [e.x, 0.0, e.z];
        let (dist, zone) = audible(e, lx, lz, h);
        if let Some(s) = e.spatial.as_ref().filter(|s| s.is_3d) {
            if opts.max {
                sphere(p, cam, rect, c, s.max, col, SphereKind::Max);
            }
            if opts.min {
                sphere(p, cam, rect, c, s.min, col, SphereKind::Min);
            }
            if opts.size {
                if let Some(r) = s.size_radius() {
                    sphere(p, cam, rect, c, r, col, SphereKind::Size);
                }
            }
            for o in &s.others {
                if opts.max {
                    sphere(p, cam, rect, c, o.max, col, SphereKind::Other);
                }
                if opts.min {
                    sphere(p, cam, rect, c, o.min, col, SphereKind::Other);
                }
            }
        }
        // Stalk to the floor, then the marker on the plane.
        if let Some((a, b)) = cam.project_segment([e.x, floor, e.z], c, rect) {
            p.line_segment([a, b], Stroke::new(1.0, fade(col, 0.6)));
        }
        let v = cam.to_view(c);
        if v[2] <= crate::camera::NEAR {
            continue;
        }
        let sp = cam.project_view(v, rect);
        let rad = (0.3 * sc * cam.focal(rect) / v[2]).clamp(3.0, 40.0);
        let hot = focused == Some(i);
        if matches!(zone, Zone::NotSpatial | Zone::Snapshot) {
            p.circle_stroke(sp, rad, Stroke::new(2.0, DIM));
        } else {
            p.circle_filled(sp, rad, col);
        }
        if hot {
            p.circle_stroke(sp, rad + 4.0, Stroke::new(2.0, Color32::WHITE));
        }
        if opts.labels && rect.expand(40.0).contains(sp) {
            let size = (16.0 - v[2] * 0.08).clamp(11.0, 16.0);
            let top = pos2(sp.x, sp.y - rad - 4.0);
            p.text(pos2(top.x, top.y - size - 2.0), Align2::CENTER_BOTTOM, &e.name, FontId::proportional(size), if hot { Color32::WHITE } else { TEXT });
            let label = if h != 0.0 && crate::lift::reacts_to_distance(e) {
                format!("{dist:.2} m (3D) · {}", zone.label())
            } else {
                format!("{dist:.2} m · {}", zone.label())
            };
            p.text(top, Align2::CENTER_BOTTOM, label, FontId::proportional(size - 2.0), zone.colour());
        }
    }

    // Crosshair.
    let s = Stroke::new(1.0, fade(Color32::WHITE, 0.5));
    p.line_segment([centre - vec2(7.0, 0.0), centre + vec2(7.0, 0.0)], s);
    p.line_segment([centre - vec2(0.0, 7.0), centre + vec2(0.0, 7.0)], s);

    Drawn { focused }
}

/// The path of a running circle walk, on the plane: faint, dashed.
pub fn circle_path(p: &Painter, rect: Rect, cam: &Camera, cx: f32, cz: f32, r: f32) {
    let pts = circle_pts([cx, 0.0, cz], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], r, 96);
    polyline_dashed(p, cam, rect, &pts, Stroke::new(1.5, fade(LISTENER, 0.45)), true);
}

/// A heading-up minimap: forward is always up, like the view.
pub fn minimap(p: &Painter, rect: Rect, cam: &Camera, scene: Option<&Scene>, range: f32) {
    p.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(10, 12, 16, 220));
    p.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::from_rgb(60, 66, 76)), egui::StrokeKind::Inside);
    let clip = p.with_clip_rect(rect.shrink(1.0));
    let c = rect.center();
    let px = rect.width() * 0.5 / range;
    let (s, co) = cam.yaw.to_radians().sin_cos();
    let to_map = |x: f32, z: f32| {
        let (dx, dz) = (x - cam.x, z - cam.z);
        let right = dx * co - dz * s;
        let fwd = dx * s + dz * co;
        pos2(c.x + right * px, c.y - fwd * px)
    };
    // Field of view wedge.
    let half = (cam.fov * 0.5).to_radians();
    let aspect = 1.6_f32;
    let hhalf = (half.tan() * aspect).atan();
    for sgn in [-1.0_f32, 1.0] {
        let end = c + vec2((hhalf * sgn).sin(), -(hhalf * sgn).cos()) * rect.width();
        clip.line_segment([c, end], Stroke::new(1.0, fade(LISTENER, 0.25)));
    }
    if let Some(scene) = scene {
        for e in &scene.emitters {
            let col = colour(&e.id);
            let m = to_map(e.x, e.z);
            if let Some(sp) = e.spatial.as_ref().filter(|s| s.is_3d) {
                clip.circle_stroke(m, sp.max * px, Stroke::new(1.0, fade(col, 0.55)));
                clip.circle_stroke(m, sp.min * px, Stroke::new(1.0, fade(col, 0.9)));
                if let Some(r) = sp.size_radius() {
                    clip.circle_filled(m, r * px, fade(col, 0.15));
                }
                for o in &sp.others {
                    clip.circle_stroke(m, o.max * px, Stroke::new(1.0, fade(col, 0.3)));
                }
                clip.circle_filled(m, 3.5, col);
            } else {
                clip.circle_stroke(m, 3.5, Stroke::new(1.5, DIM));
            }
        }
    }
    // The listener: a triangle pointing up (forward).
    clip.add(Shape::convex_polygon(vec![c + vec2(0.0, -8.0), c + vec2(5.5, 6.0), c + vec2(-5.5, 6.0)], LISTENER, Stroke::NONE));
    p.text(rect.left_bottom() + vec2(6.0, -4.0), Align2::LEFT_BOTTOM, format!("{range:.2} m"), FontId::proportional(11.0), DIM);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Spatial, demo};

    #[test]
    fn zones_follow_the_plane_distance() {
        let mut e = demo().emitters[0].clone();
        e.x = 0.0;
        e.z = 10.0;
        e.spatial = Some(Spatial { min: 2.0, max: 18.0, is_3d: true, ..Default::default() });
        assert_eq!(audible(&e, 0.0, 9.0, 0.0).1, Zone::Inside);
        assert_eq!(audible(&e, 0.0, 0.0, 0.0).1, Zone::Attenuating);
        assert_eq!(audible(&e, 0.0, -20.0, 0.0).1, Zone::OutOfRange);
        e.spatial = None;
        assert_eq!(audible(&e, 0.0, 0.0, 0.0).1, Zone::NotSpatial);
        e.kind = "Snapshot".into();
        assert_eq!(audible(&e, 0.0, 0.0, 0.0).1, Zone::Snapshot);
    }

    #[test]
    fn colours_are_stable_per_id() {
        assert_eq!(colour("{abc}"), colour("{abc}"));
    }
}
