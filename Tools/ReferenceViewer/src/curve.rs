//! Automation curve values, as measured against the FMOD 2.03.14 runtime
//! (data in `Tests/calibration/data/`): 23 curves played through the engine and
//! metered; these formulas reproduce them to within 0.18 dB.
//!
//! - A segment's bend is stored on its first point (`curveShape`, -1..1, and
//!   `isSCurve`). `shaped` gives how far along the segment's range the value is.
//! - Volume-like properties (dB) are interpolated on a fader scale, not in dB:
//!   plain dB from +10 down to -20 dB, compressed below (a measured table).
//!   Halfway along -40 → 0 dB is -16 dB, not -20.
//! - Other properties interpolate in their own units with the same bend
//!   (assumed: only volume was measured).

use crate::model::Point;

/// Bend constant for plain (non-S) bends: u = (e^(c·s·t) − 1) / (e^(c·s) − 1).
const BEND_C: f64 = 6.96;
/// Volume at or below this is silence.
pub const SILENCE_DB: f64 = -80.0;

/// The measured fader table below -20 dB: (dB, fader units), 1 dB steps.
const FADER_CSV: &str = include_str!("../../../Tests/calibration/data/fader_law_2_03_14.csv");

fn fader_table() -> &'static [(f64, f64)] {
    use std::sync::OnceLock;
    static T: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t: Vec<(f64, f64)> = FADER_CSV
            .lines()
            .filter_map(|l| {
                let mut it = l.split(',');
                Some((it.next()?.trim().parse().ok()?, it.next()?.trim().parse().ok()?))
            })
            .collect();
        // -80 dB sits at -42 (measured by the slope of the -80 → 0 curve);
        // -65..-80 is extrapolated straight.
        t.push((SILENCE_DB, -42.0));
        t.sort_by(|a, b| a.0.total_cmp(&b.0));
        t
    })
}

/// dB → fader units.
pub fn fader(db: f64) -> f64 {
    if db >= -20.0 {
        return db;
    }
    let t = fader_table();
    if db <= t[0].0 {
        return t[0].1;
    }
    for w in t.windows(2) {
        let ((a, ga), (b, gb)) = (w[0], w[1]);
        if db <= b {
            return ga + (gb - ga) * (db - a) / (b - a);
        }
    }
    -20.0
}

/// Fader units → dB.
pub fn fader_inv(g: f64) -> f64 {
    if g >= -20.0 {
        return g;
    }
    let t = fader_table();
    if g <= t[0].1 {
        return f64::NEG_INFINITY;
    }
    for w in t.windows(2) {
        let ((a, ga), (b, gb)) = (w[0], w[1]);
        if g <= gb {
            return a + (b - a) * (g - ga) / (gb - ga);
        }
    }
    -20.0
}

fn exp_bend(t: f64, a: f64) -> f64 {
    if a.abs() < 1e-9 { t } else { ((a * t).exp() - 1.0) / (a.exp() - 1.0) }
}

/// How far along a segment's value range the curve is at `t` (0..1).
pub fn shaped(t: f64, shape: f64, s_curve: bool) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if !s_curve {
        return exp_bend(t, BEND_C * shape);
    }
    if shape >= 0.0 {
        let k = 1.0 + 2.0 * shape;
        if t < 0.5 { 0.5 * (2.0 * t).powf(k) } else { 1.0 - 0.5 * (2.0 - 2.0 * t).powf(k) }
    } else {
        let inv3 = if t < 0.5 { 0.5 - 0.5 * (1.0 - 2.0 * t).powi(3) } else { 0.5 + 0.5 * (2.0 * t - 1.0).powi(3) };
        let w = shape.abs().min(1.0);
        (1.0 - w) * t + w * inv3
    }
}

/// Properties whose values are dB and follow the fader scale.
pub fn is_db(property: &str) -> bool {
    matches!(property, "volume" | "gain" | "level" | "inputGain" | "baseWetLevel" | "baseDryLevel")
}

