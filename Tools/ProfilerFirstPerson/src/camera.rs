//! The camera: the game's listener pose plus an offset, and perspective
//! projection.
//!
//! World axes are the game's FMOD coordinates: Y up. FMOD's default is
//! left-handed (+X right, +Z forward); a game that initialises FMOD with
//! FMOD_INIT_3D_RIGHTHANDED has +Z towards the viewer, so "right" is
//! forward × up instead of up × forward. The capture header says which.
//!
//! The game's camera isn't in the capture, only its listener. So the view's
//! camera is the listener pose moved by an offset in the listener's own frame
//! (right, up, back) and turned by an extra yaw/pitch: zero for "the listener
//! is the camera", or e.g. back and up for a third-person game.
//!
//! DETACHED CAMERA swaps the view for a `Free` camera you fly around, and
//! draws the listener camera's view (its `Frustum`) so you can see what it
//! covers from outside.

use crate::capture::Pose;
use eframe::egui::{Pos2, Rect, pos2};

pub type V3 = [f32; 3];

pub const NEAR: f32 = 0.05;

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn normalize(a: V3) -> V3 {
    let l = len(a);
    if l > 0.0 { scale(a, 1.0 / l) } else { a }
}
/// Rotates `v` about the unit axis `k` by `deg` (Rodrigues).
pub fn rotate(v: V3, k: V3, deg: f32) -> V3 {
    let (s, c) = deg.to_radians().sin_cos();
    add(add(scale(v, c), scale(cross(k, v), s)), scale(k, dot(k, v) * (1.0 - c)))
}

/// Where the view's camera sits relative to the listener, in its frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Offset {
    pub right: f32,
    pub up: f32,
    pub back: f32,
    /// Extra turn, degrees: positive yaw turns right, positive pitch looks up.
    pub yaw: f32,
    pub pitch: f32,
}

impl Offset {
    pub fn is_zero(&self) -> bool {
        *self == Offset::default()
    }

    /// The camera is moved off the listener (not just turned).
    pub fn moves(&self) -> bool {
        self.right != 0.0 || self.up != 0.0 || self.back != 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pos: V3,
    /// Forward, right and up unit vectors (orthonormal).
    pub f: V3,
    pub r: V3,
    pub u: V3,
    /// Vertical field of view in degrees.
    pub fov: f32,
    pub right_handed: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { pos: [0.0; 3], f: [0.0, 0.0, 1.0], r: [1.0, 0.0, 0.0], u: [0.0, 1.0, 0.0], fov: 70.0, right_handed: false }
    }
}

/// The listener's right vector, for either handedness.
pub fn right_of(fwd: V3, up: V3, right_handed: bool) -> V3 {
    normalize(if right_handed { cross(fwd, up) } else { cross(up, fwd) })
}

