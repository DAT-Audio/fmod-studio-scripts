//! Curve maths shared with the Studio script's copy (`cplShaped` / `cplCurveAt`).
//! Keep the two in step.
//!
//! Studio stores a segment's bend on its first point: 0 is straight, positive
//! starts slow, negative starts fast. With `s` set, the bend is the S-curve's
//! *strength*: 0 is a straight line (in Studio, a "Bell" made of S segments
//! with bend 0 draws as straight lines), positive eases both ends
//! (a classic S), negative is the inverse (fast ends, flat middle). FMOD
//! doesn't document the exact maths, so this is an approximation for display;
//! the stored values are what gets applied, so applying is exact.

use crate::library::Point;

/// Bends are stored in -1..=1.
pub const MAX_BEND: f64 = 1.0;

fn exponent(shape: f64) -> f64 {
    (1.0 + shape.abs() * 4.0).min(12.0)
}

/// How far along a segment's value range the curve is at `t` (0-1).
pub fn shaped(t: f64, shape: f64, s_curve: bool) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let k = exponent(shape);
    if s_curve {
        return if shape >= 0.0 {
            if t < 0.5 { 0.5 * (2.0 * t).powf(k) } else { 1.0 - 0.5 * (2.0 - 2.0 * t).powf(k) }
        } else if t < 0.5 {
            0.5 - 0.5 * (1.0 - 2.0 * t).powf(k)
        } else {
            0.5 + 0.5 * (2.0 * t - 1.0).powf(k)
        };
    }
    if shape == 0.0 {
        t
    } else if shape > 0.0 {
        t.powf(k)
    } else {
        1.0 - (1.0 - t).powf(k)
    }
}

/// Value at `x`, held flat outside the points. Points must be sorted by x.
pub fn value_at(points: &[Point], x: f64) -> f64 {
    let Some(first) = points.first() else { return 0.0 };
    if x <= first.x {
        return first.y;
    }
    for w in points.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if x <= b.x {
            let t = if b.x == a.x { 1.0 } else { (x - a.x) / (b.x - a.x) };
            return a.y + (b.y - a.y) * shaped(t, a.shape, a.s);
        }
    }
    points.last().map_or(0.0, |p| p.y)
}

/// A typical S-curve strength (used for new S segments).
pub const S_STRENGTH: f64 = 0.6;

/// The S-curve strength that puts a segment's quarter point at `fraction` of
/// the way from its start value to its end value. The inverse of
/// `shaped(0.25, strength, true)`.
pub fn s_strength_for_quarter(fraction: f64) -> f64 {
    let f = fraction.clamp(0.001, 0.499);
    if (f - 0.25).abs() < 1e-4 {
        return 0.0;
    }
    let half = 0.5f64.ln();
    let strength = if f < 0.25 {
        (((2.0 * f).ln() / half) - 1.0) / 4.0
    } else {
        -(((1.0 - 2.0 * f).ln() / half) - 1.0) / 4.0
    };
    strength.clamp(-MAX_BEND, MAX_BEND)
}

/// The bend that puts a segment's midpoint at `fraction` of the way from its
/// start value to its end value. The inverse of `shaped(0.5, bend, false)`.
pub fn bend_for_midpoint(fraction: f64) -> f64 {
    let f = fraction.clamp(0.001, 0.999);
    if (f - 0.5).abs() < 1e-4 {
        return 0.0;
    }
    let half = 0.5f64.ln();
    let bend = if f < 0.5 {
        ((f.ln() / half) - 1.0) / 4.0
    } else {
        -(((1.0 - f).ln() / half) - 1.0) / 4.0
    };
    bend.clamp(-MAX_BEND, MAX_BEND)
}

/// Segment shapes. FMOD's automation points support three natively (a
/// straight line, a bend, an S-curve); the compound ones are built from extra
/// points, so they look and play the same in Studio once applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SegmentShape {
    Linear,
    /// Keeps the current bend, or adds a gentle one.
    Curve,
    SCurve,
    /// Two S-curves: a smooth step with a flat middle.
    DoubleS,
    /// Three S-curves: smooth stairs.
    TripleS,
    /// `n` hard steps.
    Steps(u8),
    /// A wave of `n` cycles around the straight line.
    Wave(u8),
    /// Goes past the end value, then settles back.
    Overshoot,
    /// Arrives, then bounces back and settles three times.
    Bounce,
}

