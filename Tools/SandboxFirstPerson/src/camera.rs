//! The first-person camera and perspective projection.
//!
//! World axes follow the Sandbox: X and Z are the ground plane (the only
//! coordinates FMOD has), Y is up and exists only in this view. `yaw` is the
//! Sandbox listener's `rotationY`: degrees clockwise seen from above, with
//! forward = (sin yaw, cos yaw) in (X, Z). So yaw 0 faces +Z, yaw 90 faces +X,
//! and +X is on your right when facing +Z (measured from Sandbox drags).

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub x: f32,
    /// Height above the Sandbox plane: view only, FMOD never sees it.
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    /// Degrees, positive looks up. View only.
    pub pitch: f32,
    /// Vertical field of view in degrees.
    pub fov: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0, fov: 70.0 }
    }
}

/// Wraps degrees into [0, 360).
pub fn wrap_deg(d: f32) -> f32 {
    let w = d.rem_euclid(360.0);
    if w >= 360.0 { 0.0 } else { w }
}

impl Camera {
    pub fn pos(&self) -> V3 {
        [self.x, self.y, self.z]
    }

    /// Forward, right and up unit vectors.
    pub fn basis(&self) -> (V3, V3, V3) {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        let f = [sy * cp, sp, cy * cp];
        let r = [cy, 0.0, -sy];
        (f, r, cross(f, r))
    }

    /// World point to view space: x right, y up, z forward (depth).
    pub fn to_view(&self, p: V3) -> V3 {
        let (f, r, u) = self.basis();
        let d = sub(p, self.pos());
        [dot(d, r), dot(d, u), dot(d, f)]
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

    /// Moves in camera space: `fwd` along the look direction (pitch included:
    /// free flying), `right` along the ground-level right vector, `up` along
    /// world Y.
    pub fn fly(&mut self, fwd: f32, right: f32, up: f32) {
        let (f, r, _) = self.basis();
        let d = add(add(scale(f, fwd), scale(r, right)), [0.0, up, 0.0]);
        self.x += d[0];
        self.y += d[1];
        self.z += d[2];
    }

    pub fn look(&mut self, d_yaw: f32, d_pitch: f32) {
        self.yaw = wrap_deg(self.yaw + d_yaw);
        self.pitch = (self.pitch + d_pitch).clamp(-89.0, 89.0);
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

    #[test]
    fn yaw_zero_faces_plus_z_with_plus_x_on_the_right() {
        let c = Camera::default();
        let ahead = c.project([0.0, 0.0, 10.0], rect()).unwrap();
        assert!((ahead.x - 400.0).abs() < 0.01 && (ahead.y - 300.0).abs() < 0.01);
        let right = c.project([3.0, 0.0, 10.0], rect()).unwrap();
        assert!(right.x > 400.0, "+X is on the right when facing +Z");
        let up = c.project([0.0, 2.0, 10.0], rect()).unwrap();
        assert!(up.y < 300.0, "+Y is up on screen");
        assert!(c.project([0.0, 0.0, -5.0], rect()).is_none(), "behind is culled");
    }

    #[test]
    fn yaw_90_faces_plus_x_and_turning_is_clockwise() {
        let c = Camera { yaw: 90.0, ..Default::default() };
        let ahead = c.project([10.0, 0.0, 0.0], rect()).unwrap();
        assert!((ahead.x - 400.0).abs() < 0.01);
        // Facing +X, clockwise from above, the right-hand side is -Z.
        let right = c.project([10.0, 0.0, -3.0], rect()).unwrap();
        assert!(right.x > 400.0);
    }

    #[test]
    fn flying_follows_the_look_direction() {
        let mut c = Camera { yaw: 90.0, ..Default::default() };
        c.fly(2.0, 0.0, 0.0);
        assert!((c.x - 2.0).abs() < 1e-5 && c.z.abs() < 1e-5);
        let mut c = Camera { pitch: 90.0_f32.min(89.0), ..Default::default() };
        c.fly(1.0, 0.0, 0.0);
        assert!(c.y > 0.99, "looking up and flying forward climbs");
        let mut c = Camera::default();
        c.fly(0.0, 1.0, 0.0);
        assert!((c.x - 1.0).abs() < 1e-5, "strafe right is +X when facing +Z");
    }

    #[test]
    fn look_wraps_yaw_and_clamps_pitch() {
        let mut c = Camera::default();
        c.look(-30.0, 200.0);
        assert!((c.yaw - 330.0).abs() < 1e-4);
        assert_eq!(c.pitch, 89.0);
    }

    #[test]
    fn segments_crossing_the_camera_are_clipped() {
        let c = Camera::default();
        let (a, b) = c.project_segment([1.0, 0.0, -10.0], [1.0, 0.0, 10.0], rect()).unwrap();
        assert!(a.x.is_finite() && b.x.is_finite());
        assert!(c.project_segment([0.0, 0.0, -10.0], [1.0, 0.0, -1.0], rect()).is_none());
    }
}