impl Camera {
    /// The camera for a listener pose: orthonormalised, then offset.
    pub fn from_pose(p: &Pose, off: &Offset, fov: f32, right_handed: bool) -> Camera {
        let mut f = normalize(p.fwd);
        if len(f) < 0.5 {
            f = [0.0, 0.0, 1.0];
        }
        let mut r = right_of(f, p.up, right_handed);
        if len(r) < 0.5 {
            // Up parallel to forward (or missing): pick any level right.
            r = right_of(f, if f[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] }, right_handed);
        }
        let u = if right_handed { cross(r, f) } else { cross(f, r) };
        // Offset in the listener's frame, before the extra turn.
        let pos = add(add(add(p.pos, scale(r, off.right)), scale(u, off.up)), scale(f, -off.back));
        // Turn about the world's vertical (Y), so turning a tilted
        // (e.g. top-down) listener round keeps looking at the ground; then
        // tilt about the new right.
        let sign = if right_handed { -1.0 } else { 1.0 };
        let y = [0.0, 1.0, 0.0];
        let (f, r, u) = (rotate(f, y, off.yaw * sign), rotate(r, y, off.yaw * sign), rotate(u, y, off.yaw * sign));
        let (f, u) = (rotate(f, r, -off.pitch * sign), rotate(u, r, -off.pitch * sign));
        Camera { pos, f, r, u, fov, right_handed }
    }

    /// World point to view space: x right, y up, z forward (depth).
    pub fn to_view(&self, p: V3) -> V3 {
        let d = sub(p, self.pos);
        [dot(d, self.r), dot(d, self.u), dot(d, self.f)]
    }

    /// Pixels per unit of x/z at depth 1, for a viewport of this height.
    pub fn focal(&self, rect: Rect) -> f32 {
        (rect.height() * 0.5) / (self.fov.to_radians() * 0.5).tan()
    }

    /// A view-space point (in front of the near plane) to the screen.
    pub fn project_view(&self, v: V3, rect: Rect) -> Pos2 {
        let f = self.focal(rect);
        let c = rect.center();
        pos2(c.x + v[0] / v[2] * f, c.y - v[1] / v[2] * f)
    }

    /// A world point to the screen, or None behind the near plane.
    pub fn project(&self, p: V3, rect: Rect) -> Option<Pos2> {
        let v = self.to_view(p);
        (v[2] > NEAR).then(|| self.project_view(v, rect))
    }

    /// A world segment clipped to the near plane, on screen.
    pub fn project_segment(&self, a: V3, b: V3, rect: Rect) -> Option<(Pos2, Pos2)> {
        let (va, vb) = clip_near(self.to_view(a), self.to_view(b))?;
        Some((self.project_view(va, rect), self.project_view(vb, rect)))
    }

    /// Heading on the ground plane (X, Z) of the view direction, for the map.
    pub fn ground_heading(&self) -> (f32, f32) {
        let (x, z) = (self.f[0], self.f[2]);
        let l = (x * x + z * z).sqrt();
        if l > 1e-3 { (x / l, z / l) } else {
            // Looking straight down: the top of the screen is "ahead".
            let l = (self.u[0] * self.u[0] + self.u[2] * self.u[2]).sqrt().max(1e-6);
            (self.u[0] / l, self.u[2] / l)
        }
    }
}

/// A camera you fly around (DETACHED CAMERA): a position, a heading about
/// the world's vertical and a tilt. It stays level (no roll).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Free {
    pub pos: V3,
    /// Degrees: positive turns right, positive pitch looks up.
    pub yaw: f32,
    pub pitch: f32,
}

impl Free {
    /// Behind and above `c`, looking at it, so its view is in sight.
    pub fn behind(c: &Camera, dist: f32) -> Free {
        let pos = add(add(c.pos, scale(c.f, -dist)), [0.0, dist * 0.5, 0.0]);
        Free::looking_at(pos, c.pos, c.right_handed)
    }

    pub fn looking_at(pos: V3, target: V3, right_handed: bool) -> Free {
        let d = normalize(sub(target, pos));
        let sx = if right_handed { -1.0 } else { 1.0 };
        Free { pos, yaw: (sx * d[0]).atan2(d[2]).to_degrees(), pitch: d[1].clamp(-1.0, 1.0).asin().to_degrees().clamp(-89.0, 89.0) }
    }

    pub fn camera(&self, fov: f32, right_handed: bool) -> Camera {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        // Yaw 0 looks along +Z; turning right goes towards screen right
        // (+X left-handed, -X right-handed).
        let sx = if right_handed { -1.0 } else { 1.0 };
        let f = [sx * sy * cp, sp, cy * cp];
        let r = right_of(f, [0.0, 1.0, 0.0], right_handed);
        let u = if right_handed { cross(r, f) } else { cross(f, r) };
        Camera { pos: self.pos, f, r, u, fov, right_handed }
    }

    pub fn look(&mut self, d_yaw: f32, d_pitch: f32) {
        self.yaw = (self.yaw + d_yaw + 180.0).rem_euclid(360.0) - 180.0;
        self.pitch = (self.pitch + d_pitch).clamp(-89.0, 89.0);
    }