impl SegmentShape {
    pub fn all() -> Vec<(SegmentShape, &'static str)> {
        use SegmentShape::*;
        vec![
            (Linear, "Linear"),
            (Curve, "Single curve (bend)"),
            (SCurve, "S-curve"),
            (DoubleS, "Double S (smooth step)"),
            (TripleS, "Triple S (smooth stairs)"),
            (Steps(2), "Steps ×2"),
            (Steps(4), "Steps ×4"),
            (Steps(8), "Steps ×8"),
            (Wave(1), "Wave ×1"),
            (Wave(2), "Wave ×2"),
            (Wave(4), "Wave ×4"),
            (Overshoot, "Overshoot"),
            (Bounce, "Bounce"),
        ]
    }
    pub fn is_native(self) -> bool {
        matches!(self, SegmentShape::Linear | SegmentShape::Curve | SegmentShape::SCurve)
    }
}

/// Reshapes the segment from `pts[i]` to `pts[i + 1]`. Native shapes change
/// the first point's bend/S; compound shapes insert points between the two.
/// The end points never move. Returns the new points.
pub fn reshape_segment(pts: &[Point], i: usize, shape: SegmentShape) -> Vec<Point> {
    if i + 1 >= pts.len() {
        return pts.to_vec();
    }
    let (a, b) = (pts[i].clone(), pts[i + 1].clone());
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let at = |t: f64, y: f64, shape: f64, s: bool| Point { x: a.x + dx * t, y: y.clamp(0.0, 1.0), shape, s };
    let mut first = a.clone();
    first.shape = 0.0;
    first.s = false;
    let mut inner: Vec<Point> = Vec::new();
    match shape {
        SegmentShape::Linear => {}
        SegmentShape::Curve => {
            first.shape = if a.shape != 0.0 && !a.s { a.shape } else if dy >= 0.0 { -0.4 } else { 0.4 };
        }
        SegmentShape::SCurve => {
            first.s = true;
            first.shape = if a.s && a.shape != 0.0 { a.shape } else { S_STRENGTH };
        }
        SegmentShape::DoubleS | SegmentShape::TripleS => {
            // Smooth steps: each sub-segment is an S, so it flattens at every step.
            let n = if shape == SegmentShape::DoubleS { 2 } else { 3 };
            first.s = true;
            first.shape = 0.8;
            for k in 1..n {
                let t = k as f64 / n as f64;
                inner.push(at(t, a.y + dy * t, 0.8, true));
            }
        }
        SegmentShape::Steps(n) => {
            // n levels from a's value to b's: hold each, jump at k/n.
            let n = n.max(2) as usize;
            let eps = (dx / n as f64) * 0.02;
            let level = |k: usize| a.y + dy * k as f64 / (n - 1) as f64;
            for k in 1..n {
                let t = k as f64 / n as f64;
                inner.push(Point { x: a.x + dx * t - eps, y: level(k - 1).clamp(0.0, 1.0), shape: 0.0, s: false });
                inner.push(at(t, level(k), 0.0, false));
            }
        }
        SegmentShape::Wave(n) => {
            let n = n.max(1) as usize;
            let amp = 0.15f64.min(dy.abs().max(0.1));
            first.s = true;
            first.shape = S_STRENGTH;
            for k in 1..(n * 2) {
                let t = k as f64 / (n * 2) as f64;
                let off = if k % 2 == 1 { amp } else { -amp };
                inner.push(at(t, a.y + dy * t + off, S_STRENGTH, true));
            }
        }
        SegmentShape::Overshoot => {
            first.shape = if dy >= 0.0 { -0.5 } else { 0.5 };
            let over = if dy.abs() < 1e-9 { 0.1 } else { 0.15 * dy };
            inner.push(at(0.7, b.y + over, S_STRENGTH, true));
        }
        SegmentShape::Bounce => {
            first.shape = if dy >= 0.0 { 0.6 } else { -0.6 };
            let mut t = 0.55;
            let mut h = 0.35;
            for _ in 0..3 {
                let w = (1.0 - t) * 0.45;
                inner.push(at(t, b.y, S_STRENGTH, true));
                inner.push(at(t + w / 2.0, b.y - dy * h, S_STRENGTH, true));
                t += w;
                h *= 0.45;
            }
        }
    }
    let mut out = pts[..i].to_vec();
    out.push(first);
    out.extend(inner);
    out.extend_from_slice(&pts[i + 1..]);
    out
}

