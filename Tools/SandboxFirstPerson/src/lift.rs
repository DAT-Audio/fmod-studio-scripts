//! Elevation: fakes height, which the Sandbox can't represent, by moving
//! emitters along the ground.
//!
//! When you fly up or down by `h`, each distance-reactive emitter is moved, in
//! the listener's frame (facing = camera yaw):
//!   - its **left/right offset is kept** (it's what places a sound left or
//!     right), and
//!   - its **front/back offset absorbs the height**: √(front² + h²), so its
//!     distance from the listener is your true 3D distance to it.
//! This is exact for every emitter at once, rows of street emitters included,
//! and a sound below you sits near the centre. The listener stays under you.
//!
//! Only emitters that react to distance move: a spatializer with a distance
//! curve, or a property automated on built-in Distance / Distance
//! (Normalized). 2D events, curve-Off spatializers and snapshots stay put.
//!
//! History: 0.1.4–0.2.0 moved the *listener* to a best fit instead. That
//! can't be right for a row of emitters, which each want a different shift.
//! They chose moving emitters, with
//! Elevation off by default and an explanation shown when it's turned on.

use crate::scene::{Emitter, Scene};
use std::collections::HashMap;

/// Below this height nothing is moved.
pub const MIN_HEIGHT: f32 = 0.005;

/// Which side (front +1 / back -1) each emitter was put on, so it only flips
/// once you've clearly passed over it.
pub type Sides = HashMap<String, f32>;

/// Does this emitter's level depend on distance (so it should be moved)?
pub fn reacts_to_distance(e: &Emitter) -> bool {
    !e.is_snapshot() && e.spatial.as_ref().is_some_and(|s| s.distance_range().is_some())
}

/// Where each distance-reactive emitter goes for a camera at (x, h, z) facing
/// `yaw`: (id, x, z). Empty near the plane.
pub fn targets(scene: &Scene, x: f32, z: f32, h: f32, yaw: f32, sides: &mut Sides) -> Vec<(String, f32, f32)> {
    let h = h.abs();
    if h < MIN_HEIGHT {
        sides.clear();
        return Vec::new();
    }
    let (s, c) = yaw.to_radians().sin_cos();
    let (fx, fz) = (s, c); // forward
    let (rx, rz) = (c, -s); // right
    let mut out = Vec::new();
    for e in scene.emitters.iter().filter(|e| reacts_to_distance(e)) {
        let (dx, dz) = (e.x - x, e.z - z);
        let fwd = dx * fx + dz * fz;
        let lat = dx * rx + dz * rz;
        let margin = 0.1 * h;
        let side = match sides.get(&e.id) {
            Some(&prev) if fwd * prev > -margin => prev,
            _ => if fwd >= 0.0 { 1.0 } else { -1.0 },
        };
        sides.insert(e.id.clone(), side);
        let fwd2 = side * (fwd * fwd + h * h).sqrt();
        out.push((e.id.clone(), x + lat * rx + fwd2 * fx, z + lat * rz + fwd2 * fz));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Spatial;

    fn emitter(id: &str, x: f32, z: f32) -> Emitter {
        Emitter {
            id: id.into(),
            name: id.into(),
            path: String::new(),
            kind: "Event".into(),
            x,
            z,
            rot: 0.0,
            loop_: false,
            spatial: Some(Spatial { min: 1.0, max: 30.0, is_3d: true, ..Default::default() }),
        }
    }

    fn scene(es: Vec<Emitter>) -> Scene {
        Scene { emitters: es, listeners: 1, ..Default::default() }
    }

    fn dist(t: &(String, f32, f32), x: f32, z: f32) -> f32 {
        ((t.1 - x).powi(2) + (t.2 - z).powi(2)).sqrt()
    }

    #[test]
    fn nothing_moves_near_the_plane() {
        assert!(targets(&scene(vec![emitter("a", 0.0, 10.0)]), 0.0, 0.0, 0.0, 0.0, &mut Sides::new()).is_empty());
    }

    #[test]
    fn a_whole_row_is_exact_at_once() {
        // A row of emitters ahead, at different distances.
        let row: Vec<_> = (0..8).map(|i| emitter(&format!("e{i}"), if i % 2 == 0 { -1.5 } else { 1.5 }, 2.0 + i as f32 * 1.5)).collect();
        let s = scene(row.clone());
        let t = targets(&s, 0.0, 0.0, 4.0, 0.0, &mut Sides::new());
        assert_eq!(t.len(), 8);
        for (e, m) in row.iter().zip(&t) {
            let d3 = (e.x * e.x + e.z * e.z + 16.0).sqrt();
            assert!((dist(m, 0.0, 0.0) - d3).abs() < 1e-3, "{}: heard {} want {}", e.id, dist(m, 0.0, 0.0), d3);
            assert!((m.1 - e.x).abs() < 1e-4, "{}: left/right offset kept", e.id);
        }
    }

    #[test]
    fn below_you_sits_near_the_centre_and_flips_only_once_past() {
        let s = scene(vec![emitter("a", 0.3, 0.0)]);
        let mut sides = Sides::new();
        let ahead = targets(&s, 0.0, -0.5, 5.0, 0.0, &mut sides);
        assert!((ahead[0].1 - 0.3).abs() < 1e-4 && ahead[0].2 > 4.0, "ahead, 0.3 right: {:?}", ahead[0]);
        let just = targets(&s, 0.0, 0.2, 5.0, 0.0, &mut sides);
        assert!(just[0].2 > 0.0, "no flip yet");
        let past = targets(&s, 0.0, 1.0, 5.0, 0.0, &mut sides);
        assert!(past[0].2 < 0.0, "flipped once clearly past");
    }

    #[test]
    fn only_distance_reactive_emitters_move() {
        let mut off = emitter("off", 0.0, 5.0);
        off.spatial.as_mut().unwrap().rolloff = 4;
        let mut flat = emitter("flat", 1.0, 5.0);
        flat.spatial.as_mut().unwrap().is_3d = false;
        let mut snap = emitter("snap", 2.0, 5.0);
        snap.kind = "Snapshot".into();
        let t = targets(&scene(vec![off, flat, snap, emitter("ok", 3.0, 5.0)]), 0.0, 0.0, 3.0, 0.0, &mut Sides::new());
        assert_eq!(t.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(), vec!["ok"]);
    }
}