/// The curve's value at `x`, held flat outside its points. `db` selects the
/// fader-scale interpolation. Points must be sorted by x.
pub fn value_at(points: &[Point], x: f64, db: bool) -> Option<f64> {
    let first = points.first()?;
    if x <= first.x {
        return Some(first.y);
    }
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x <= b.x {
            let t = if b.x == a.x { 1.0 } else { (x - a.x) / (b.x - a.x) };
            let u = shaped(t, a.shape, a.s);
            if db {
                if a.y <= SILENCE_DB && b.y <= SILENCE_DB {
                    return Some(f64::NEG_INFINITY);
                }
                let (ga, gb) = (fader(a.y), fader(b.y));
                return Some(fader_inv(ga + u * (gb - ga)));
            }
            return Some(a.y + u * (b.y - a.y));
        }
    }
    points.last().map(|p| p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEASURED: &str = include_str!("../../../Tests/calibration/data/measured_2_03_14.csv");

    fn case(id: &str) -> Option<(f64, bool, f64, f64)> {
        // (bend, S-curve, value at 0, value at 1).
        Some(match id {
            "lin" => (0.0, false, -40.0, 0.0),
            "p025" => (0.25, false, -40.0, 0.0),
            "p05" => (0.5, false, -40.0, 0.0),
            "p075" => (0.75, false, -40.0, 0.0),
            "p1" => (1.0, false, -40.0, 0.0),
            "m025" => (-0.25, false, -40.0, 0.0),
            "m05" => (-0.5, false, -40.0, 0.0),
            "m1" => (-1.0, false, -40.0, 0.0),
            "s0" => (0.0, true, -40.0, 0.0),
            "s03" => (0.3, true, -40.0, 0.0),
            "s06" => (0.6, true, -40.0, 0.0),
            "s1" => (1.0, true, -40.0, 0.0),
            "sm03" => (-0.3, true, -40.0, 0.0),
            "sm06" => (-0.6, true, -40.0, 0.0),
            "sm1" => (-1.0, true, -40.0, 0.0),
            "r20_0" => (0.0, false, -20.0, 0.0),
            "r10_10" => (0.0, false, -10.0, 10.0),
            "r60_20" => (0.0, false, -60.0, -20.0),
            "r30_6" => (0.0, false, -30.0, -6.0),
            "r5_0" => (0.0, false, -5.0, 0.0),
            "p05_r20_0" => (0.5, false, -20.0, 0.0),
            _ => return None,
        })
    }

    #[test]
    fn reproduces_the_measured_runtime_curves() {
        let rows: Vec<(String, f64, f64)> = MEASURED
            .lines()
            .skip(1)
            .filter_map(|l| {
                let v: Vec<&str> = l.split(',').collect();
                Some((v.first()?.to_string(), v.get(1)?.parse().ok()?, v.get(2)?.parse().ok()?))
            })
            .collect();
        // The tone's level at 0 dB volume.
        let reference = rows.iter().find(|r| r.0 == "lin" && r.1 == 1.0).unwrap().2;
        let (mut worst, mut n) = (0.0f64, 0);
        for (id, x, db) in &rows {
            let Some((shape, s, lo, hi)) = case(id) else { continue };
            let measured = db - reference;
            // Skip the first block (meter lag) and near-silence.
            if *x < 0.05 || *x > 0.97 || measured < -64.0 {
                continue;
            }
            let pts = [Point { x: 0.0, y: lo, shape, s }, Point { x: 1.0, y: hi, shape: 0.0, s: false }];
            let got = value_at(&pts, *x, true).unwrap();
            worst = worst.max((got - measured).abs());
            n += 1;
        }
        assert!(n > 500, "{n} samples");
        assert!(worst < 0.25, "worst error {worst:.3} dB");
    }

    #[test]
    fn halfway_along_minus_40_to_0_is_minus_16() {
        let pts = [Point { x: 0.0, y: -40.0, shape: 0.0, s: false }, Point { x: 1.0, y: 0.0, shape: 0.0, s: false }];
        assert!((value_at(&pts, 0.5, true).unwrap() + 16.0).abs() < 0.1);
        // Not dB: straight in its own units.
        assert!((value_at(&pts, 0.5, false).unwrap() + 20.0).abs() < 1e-9);
        // Held flat outside.
        assert_eq!(value_at(&pts, -3.0, true), Some(-40.0));
        assert_eq!(value_at(&pts, 9.0, true), Some(0.0));
    }

    #[test]
    fn fader_scale_round_trips() {
        for db in [-75.0, -60.0, -40.0, -33.3, -20.0, -6.0, 0.0, 10.0] {
            assert!((fader_inv(fader(db)) - db).abs() < 1e-6, "{db}");
        }
        assert_eq!(fader_inv(-50.0), f64::NEG_INFINITY);
    }

    #[test]
    fn shapes_hit_their_ends() {
        for (s, sc) in [(0.0, false), (0.7, false), (-0.7, false), (0.6, true), (-0.6, true), (0.0, true)] {
            assert!(shaped(0.0, s, sc).abs() < 1e-9 && (shaped(1.0, s, sc) - 1.0).abs() < 1e-9);
        }
        assert!((shaped(0.3, 0.0, true) - 0.3).abs() < 1e-9, "an S-curve with bend 0 is straight");
    }
}