/// Whole-curve edits: Reverse (left to right), Flip (upside down) and
/// Mirror (there and back). Each keeps every segment's shape: a bend lives on
/// the segment's first point, so reversing moves it to the other end and
/// negates it (slow-start becomes slow-finish); an S-curve is symmetric, so it
/// keeps its strength. Flipping keeps bends as they are (a slow start stays slow).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transform {
    Reverse,
    Flip,
    Mirror,
}

impl Transform {
    pub fn all() -> [(Transform, &'static str, &'static str); 3] {
        [
            (Transform::Reverse, "Reverse", "Plays the curve backwards: the end becomes the start (a rise becomes a fall)"),
            (Transform::Flip, "Flip", "Turns the curve upside down: high values become low (a rise becomes a fall, a bell becomes a dip)"),
            (Transform::Mirror, "Mirror", "There and back: squeezes the curve into the first half and adds its reverse in the second half (a rise becomes a bell)"),
        ]
    }
}

pub fn transform(pts: &[Point], t: Transform) -> Vec<Point> {
    match t {
        Transform::Reverse => reversed(pts),
        Transform::Flip => pts.iter().map(|q| Point { y: tidy(1.0 - q.y), ..q.clone() }).collect(),
        Transform::Mirror => {
            if pts.len() < 2 {
                return pts.to_vec();
            }
            let first: Vec<Point> = pts.iter().map(|q| Point { x: tidy(q.x * 0.5), ..q.clone() }).collect();
            let second: Vec<Point> = reversed(pts).into_iter().map(|q| Point { x: tidy(0.5 + q.x * 0.5), ..q }).collect();
            // The two halves meet at x = 0.5: keep one point there, carrying
            // the second half's first segment.
            let mut out = first[..first.len() - 1].to_vec();
            out.extend(second);
            out
        }
    }
}

/// Rounds off float noise (1 - 0.7 = 0.30000000000000004), so files stay tidy
/// and transforming twice gives the curve back exactly.
fn tidy(v: f64) -> f64 {
    (v * 1e9).round() / 1e9
}

