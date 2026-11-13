//! Automatic listener movements: spin on the spot, and walk a circle.
//!
//! Both move only what FMOD can hear: X/Z and facing (the Sandbox has no
//! height or roll). Any manual move stops them (app.rs).
//!
//! Angles follow the Sandbox: degrees clockwise seen from above, and a point
//! at angle `a` on a circle of radius `r` around `c` is
//! c + r * (sin a, cos a) in (X, Z), like forward = (sin yaw, cos yaw).

use crate::camera::{Camera, wrap_deg};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spin {
    Off,
    CwSlow,
    CwFast,
    CcwSlow,
    CcwFast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Face {
    /// Look where you're walking.
    Travel,
    /// Look at the circle's centre (e.g. the emitter you orbit).
    Centre,
    /// Leave facing alone.
    Keep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Centre {
    /// The circle passes through where you are, its centre beside you: no jump.
    Here,
    /// The circle goes round the emitter under the crosshair.
    Emitter,
}

/// The remembered settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub slow_dps: f32,
    pub fast_dps: f32,
    pub radius: f32,
    pub circle_speed: f32,
    pub centre: Centre,
    pub face: Face,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { slow_dps: 20.0, fast_dps: 90.0, radius: 5.0, circle_speed: 2.0, centre: Centre::Here, face: Face::Travel }
    }
}

/// A running circle walk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    /// +1 clockwise, -1 anticlockwise (seen from above, like the Sandbox).
    pub dir: f32,
    pub cx: f32,
    pub cz: f32,
    pub r: f32,
    /// Current angle, degrees.
    pub a: f32,
}

impl Circle {
    /// Starts a circle. `Here`: the centre is `r` to your right (clockwise)
    /// or left (anticlockwise), so walking starts where you stand, facing on.
    /// Around a point: you move onto the circle along the line from it.
    pub fn start(cam: &Camera, dir: f32, r: f32, around: Option<(f32, f32)>) -> Circle {
        let r = r.max(0.1);
        let (cx, cz) = match around {
            Some(c) => c,
            None => {
                let (s, c) = cam.yaw.to_radians().sin_cos();
                // Right of facing is (cos yaw, -sin yaw).
                (cam.x + dir * r * c, cam.z - dir * r * s)
            }
        };
        let (dx, dz) = (cam.x - cx, cam.z - cz);
        let a = if dx.abs() + dz.abs() < 1e-6 { cam.yaw + 180.0 } else { dx.atan2(dz).to_degrees() };
        Circle { dir, cx, cz, r, a }
    }

    pub fn point(&self) -> (f32, f32) {
        let (s, c) = self.a.to_radians().sin_cos();
        (self.cx + self.r * s, self.cz + self.r * c)
    }

    /// Facing while walking it.
    pub fn yaw(&self, face: Face) -> Option<f32> {
        match face {
            Face::Travel => Some(wrap_deg(self.a + 90.0 * self.dir)),
            Face::Centre => Some(wrap_deg(self.a + 180.0)),
            Face::Keep => None,
        }
    }

    /// Advances by `speed` m/s for `dt` seconds.
    pub fn step(&mut self, speed: f32, dt: f32) {
        self.a = wrap_deg(self.a + self.dir * (speed * dt / self.r).to_degrees());
    }
}

pub fn spin_rate(spin: Spin, s: &Settings) -> f32 {
    match spin {
        Spin::Off => 0.0,
        Spin::CwSlow => s.slow_dps,
        Spin::CwFast => s.fast_dps,
        Spin::CcwSlow => -s.slow_dps,
        Spin::CcwFast => -s.fast_dps,
    }
}

/// Moves the camera one frame. Spin overrides the circle's facing.
pub fn apply(cam: &mut Camera, spin: Spin, circle: Option<&mut Circle>, s: &Settings, dt: f32) {
    if let Some(c) = circle {
        c.step(s.circle_speed, dt);
        let (x, z) = c.point();
        cam.x = x;
        cam.z = z;
        if spin == Spin::Off {
            if let Some(y) = c.yaw(s.face) {
                cam.yaw = y;
            }
        }
    }
    let rate = spin_rate(spin, s);
    if rate != 0.0 {
        cam.yaw = wrap_deg(cam.yaw + rate * dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn here_circle_starts_where_you_stand_facing_on() {
        let cam = Camera { x: 1.0, z: 2.0, yaw: 0.0, ..Default::default() };
        let c = Circle::start(&cam, 1.0, 5.0, None);
        assert!(close(c.cx, 6.0) && close(c.cz, 2.0), "clockwise: centre on your right (+X when facing +Z)");
        let (x, z) = c.point();
        assert!(close(x, 1.0) && close(z, 2.0), "no jump");
        assert!(close(c.yaw(Face::Travel).unwrap(), 0.0), "facing on along the circle");
        let ccw = Circle::start(&cam, -1.0, 5.0, None);
        assert!(close(ccw.cx, -4.0));
        assert!(close(ccw.yaw(Face::Travel).unwrap(), 0.0));
    }

    #[test]
    fn clockwise_turns_right_and_keeps_the_radius() {
        let cam = Camera { x: 0.0, z: 0.0, yaw: 0.0, ..Default::default() };
        let mut c = Circle::start(&cam, 1.0, 5.0, None);
        c.step(2.0, 1.0);
        let (x, z) = c.point();
        assert!(x > 0.0 && z > 0.0, "moved forward and to the right: ({x}, {z})");
        assert!(close(((x - c.cx).powi(2) + (z - c.cz).powi(2)).sqrt(), 5.0));
        let yaw = c.yaw(Face::Travel).unwrap();
        assert!(yaw > 0.0 && yaw < 90.0, "turned right: {yaw}");
    }

    #[test]
    fn orbiting_a_point_faces_it_at_the_given_radius() {
        let cam = Camera { x: 0.0, z: -3.0, yaw: 0.0, ..Default::default() };
        let c = Circle::start(&cam, 1.0, 8.0, Some((0.0, 0.0)));
        let (x, z) = c.point();
        assert!(close(x, 0.0) && close(z, -8.0), "moved out to the radius, along the line from the centre");
        assert!(close(c.yaw(Face::Centre).unwrap(), 0.0), "looking at the centre (+Z)");
    }

    #[test]
    fn spin_rates_and_spin_wins_over_circle_facing() {
        let s = Settings::default();
        let mut cam = Camera::default();
        apply(&mut cam, Spin::CcwSlow, None, &s, 1.0);
        assert!(close(cam.yaw, 340.0));
        let mut cam = Camera::default();
        let mut c = Circle::start(&cam, 1.0, 5.0, None);
        apply(&mut cam, Spin::CwFast, Some(&mut c), &s, 0.5);
        assert!(close(cam.yaw, 45.0), "spin, not the circle, sets facing: {}", cam.yaw);
    }
}