    /// Moves along where `cam` (this camera) looks, its right, and world up.
    pub fn fly(&mut self, cam: &Camera, fwd: f32, right: f32, up: f32) {
        self.pos = add(add(add(self.pos, scale(cam.f, fwd)), scale(cam.r, right)), [0.0, up, 0.0]);
    }
}

/// What a camera sees, out to a distance: its position and the four corners
/// of its picture at that distance (top left, top right, bottom right, bottom
/// left), for a viewport `aspect` (width / height) wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    pub apex: V3,
    pub corners: [V3; 4],
    pub fov: f32,
}

impl Camera {
    pub fn frustum(&self, aspect: f32, length: f32) -> Frustum {
        let h = (self.fov.to_radians() * 0.5).tan() * length;
        let w = h * aspect;
        let c = add(self.pos, scale(self.f, length));
        let at = |x: f32, y: f32| add(add(c, scale(self.r, x * w)), scale(self.u, y * h));
        Frustum { apex: self.pos, corners: [at(-1.0, 1.0), at(1.0, 1.0), at(1.0, -1.0), at(-1.0, -1.0)], fov: self.fov }
    }
}

/// Clips a view-space segment to z > NEAR.
pub fn clip_near(a: V3, b: V3) -> Option<(V3, V3)> {
    let (ia, ib) = (a[2] > NEAR, b[2] > NEAR);
    match (ia, ib) {
        (true, true) => Some((a, b)),
        (false, false) => None,
        _ => {
            let t = (NEAR - a[2]) / (b[2] - a[2]);
            let m = add(a, scale(sub(b, a), t));
            Some(if ia { (a, m) } else { (m, b) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))
    }
    fn level() -> Pose {
        Pose { pos: [0.0; 3], fwd: [0.0, 0.0, 1.0], up: [0.0, 1.0, 0.0] }
    }
    fn close(a: V3, b: V3) -> bool {
        len(sub(a, b)) < 1e-4
    }

    #[test]
    fn left_handed_default_faces_plus_z_with_plus_x_right() {
        let c = Camera::from_pose(&level(), &Offset::default(), 70.0, false);
        assert!(close(c.r, [1.0, 0.0, 0.0]) && close(c.u, [0.0, 1.0, 0.0]));
        let ahead = c.project([0.0, 0.0, 10.0], rect()).unwrap();
        assert!((ahead.x - 400.0).abs() < 0.01 && (ahead.y - 300.0).abs() < 0.01);
        assert!(c.project([3.0, 0.0, 10.0], rect()).unwrap().x > 400.0);
        assert!(c.project([0.0, 2.0, 10.0], rect()).unwrap().y < 300.0);
        assert!(c.project([0.0, 0.0, -5.0], rect()).is_none());
    }

    #[test]
    fn right_handed_puts_plus_x_on_the_right_facing_minus_z() {
        let p = Pose { fwd: [0.0, 0.0, -1.0], ..level() };
        let c = Camera::from_pose(&p, &Offset::default(), 70.0, true);
        assert!(close(c.r, [1.0, 0.0, 0.0]), "OpenGL-style: facing -Z, +X is right");
        assert!(close(c.u, [0.0, 1.0, 0.0]));
    }

    #[test]
    fn a_top_down_listener_looks_down() {
        // A real top-down capture's listener: 60 degrees down, up tilted forward.
        let p = Pose { pos: [0.0, 132.7, 0.0], fwd: [0.0, -0.866025, 0.5], up: [0.0, 0.5, 0.866025] };
        let c = Camera::from_pose(&p, &Offset::default(), 70.0, false);
        let below = c.project([0.0, 0.0, 132.7 * 0.5 / 0.866025], rect()).unwrap();
        assert!((below.x - 400.0).abs() < 0.5 && (below.y - 300.0).abs() < 0.5, "the ground point it faces is centred");
        assert_eq!(c.ground_heading(), (0.0, 1.0));
    }

    #[test]
    fn offset_moves_in_the_listener_frame_and_turns() {
        let off = Offset { right: 1.0, up: 2.0, back: 5.0, yaw: 0.0, pitch: 0.0 };
        let c = Camera::from_pose(&level(), &off, 70.0, false);
        assert!(close(c.pos, [1.0, 2.0, -5.0]));
        let c = Camera::from_pose(&level(), &Offset { yaw: 90.0, ..Default::default() }, 70.0, false);
        assert!(close(c.f, [1.0, 0.0, 0.0]), "yaw +90 turns right, to +X");
        let c = Camera::from_pose(&level(), &Offset { pitch: -30.0, ..Default::default() }, 70.0, false);
        assert!(c.f[1] < -0.49 && c.f[1] > -0.51, "pitch -30 looks down");
        let c = Camera::from_pose(&Pose { fwd: [0.0, 0.0, -1.0], ..level() }, &Offset { yaw: 90.0, ..Default::default() }, 70.0, true);
        assert!(close(c.f, [1.0, 0.0, 0.0]), "right-handed: yaw +90 also turns right");
    }

    #[test]
    fn turning_a_top_down_listener_round_still_looks_down() {
        let p = Pose { pos: [0.0, 132.7, 0.0], fwd: [0.0, -0.866025, 0.5], up: [0.0, 0.5, 0.866025] };
        let c = Camera::from_pose(&p, &Offset { yaw: 180.0, ..Default::default() }, 70.0, false);
        assert!(close(c.f, [0.0, -0.866025, -0.5]), "{:?}", c.f);
        assert!(c.u[1] > 0.49, "still upright");
    }

    #[test]
    fn segments_crossing_the_camera_are_clipped() {
        let c = Camera::default();
        let (a, b) = c.project_segment([1.0, 0.0, -10.0], [1.0, 0.0, 10.0], rect()).unwrap();
        assert!(a.x.is_finite() && b.x.is_finite());
        assert!(c.project_segment([0.0, 0.0, -10.0], [1.0, 0.0, -1.0], rect()).is_none());
    }

    #[test]
    fn a_free_camera_looks_where_it_was_pointed_in_both_handednesses() {
        for rh in [false, true] {
            let target = [3.0, -2.0, 7.0];
            let f = Free::looking_at([1.0, 4.0, -5.0], target, rh);
            let c = f.camera(70.0, rh);
            let v = c.to_view(target);
            assert!(v[0].abs() < 1e-3 && v[1].abs() < 1e-3 && v[2] > 0.0, "{rh}: {v:?}");
            assert!(c.r[1].abs() < 1e-5, "level: no roll");
            // Turning right moves the old centre to the left of the screen.
            let mut g = f;
            g.look(10.0, 0.0);
            assert!(g.camera(70.0, rh).to_view(target)[0] < 0.0, "{rh}");
        }
    }

    #[test]
    fn behind_puts_the_followed_camera_in_front_and_frustum_corners_on_its_edges() {
        let lc = Camera { pos: [0.0, 1.0, 0.0], ..Camera::default() };
        let free = Free::behind(&lc, 10.0).camera(70.0, false);
        assert!(free.to_view(lc.pos)[2] > 0.0);
        let fr = lc.frustum(2.0, 5.0);
        for (k, c) in fr.corners.iter().enumerate() {
            let v = lc.to_view(*c);
            assert!((v[2] - 5.0).abs() < 1e-4);
            let p = lc.project_view(v, Rect::from_min_size(pos2(0.0, 0.0), eframe::egui::vec2(200.0, 100.0)));
            assert!((p.x - [0.0, 200.0, 200.0, 0.0][k]).abs() < 0.01 && (p.y - [0.0, 0.0, 100.0, 100.0][k]).abs() < 0.01, "{k}: {p:?}");
        }
    }
}
