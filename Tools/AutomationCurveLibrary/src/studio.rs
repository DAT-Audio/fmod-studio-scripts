//! What the app knows about the FMOD project, from the Studio script's
//! `snapshot` reply (`aclSnapshot`): apply targets from Studio's selection,
//! every shared effect, and the project's parameters.

use crate::library::Point;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(default)]
    pub selection: Vec<Target>,
    #[serde(default)]
    pub shared_effects: Vec<Target>,
    #[serde(default)]
    pub parameters: Vec<Param>,
}

/// One `snapshot` reply. A light reply (polled often) has no shared effects
/// (`null`): the last full list is kept.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    #[serde(default)]
    pub selection: Vec<Target>,
    #[serde(default)]
    pub shared_effects: Option<Vec<Target>>,
    #[serde(default)]
    pub parameters: Vec<Param>,
}

impl Snapshot {
    pub fn merge(previous: Option<&Snapshot>, reply: Reply) -> Snapshot {
        Snapshot {
            selection: reply.selection,
            shared_effects: reply.shared_effects.unwrap_or_else(|| previous.map(|p| p.shared_effects.clone()).unwrap_or_default()),
            parameters: reply.parameters,
        }
    }
}

/// Something a curve can be applied to: an automatable object in Studio.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub kind_label: String,
    pub label: String,
    #[serde(default, rename = "where")]
    pub place: String,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub event_name: String,
    #[serde(default)]
    pub entity: String,
    #[serde(default)]
    pub uses: Option<u32>,
    #[serde(default)]
    pub selected_curve: Option<String>,
    #[serde(default)]
    pub timeline_estimated: bool,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub properties: Vec<PropertyInfo>,
    #[serde(default)]
    pub parameters: Vec<ParamRef>,
    #[serde(default)]
    pub timeline_length: Option<f64>,
    #[serde(default)]
    pub prefer: Option<Prefer>,
    /// A spatializer's built-in distance attenuation (display only).
    #[serde(default)]
    pub spatial: Option<Spatial>,
}

/// A spatializer's distance attenuation: a built-in mode over a min/max
/// distance. FMOD 2.02-2.04 can't give it a custom curve; it's shown so you
/// can see it (and copy it to a library).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Spatial {
    pub rolloff: String,
    /// LinearSquared 0, Linear 1, Inverse 2, InverseTapered 3, Off 4.
    pub rolloff_type: u32,
    pub min: f64,
    pub max: f64,
    /// Where min/max come from: "effect", "event" or "each event".
    #[serde(default)]
    pub source: String,
}

impl Spatial {
    /// Gain (linear amplitude, 0-1) at distance `d`, per the effect reference:
    /// unattenuated inside min; Linear Squared and Linear reach silence at
    /// max; Inverse is min/d and stops attenuating at max; Inverse Tapered
    /// is the lower of Inverse and Linear Squared; Off is no attenuation.
    pub fn gain(&self, d: f64) -> f64 {
        let (lo, hi) = (self.min.max(1e-6), self.max.max(self.min + 1e-6));
        let t = ((d - lo) / (hi - lo)).clamp(0.0, 1.0);
        let linear_sq = (1.0 - t) * (1.0 - t);
        let inverse = lo / d.clamp(lo, hi);
        match self.rolloff_type {
            0 => linear_sq,
            1 => 1.0 - t,
            2 => inverse,
            3 => inverse.min(linear_sq),
            _ => 1.0,
        }
    }

