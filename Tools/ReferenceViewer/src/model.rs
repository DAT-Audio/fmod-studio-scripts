//! The messages from the Studio script (`Scripts/DAT-audio Utilities/Open Reference Viewer.js`),
//! one JSON object per line:
//! - `hello {studio, script, project}`
//! - `focus {kind: none|event|parameter|snapshot|error, params, ...}`
//! - `cursors {values: {paramId: value}}`
//!
//! The script sends `null` for anything a Studio build doesn't have, so nulls
//! are stripped before parsing and every field has a default.

use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct EventRef {
    pub id: String,
    pub name: String,
    pub path: String,
    pub snapshot: bool,
    /// Studio's colour name ("Default", "Red", "Blue Light 1"…).
    pub color: String,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct ParamRef {
    pub id: String,
    pub name: String,
    pub timeline: bool,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct ParamInfo {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub initial: Option<f64>,
    pub cursor: Option<f64>,
    pub labels: Vec<String>,
    pub global: bool,
    pub read_only: bool,
    pub hold: bool,
    pub velocity: Option<f64>,
}

/// One automation point: position, value, bend (`curveShape`), S-curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub shape: f64,
    pub s: bool,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Curve {
    pub param: Option<ParamRef>,
    points: Vec<Vec<Value>>,
}

impl Curve {
    /// The points, sorted by position (the script sorts them too).
    pub fn points(&self) -> Vec<Point> {
        let mut out: Vec<Point> = self
            .points
            .iter()
            .filter_map(|p| {
                let num = |i: usize| p.get(i).and_then(Value::as_f64);
                Some(Point { x: num(0)?, y: num(1)?, shape: num(2).unwrap_or(0.0), s: p.get(3).and_then(Value::as_bool).unwrap_or(false) })
            })
            .collect();
        out.sort_by(|a, b| a.x.total_cmp(&b.x));
        out
    }
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Automation {
    pub property: String,
    pub curves: Vec<Curve>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Modulation {
    pub id: String,
    #[serde(rename = "type")]
    pub entity: String,
    pub kind: String,
    pub property: String,
    pub settings: Map<String, Value>,
    pub automation: Vec<Automation>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Condition {
    pub kind: String,
    pub param: Option<ParamRef>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub inverted: bool,
    pub labels: Vec<String>,
    pub event_state: Option<f64>,
    pub automation: Vec<Automation>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Delay {
    #[serde(rename = "type")]
    pub kind: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct PassedParam {
    pub param: Option<ParamRef>,
    pub value: Option<f64>,
    pub automation: Vec<Automation>,
    pub modulation: Vec<Modulation>,
}

/// An instrument, or a Multi/Scatterer around one (`container` set).
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Instrument {
    pub id: String,
    #[serde(rename = "type")]
    pub entity: String,
    pub kind: String,
    pub name: String,
    pub volume: Option<f64>,
    pub pitch: Option<f64>,
    pub probability_on: bool,
    pub probability: Option<f64>,
    pub condition_mode: Option<f64>,
    pub conditions: Vec<Condition>,
    pub delay: Option<Delay>,
    pub start: Option<f64>,
    pub length: Option<f64>,
    pub start_offset: Option<f64>,
    #[serde(rename = "async")]
    pub is_async: bool,
    pub cutoff: bool,
    pub looping: bool,
    pub quantization: Option<f64>,
    pub max_voices: Option<f64>,
    pub automation: Vec<Automation>,
    pub modulation: Vec<Modulation>,
    pub params: Vec<PassedParam>,
    pub intensity: Option<f64>,
    pub container: Option<String>,
    pub entries: Option<f64>,
    pub playlist_mode: Option<f64>,
    pub playback_mode: Option<f64>,
    pub entry_percentage: Option<f64>,
    pub settings: Map<String, Value>,
    /// Snapshot instruments: the snapshot they start.
    pub snapshot: Option<EventRef>,
    pub track: Option<Track>,
    /// Snapshot triggers: the event the instrument is in.
    pub event: Option<EventRef>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Spatial {
    #[serde(rename = "override")]
    pub overrides: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub rolloff: Option<f64>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct SnapshotOn {
    pub snapshot: Option<EventRef>,
    pub property: String,
    pub value: Option<f64>,
    pub intensity: Option<f64>,
    pub automation: Vec<Automation>,
    pub scoped: bool,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Effect {
    pub id: String,
    pub bypass: bool,
    pub preset: Option<String>,
    #[serde(rename = "type")]
    pub entity: String,
    pub kind: String,
    pub name: String,
    pub fader: bool,
    pub gain: Option<f64>,
    pub level: Option<f64>,
    pub to: Option<String>,
    pub spatial: Option<Spatial>,
    pub settings: Map<String, Value>,
    pub automation: Vec<Automation>,
    pub modulation: Vec<Modulation>,
    pub snapshots: Vec<SnapshotOn>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Vca {
    pub id: String,
    pub name: String,
    pub volume: Option<f64>,
    pub automation: Vec<Automation>,
    pub snapshots: Vec<SnapshotOn>,
}

/// A mixer strip: an event track, the event's master track, its input in the
/// mixer, a bus, the Master Bus.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Strip {
    pub id: String,
    #[serde(rename = "type")]
    pub entity: String,
    pub kind: String,
    pub name: String,
    pub color: String,
    pub volume: Option<f64>,
    pub pitch: Option<f64>,
    pub automation: Vec<Automation>,
    pub modulation: Vec<Modulation>,
    pub effects: Vec<Effect>,
    pub vcas: Vec<Vca>,
    pub snapshots: Vec<SnapshotOn>,
}

impl Strip {
    pub fn label(&self) -> String {
        if self.name.is_empty() { self.kind.clone() } else { format!("{} '{}'", self.kind, self.name) }
    }
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    #[serde(rename = "type")]
    pub entity: String,
    pub name: String,
    pub master: bool,
    pub strip: Option<Strip>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Sheet {
    pub kind: String,
    pub param: Option<ParamRef>,
    pub start: Option<f64>,
    pub length: Option<f64>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Macros {
    pub settings: Map<String, Value>,
    pub automation: Vec<Automation>,
    pub modulation: Vec<Modulation>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct AssetInfo {
    pub length: Option<f64>,
    pub channels: Option<f64>,
    pub rate: Option<f64>,
    pub streaming: bool,
    pub uses: Option<f64>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct PresetEffect {
    #[serde(rename = "type")]
    pub entity: String,
    pub kind: String,
    pub settings: Map<String, Value>,
}

/// An event, asset, mixer strip (bus, return, VCA), event track, or shared effect.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    /// event | asset | bus | return | vca | track | preset
    pub kind: String,
    /// Studio's name for the object's type ("Bus", "Return bus", "Master Bus"…).
    pub label: String,
    pub name: String,
    pub path: String,
    pub snapshot: bool,
    /// Studio's colour name ("Default", "Red", "Blue Light 1"…).
    pub color: String,
    pub is_focus: bool,
    /// Negative: above the focus (events that use it); positive: below.
    pub depth: i32,
    /// How many event instruments use this event, anywhere in the project.
    pub used_by: u32,
    pub master: Option<Strip>,
    pub macros: Option<Macros>,
    pub snapshots: Vec<Instrument>,
    /// The focus and top-level events: input strip, buses, Master Bus.
    pub route: Option<Vec<Strip>>,
    pub asset: Option<AssetInfo>,
    /// Buses, returns, VCAs and event tracks.
    pub strip: Option<Strip>,
    /// Event tracks: the event they're in.
    pub event: Option<EventRef>,
    pub slaves: Option<f64>,
    pub effect: Option<PresetEffect>,
    pub uses: Option<f64>,
    /// Banks the event is assigned to directly (not through a parent).
    pub banks: Vec<String>,
    /// Added by the viewer past the top-level events: their mixer route and
    /// the snapshots they start ("further chain"). Not sent by the script.
    #[serde(skip)]
    pub further: bool,
}

impl Node {
    pub fn is_event(&self) -> bool {
        self.kind.is_empty() || self.kind == "event"
    }
    /// What to open in Studio for this node (a track opens its event).
    pub fn open_id(&self) -> (String, String) {
        match &self.event {
            Some(e) if self.kind == "track" => (e.id.clone(), e.name.clone()),
            _ => (self.id.clone(), self.name.clone()),
        }
    }
}

/// A link in signal-flow terms: `child` feeds `parent`. `kind`:
/// - event: an event instrument in `parent` plays `child`;
/// - asset: an audio instrument in `parent` plays the asset `child`;
/// - route: bus (or event input) `child` outputs into `parent`;
/// - send: a send on `child` feeds the return `parent`;
/// - effect: the shared effect `child` sits on `parent`;
/// - vca: the VCA `parent` controls `child`.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Edge {
    pub id: String,
    pub kind: String,
    pub parent_id: String,
    pub child_id: String,
    pub send: Option<Effect>,
    pub effect: Option<Effect>,
    pub place: Option<String>,
    #[serde(skip)]
    pub further: bool,
    pub instrument: Instrument,
    /// Multi/Scatterer around the instrument, innermost first.
    pub containers: Vec<Instrument>,
    pub track: Option<Track>,
    pub sheet: Option<Sheet>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Graph {
    /// event | asset | preset | bus | return | send | vca
    pub mode: String,
    pub focus_id: String,
    pub focus_ids: Vec<String>,
    /// Send mode: the selected send.
    pub highlight: Option<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub truncated: bool,
    pub max_depth: Option<f64>,
    pub max_nodes: Option<f64>,
}

impl Graph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn focus(&self) -> Option<&Node> {
        self.node(&self.focus_id)
    }
    pub fn foci(&self) -> Vec<&Node> {
        let ids: Vec<&String> = if self.focus_ids.is_empty() { vec![&self.focus_id] } else { self.focus_ids.iter().collect() };
        ids.into_iter().filter_map(|id| self.node(id)).collect()
    }
    pub fn mode(&self) -> &str {
        if self.mode.is_empty() { "event" } else { &self.mode }
    }
    /// Links into `id` from the events that use it.
    pub fn users_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Edge> + 'a {
        self.edges.iter().filter(move |e| e.child_id == id)
    }
    /// Links from `id` to the events it uses.
    pub fn used_by<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Edge> + 'a {
        self.edges.iter().filter(move |e| e.parent_id == id)
    }
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Use {
    pub kind: String,
    pub event: Option<EventRef>,
    pub child: Option<EventRef>,
    pub target: String,
    pub target_id: String,
    pub property: String,
    points: Vec<Vec<Value>>,
    pub modulates: Option<String>,
    pub snapshot_property: Option<String>,
    pub mixer: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub inverted: bool,
    pub value: Option<f64>,
    pub automation: Vec<Automation>,
}

impl Use {
    pub fn curve(&self) -> Curve {
        Curve { param: None, points: self.points.clone() }
    }
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct SnapshotProperty {
    pub id: String,
    pub property: String,
    pub value: Option<f64>,
    pub target: String,
    pub target_id: String,
    pub base: Option<f64>,
    pub automation: Vec<Automation>,
    pub modulation: Vec<Modulation>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Scoped {
    pub id: String,
    pub strip: String,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub behavior: Option<f64>,
    pub intensity: Option<f64>,
    pub macros: Option<Macros>,
    pub properties: Vec<SnapshotProperty>,
    pub scoped: Vec<Scoped>,
    pub triggers: Vec<Instrument>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Selection {
    pub count: u32,
    /// Other kinds that were selected too, and how many: not shown.
    pub others: HashMap<String, f64>,
    pub more: u32,
    pub source: String,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Focus {
    pub selection: Option<Selection>,
    pub kind: String,
    pub graph: Option<Graph>,
    pub param: Option<ParamInfo>,
    pub uses: Vec<Use>,
    pub sandbox: u32,
    pub snapshot: Option<SnapshotInfo>,
    pub params: HashMap<String, ParamInfo>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum Msg {
    Hello { studio: String, script: String },
    Focus(Box<Focus>),
    Cursors(HashMap<String, f64>),
    Other,
}

/// Removes nulls everywhere, so serde's defaults apply instead.
fn strip_nulls(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            m.values_mut().for_each(strip_nulls);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

pub fn parse(line: &str) -> Result<Msg, String> {
    let mut v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    // Points are arrays whose nulls carry meaning; keep them as they are.
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("").to_string();
    match ty.as_str() {
        "hello" => Ok(Msg::Hello {
            studio: v.get("studio").and_then(Value::as_str).unwrap_or("").into(),
            script: v.get("script").and_then(Value::as_str).unwrap_or("").into(),
        }),
        "focus" => {
            strip_nulls(&mut v);
            serde_json::from_value::<Focus>(v).map(|f| Msg::Focus(Box::new(f))).map_err(|e| e.to_string())
        }
        "cursors" => {
            let mut out = HashMap::new();
            if let Some(m) = v.get("values").and_then(Value::as_object) {
                for (k, x) in m {
                    if let Some(f) = x.as_f64() {
                        out.insert(k.clone(), f);
                    }
                }
            }
            Ok(Msg::Cursors(out))
        }
        _ => Ok(Msg::Other),
    }
}

/// Sample focus messages in the format Studio sends, used by --demo and the tests.
pub mod fixtures {
    pub const EVENT_LEAF: &str = include_str!("../../../Tests/fixtures/focus_event_leaf.json");
    pub const EVENT_TOP: &str = include_str!("../../../Tests/fixtures/focus_event_top.json");
    pub const PARAMETER: &str = include_str!("../../../Tests/fixtures/focus_parameter.json");
    pub const SNAPSHOT: &str = include_str!("../../../Tests/fixtures/focus_snapshot.json");
    pub const EVENT_MULTI: &str = include_str!("../../../Tests/fixtures/focus_event_multi.json");
    pub const ASSET: &str = include_str!("../../../Tests/fixtures/focus_asset.json");
    pub const PRESET: &str = include_str!("../../../Tests/fixtures/focus_preset.json");
    pub const RETURN: &str = include_str!("../../../Tests/fixtures/focus_return.json");
    pub const BUS: &str = include_str!("../../../Tests/fixtures/focus_bus.json");

    pub fn focus(json: &str) -> super::Focus {
        // Fixtures are pretty-printed; the wire format is one line.
        let line = serde_json::to_string(&serde_json::from_str::<serde_json::Value>(json).unwrap()).unwrap();
        match super::parse(&line).unwrap() {
            super::Msg::Focus(f) => *f,
            m => panic!("not a focus: {m:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;

    #[test]
    fn parses_the_event_fixture() {
        let f = focus(EVENT_LEAF);
        assert_eq!(f.kind, "event");
        let g = f.graph.unwrap();
        assert_eq!(g.nodes.len(), 5);
        assert_eq!(g.edges.len(), 4);
        let a = g.focus().unwrap();
        assert_eq!(a.name, "A_leaf");
        assert_eq!(a.route.as_ref().unwrap().len(), 4);
        assert_eq!(g.users_of(&g.focus_id).count(), 3);
        let b = g.nodes.iter().find(|n| n.name == "B_mid").unwrap();
        let e = g.users_of(&g.focus_id).find(|e| e.parent_id == b.id).unwrap();
        assert_eq!(e.instrument.conditions[0].param.as_ref().unwrap().name, "Intensity");
        let pts = e.track.as_ref().unwrap().strip.as_ref().unwrap().automation[0].curves[0].points();
        assert_eq!(pts.len(), 2);
        assert_eq!((pts[0].x, pts[0].y, pts[0].shape), (0.0, -20.0, 0.5));
        assert!(f.params.values().any(|p| p.name == "Intensity" && p.max == Some(10.0)));
    }

    #[test]
    fn parses_parameter_and_snapshot_fixtures() {
        let p = focus(PARAMETER);
        assert_eq!(p.kind, "parameter");
        assert_eq!(p.param.unwrap().name, "Intensity");
        assert!(p.uses.iter().any(|u| u.kind == "passed" && u.value == Some(4.5)));
        let s = focus(SNAPSHOT);
        let sn = s.snapshot.unwrap();
        assert_eq!(sn.properties[0].value, Some(-9.0));
        assert_eq!(sn.triggers[0].intensity, Some(60.0));
    }

    #[test]
    fn parses_the_new_kinds() {
        let a = focus(ASSET).graph.unwrap();
        assert_eq!(a.mode(), "asset");
        assert_eq!(a.focus().unwrap().kind, "asset");
        assert_eq!(a.edges.iter().filter(|e| e.kind == "asset").count(), 2);
        let r = focus(RETURN).graph.unwrap();
        assert!(r.edges.iter().any(|e| e.kind == "send" && e.send.as_ref().and_then(|s| s.level) == Some(-10.0)));
        let m = focus(EVENT_MULTI).graph.unwrap();
        assert_eq!(m.foci().len(), 2);
        let p = focus(PRESET).graph.unwrap();
        assert!(p.edges.iter().any(|e| e.kind == "effect" && e.place.as_deref().unwrap_or("").contains("B track")));
        assert_eq!(focus(BUS).graph.unwrap().focus().unwrap().kind, "bus");
    }

    #[test]
    fn nulls_and_unknown_fields_are_fine() {
        let m = super::parse(r#"{"type":"focus","kind":"event","graph":{"focusId":"a","nodes":[{"id":"a","name":null,"route":null,"extra":1}],"edges":[]},"params":{}}"#).unwrap();
        let super::Msg::Focus(f) = m else { panic!() };
        assert_eq!(f.graph.unwrap().nodes[0].name, "");
        let super::Msg::Cursors(c) = super::parse(r#"{"type":"cursors","values":{"x":6,"y":null}}"#).unwrap() else { panic!() };
        assert_eq!(c.get("x"), Some(&6.0));
        assert!(!c.contains_key("y"));
    }
}