fn reversed(pts: &[Point]) -> Vec<Point> {
    let n = pts.len();
    (0..n)
        .map(|j| {
            let q = &pts[n - 1 - j];
            // The segment after new point j is the old segment before old point n-1-j.
            let (shape, s) = if j + 1 < n {
                let seg = &pts[n - 2 - j];
                (if seg.s { seg.shape } else { -seg.shape }, seg.s)
            } else {
                (0.0, false)
            };
            Point { x: tidy(1.0 - q.x), y: q.y, shape, s }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, shape: f64, s: bool) -> Point {
        Point { x, y, shape, s }
    }

    #[test]
    fn linear_and_flat_ends() {
        let pts = [p(0.2, 0.3, 0.0, false), p(0.8, 0.9, 0.0, false)];
        assert!((value_at(&pts, 0.5) - 0.6).abs() < 1e-12);
        assert_eq!(value_at(&pts, 0.0), 0.3);
        assert_eq!(value_at(&pts, 1.0), 0.9);
    }

    #[test]
    fn bend_direction_matches_script() {
        assert!(shaped(0.5, 0.6, false) < 0.5);
        assert!(shaped(0.5, -0.6, false) > 0.5);
        assert!((shaped(0.5, 0.0, true) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn s_curve_strength() {
        assert!((shaped(0.25, 0.0, true) - 0.25).abs() < 1e-12, "strength 0 is a straight line, as in Studio");
        assert!(shaped(0.25, 0.6, true) < 0.2, "positive eases the ends");
        assert!(shaped(0.25, -0.6, true) > 0.3, "negative is fast at the ends");
        assert!((shaped(0.5, 0.6, true) - 0.5).abs() < 1e-12 && (shaped(0.5, -0.6, true) - 0.5).abs() < 1e-12);
        for &b in &[-1.0, -0.5, -0.1, 0.0, 0.2, 0.6, 1.0] {
            let f = shaped(0.25, b, true);
            assert!((s_strength_for_quarter(f) - b).abs() < 1e-9, "strength {b}");
        }
    }

    #[test]
    fn bend_solver_round_trips() {
        for &b in &[-1.0, -0.6, -0.434, -0.1, 0.0, 0.1, 0.47, 1.0] {
            let f = shaped(0.5, b, false);
            assert!((bend_for_midpoint(f) - b).abs() < 1e-9, "bend {b}");
        }
    }

    #[test]
    fn reshape_keeps_end_points_and_order() {
        let base = vec![p(0.1, 0.2, 0.0, false), p(0.8, 0.9, 0.0, false), p(1.0, 0.0, 0.0, false)];
        for (shape, name) in SegmentShape::all() {
            let out = reshape_segment(&base, 0, shape);
            assert_eq!((out[0].x, out[0].y), (0.1, 0.2), "{name}");
            let last = out.iter().position(|q| q.x == 0.8 && q.y == 0.9).expect(name);
            assert_eq!(out.len() - last, 2, "{name}: the rest is untouched");
            assert!(out.windows(2).all(|w| w[0].x <= w[1].x), "{name}: x order {out:?}");
            assert!(out.iter().all(|q| (0.0..=1.0).contains(&q.y)), "{name}");
            if shape.is_native() {
                assert_eq!(out.len(), base.len(), "{name}: native shapes add no points");
            } else {
                assert!(out.len() > base.len(), "{name}: compound shapes add points");
            }
        }
        let sc = reshape_segment(&base, 0, SegmentShape::SCurve);
        assert!(sc[0].s && sc[0].shape > 0.0, "a new S-curve has a real strength (0 would be straight in Studio)");
        let steps = reshape_segment(&base, 0, SegmentShape::Steps(4));
        let levels: Vec<f64> = steps[..steps.len() - 1].iter().map(|q| (q.y * 1000.0).round()).collect();
        assert!(levels.windows(2).all(|w| w[0] <= w[1]), "steps rise monotonically: {levels:?}");
    }

    fn same_shape(a: &[Point], f: impl Fn(f64) -> f64) {
        for i in 0..=100 {
            let x = i as f64 / 100.0;
            let (got, want) = (value_at(a, x), f(x));
            assert!((got - want).abs() < 1e-9, "at {x}: {got} vs {want}");
        }
    }

    #[test]
    fn reverse_flip_and_mirror_keep_every_segment_shape() {
        // Bends, an S-curve, and uneven spacing.
        let c = vec![p(0.0, 0.1, 0.6, false), p(0.3, 0.8, 0.5, true), p(0.7, 0.4, -0.3, false), p(1.0, 1.0, 0.0, false)];
        let r = transform(&c, Transform::Reverse);
        same_shape(&r, |x| value_at(&c, 1.0 - x));
        assert_eq!(transform(&r, Transform::Reverse), c, "reversing twice gives the curve back");
        let f = transform(&c, Transform::Flip);
        same_shape(&f, |x| 1.0 - value_at(&c, x));
        let m = transform(&c, Transform::Mirror);
        same_shape(&m, |x| if x <= 0.5 { value_at(&c, x * 2.0) } else { value_at(&c, 2.0 - x * 2.0) });
        assert_eq!(m.len(), c.len() * 2 - 1, "one shared point in the middle");
        assert!(m.windows(2).all(|w| w[0].x <= w[1].x));
    }
}