    pub fn is_off(&self) -> bool {
        self.rolloff_type >= 4
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct PropertyInfo {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub automated: bool,
    #[serde(default)]
    pub lo: Option<f64>,
    #[serde(default)]
    pub hi: Option<f64>,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub curves: Vec<CurveInfo>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParamRef {
    pub id: String,
    #[serde(default)]
    pub in_event: bool,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prefer {
    pub property: String,
    #[serde(default)]
    pub parameter_id: Option<String>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurveInfo {
    pub parameter_id: String,
    pub parameter_name: String,
    pub pmin: Option<f64>,
    pub pmax: Option<f64>,
    #[serde(default)]
    pub points: Vec<RawPoint>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RawPoint {
    pub position: f64,
    pub value: f64,
    #[serde(default)]
    pub shape: f64,
    #[serde(default)]
    pub s: bool,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Param {
    pub id: String,
    pub name: String,
    pub min: f64,
    pub max: f64,
    #[serde(default)]
    pub global: bool,
    #[serde(default)]
    pub timeline: bool,
    #[serde(default)]
    pub estimated: bool,
}

/// What a 0-1 shape is stretched over: positions plo..phi, values vlo..vhi.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub plo: f64,
    pub phi: f64,
    pub vlo: f64,
    pub vhi: f64,
}

impl Snapshot {
    pub fn param(&self, id: &str) -> Option<&Param> {
        self.parameters.iter().find(|p| p.id == id)
    }
    /// Selection targets first, then every shared effect not already listed.
    pub fn all_targets(&self) -> Vec<&Target> {
        let mut out: Vec<&Target> = self.selection.iter().collect();
        for t in &self.shared_effects {
            if !self.selection.iter().any(|s| s.id == t.id && s.prefer.is_none()) {
                out.push(t);
            }
        }
        out
    }
}

impl PropertyInfo {
    /// "Min Extent  (minimumExtent)", or just the name when there's no label.
    pub fn display(&self) -> String {
        if self.label.is_empty() || self.label == self.name { self.name.clone() } else { format!("{}  ({})", self.label, self.name) }
    }
}

impl Target {
    /// "Effect insert: Gain".
    pub fn title(&self) -> String {
        if self.kind_label.is_empty() { self.label.clone() } else { format!("{}: {}", self.kind_label, self.label) }
    }
    pub fn property(&self, name: &str) -> Option<&PropertyInfo> {
        self.properties.iter().find(|p| p.name == name)
    }
    pub fn curve(&self, property: &str, param: &str) -> Option<&CurveInfo> {
        self.property(property)?.curves.iter().find(|c| c.parameter_id == param)
    }
    /// A stable key for "the same target": the object, plus a selected curve.
    pub fn key(&self) -> String {
        match &self.prefer {
            Some(p) => format!("{}|{}|{}", self.id, p.property, p.parameter_id.as_deref().unwrap_or("")),
            None => self.id.clone(),
        }
    }
}

pub fn normalise(points: &[RawPoint], r: &Range) -> Vec<Point> {
    let (dp, dv) = (r.phi - r.plo, r.vhi - r.vlo);
    let mut out: Vec<Point> = points
        .iter()
        .map(|p| Point {
            x: if dp != 0.0 { (p.position - r.plo) / dp } else { 0.0 },
            y: if dv != 0.0 { (p.value - r.vlo) / dv } else { 0.0 },
            shape: p.shape,
            s: p.s,
        })
        .collect();
    crate::library::sort_points(&mut out);
    out
}

/// Suggested ranges for a target property on a parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggested {
    pub range: Range,
    pub value_known: bool,
    pub value_source: String,
    /// The position range is a guess (an empty timeline): ask the user to check it.
    pub position_estimated: bool,
}

pub fn suggest(target: &Target, property: &str, param_id: &str, snap: &Snapshot) -> Suggested {
    let curve = target.curve(property, param_id);
    let param = snap.param(param_id);
    let position_estimated = param.is_some_and(|p| p.timeline && p.estimated) && curve.is_none();
    // Positions: the parameter's range; a timeline runs to the event's end.
    let (plo, phi) = match (param, curve) {
        (Some(p), _) if p.timeline => {
            let end = p.max.max(target.timeline_length.unwrap_or(0.0));
            let end = curve.and_then(|c| c.points.last().map(|q| q.position)).map_or(end, |last| end.max(last));
            (0.0, if end > 0.0 { end } else { 1.0 })
        }
        (Some(p), _) => (p.min, p.max),
        (None, Some(c)) => (c.pmin.unwrap_or(0.0), c.pmax.unwrap_or_else(|| c.points.last().map_or(1.0, |q| q.position.max(1.0)))),
        (None, None) => (0.0, 1.0),
    };
    if let Some(info) = target.property(property)
        && let (Some(lo), Some(hi)) = (info.lo, info.hi)
    {
        let src = if info.unit.is_empty() { "the Effect Reference".to_string() } else { format!("the Effect Reference ({})", info.unit) };
        return Suggested { range: Range { plo, phi, vlo: lo, vhi: hi }, value_known: true, value_source: src, position_estimated };
    }
    if let Some(c) = curve
        && !c.points.is_empty()
    {
        let lo = c.points.iter().map(|p| p.value).fold(f64::INFINITY, f64::min);
        let mut hi = c.points.iter().map(|p| p.value).fold(f64::NEG_INFINITY, f64::max);
        if hi - lo < 1e-9 {
            hi = lo + 1.0;
        }
        return Suggested {
            range: Range { plo, phi, vlo: lo, vhi: hi },
            value_known: false,
            value_source: "the curve's own lowest and highest values (the property's range is unknown)".into(),
            position_estimated,
        };
    }
    Suggested {
        range: Range { plo, phi, vlo: 0.0, vhi: 1.0 },
        value_known: false,
        value_source: "nothing: the property's range is unknown, enter it".into(),
        position_estimated,
    }
}

#[cfg(test)]
mod spatial_tests {
    use super::Spatial;

    #[test]
    fn built_in_attenuation_matches_the_effect_reference() {
        let s = |t| Spatial { rolloff: String::new(), rolloff_type: t, min: 2.0, max: 20.0, source: String::new() };
        for t in 0..4 {
            assert_eq!(s(t).gain(1.0), 1.0, "unattenuated inside min (mode {t})");
        }
        assert_eq!(s(0).gain(20.0), 0.0);
        assert!((s(1).gain(11.0) - 0.5).abs() < 1e-9, "linear: half way, half gain");
        assert!((s(2).gain(4.0) - 0.5).abs() < 1e-9, "inverse: min/d");
        assert!((s(2).gain(40.0) - 0.1).abs() < 1e-9, "inverse stops attenuating at max");
        assert_eq!(s(3).gain(20.0), 0.0, "inverse tapered reaches silence at max");
        assert_eq!(s(4).gain(15.0), 1.0, "off");
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn sample() -> Snapshot {
        serde_json::from_str(
            r#"{
          "selection": [
            { "id": "{lp}", "kind": "insert", "label": "LowpassSimple insert", "where": "track 'Engine' in event 'Car'", "eventId": "{ev}",
              "properties": [ { "name": "cutoff", "automated": false, "lo": 10, "hi": 22000, "unit": "Hz", "curves": [] } ],
              "parameters": [ { "id": "{tl}", "inEvent": true }, { "id": "{dist}", "inEvent": true }, { "id": "{cam}", "inEvent": false } ],
              "timelineLength": 3.5 },
            { "id": "{fx}", "kind": "shared", "label": "Selected curve: gain on Distance · Shared effect 'FMOD Gain'",
              "prefer": { "property": "gain", "parameterId": "{dist}" },
              "properties": [ { "name": "gain", "automated": true, "lo": -80, "hi": 10, "unit": "dB",
                 "curves": [ { "parameterId": "{dist}", "parameterName": "Distance", "pmin": 0, "pmax": 20,
                   "points": [ { "position": 2, "value": 0, "shape": -0.434, "s": false }, { "position": 15, "value": -80 } ] } ] },
                 { "name": "mystery", "automated": true, "curves": [ { "parameterId": "{cam}", "parameterName": "CameraHeight", "pmin": 0, "pmax": 100,
                   "points": [ { "position": 0, "value": 3 }, { "position": 100, "value": 7 } ] } ] } ],
              "parameters": [ { "id": "{dist}" }, { "id": "{cam}" } ] }
          ],
          "sharedEffects": [
            { "id": "{fx}", "kind": "shared", "label": "Shared effect 'FMOD Gain'", "properties": [], "parameters": [] },
            { "id": "{fx2}", "kind": "shared", "label": "Shared effect 'Muffle'", "properties": [], "parameters": [] }
          ],
          "parameters": [
            { "id": "{dist}", "name": "Distance", "min": 0, "max": 20 },
            { "id": "{cam}", "name": "CameraHeight", "min": 0, "max": 100 },
            { "id": "{tl}", "name": "Timeline ('Car')", "min": 0, "max": 3.5, "timeline": true }
          ]
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn targets_and_keys() {
        let s = sample();
        let all = s.all_targets();
        assert_eq!(all.len(), 4, "the selected curve's shared effect is still listed as a plain shared effect");
        assert_ne!(all[1].key(), all[2].key());
        assert_eq!(all[0].place, "track 'Engine' in event 'Car'");
    }

    #[test]
    fn suggestions() {
        let s = sample();
        let lp = &s.selection[0];
        let g = suggest(lp, "cutoff", "{cam}", &s);
        assert_eq!(g.range, Range { plo: 0.0, phi: 100.0, vlo: 10.0, vhi: 22000.0 });
        assert!(g.value_known);
        let tl = suggest(lp, "cutoff", "{tl}", &s);
        assert_eq!((tl.range.plo, tl.range.phi), (0.0, 3.5), "timeline runs to the event's end");
        let fx = &s.selection[1];
        let unknown = suggest(fx, "mystery", "{cam}", &s);
        assert!(!unknown.value_known);
        assert_eq!((unknown.range.vlo, unknown.range.vhi), (3.0, 7.0));
    }

    #[test]
    fn normalise_maps_ranges() {
        let s = sample();
        let c = s.selection[1].curve("gain", "{dist}").unwrap();
        let n = normalise(&c.points, &Range { plo: 0.0, phi: 20.0, vlo: -80.0, vhi: 10.0 });
        assert!((n[0].x - 0.1).abs() < 1e-12 && (n[0].y - 80.0 / 90.0).abs() < 1e-12 && n[0].shape == -0.434);
        assert_eq!(n[1].y, 0.0);
    }

    #[test]
    fn light_replies_keep_the_shared_list() {
        let full = sample();
        let light: Reply = serde_json::from_str(r#"{"selection": [], "sharedEffects": null, "parameters": []}"#).unwrap();
        let merged = Snapshot::merge(Some(&full), light);
        assert_eq!(merged.shared_effects.len(), 2);
        assert!(merged.selection.is_empty());
    }

    #[test]
    fn estimated_timeline_is_flagged() {
        let mut s = sample();
        s.parameters[2].estimated = true;
        let lp = s.selection[0].clone();
        assert!(suggest(&lp, "cutoff", "{tl}", &s).position_estimated);
        assert!(!suggest(&lp, "cutoff", "{cam}", &s).position_estimated);
    }
}
