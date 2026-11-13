//! What the Studio script sends: the followed profiler session, its playhead,
//! and the spatial settings of the events the captures mention.

use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Spatial {
    /// Effective range: a spatializer's override, otherwise the event's own.
    pub min: f32,
    pub max: f32,
    #[serde(rename = "is3D", default)]
    pub is_3d: bool,
    /// LinearSquared, Linear, Inverse, InverseTapered, Custom/Off.
    #[serde(default)]
    pub rolloff: i32,
    #[serde(default)]
    pub size: f32,
    #[serde(rename = "minExtent", default)]
    pub min_extent: f32,
    /// 0 Auto, 1 User, 2 Off.
    #[serde(rename = "extentMode", default)]
    pub extent_mode: i32,
    #[serde(default)]
    pub spatializer: String,
    /// The track it sits on ("Master" or a group track's name).
    #[serde(default)]
    pub track: String,
    /// The effect preset's name, when it's a preset (shared) effect.
    #[serde(default)]
    pub preset: String,
    /// True when the spatializer's own range overrides the event's.
    #[serde(rename = "override", default)]
    pub override_range: bool,
    /// Further spatializers in the event (e.g. a smaller one on one track).
    #[serde(default)]
    pub others: Vec<Spatial>,
    /// Properties automated on built-in Distance / Distance (Normalized),
    /// e.g. a snapshot instrument's intensity (top-level only).
    #[serde(rename = "distanceDriven", default)]
    pub distance_driven: Vec<DistanceUse>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DistanceUse {
    pub what: String,
    #[serde(default)]
    pub normalized: bool,
    /// The Distance parameter's maximum (unused when normalized).
    #[serde(default)]
    pub max: f32,
}

impl Spatial {
    pub fn rolloff_name(&self) -> &'static str {
        match self.rolloff {
            0 => "Linear Squared",
            1 => "Linear",
            2 => "Inverse",
            3 => "Inverse Tapered",
            _ => "Off (no curve)",
        }
    }

    /// Radius of the sound-size sphere, when it's set by hand. Sound Size is
    /// treated as a diameter (unverified against FMOD's docs).
    /// True when distance changes the level: the curve isn't Off. Studio
    /// stores the Off button as distanceRolloffType 4.
    pub fn has_curve(&self) -> bool {
        self.is_3d && self.rolloff != 4
    }

    /// The (min, max) range over which the event reacts to distance, for the
    /// height fit: spatializers with a curve, and Distance automation (which
    /// counts from 0). None: it doesn't react to distance.
    pub fn distance_range(&self) -> Option<(f32, f32)> {
        let curves = std::iter::once(self).chain(self.others.iter()).filter(|s| s.has_curve()).map(|s| (s.min, s.max));
        // Normalized distance runs over the event's min..max.
        let autos = self.distance_driven.iter().map(|d| if d.normalized { (self.min, self.max) } else { (0.0, d.max) });
        curves.chain(autos).filter(|r| r.1 > 0.0).reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    }

    pub fn size_radius(&self) -> Option<f32> {
        (self.extent_mode == 1 && self.size > 0.0).then_some(self.size * 0.5)
    }
}

/// An event from the project, looked up by the GUID (or path) in a capture.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EventInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    /// "Event", or "Snapshot".
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub spatial: Option<Spatial>,
    /// Not in the project (another project's banks, or deleted).
    #[serde(default)]
    pub missing: bool,
    /// The Event Browser colour: "Default", "Red", "Cyan Light 2", ...
    #[serde(default)]
    pub color: String,
    /// Events its instruments play (nested/referenced), recursively.
    #[serde(default)]
    pub children: Vec<ChildEvent>,
}

/// An event played by another event's instrument. It runs at the parent
/// instance's position (a scatterer's children around it). The capture
/// doesn't say when it plays.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ChildEvent {
    pub id: String,
    /// The event whose instrument plays it (the top event or another child).
    #[serde(default)]
    pub parent: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub color: String,
    /// In a scatterer: spawned around the parent, not on it.
    #[serde(default)]
    pub scattered: bool,
    #[serde(default)]
    pub depth: u32,
    #[serde(default)]
    pub spatial: Option<Spatial>,
}

impl EventInfo {
    pub fn is_snapshot(&self) -> bool {
        self.kind == "Snapshot"
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Session {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// The session's folder (absolute), holding apicommands-NNNN.txt.
    #[serde(default)]
    pub dir: String,
    /// The same folder under `<project>/.unsaved/`, where Studio keeps a
    /// session recorded since the last project save.
    #[serde(default)]
    pub unsaved: String,
    /// ProfilerRecordingMarker positions (seconds), sorted: where each
    /// recording starts on the session timeline.
    #[serde(default)]
    pub markers: Vec<f64>,
}

/// One line from the script.
#[derive(Debug, PartialEq)]
pub enum Msg {
    Hello { studio: String, script: String, project: String },
    Session(Option<Session>),
    /// The session timeline's cursor and playhead (seconds; the playhead may
    /// be absent when the script can't read one).
    Time { cursor: f64, playhead: Option<f64> },
    Events(Vec<EventInfo>),
    /// Events of the profiler tracks selected in the Profiler window.
    Selection(Vec<String>),
}

pub fn parse(line: &str) -> Option<Msg> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let f = |k: &str| v.get(k).and_then(|x| x.as_f64()).filter(|x| x.is_finite());
    match v.get("type")?.as_str()? {
        "hello" => Some(Msg::Hello { studio: s("studio"), script: s("script"), project: s("project") }),
        "session" if v.get("none").and_then(|x| x.as_bool()) == Some(true) => Some(Msg::Session(None)),
        "session" => serde_json::from_value::<Session>(v).ok().map(|mut se| {
            se.markers.sort_by(f64::total_cmp);
            Msg::Session(Some(se))
        }),
        "time" => Some(Msg::Time { cursor: f("cursor")?, playhead: f("playhead") }),
        "events" => serde_json::from_value::<Vec<EventInfo>>(v.get("items")?.clone()).ok().map(Msg::Events),
        "selection" => serde_json::from_value::<Vec<String>>(v.get("ids")?.clone()).ok().map(Msg::Selection),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_message_kind() {
        assert_eq!(
            parse(r#"{"type":"hello","studio":"2.03.14","script":"0.1.0","project":"/p.fspro"}"#),
            Some(Msg::Hello { studio: "2.03.14".into(), script: "0.1.0".into(), project: "/p.fspro".into() })
        );
        assert_eq!(parse(r#"{"type":"session","none":true}"#), Some(Msg::Session(None)));
        let Some(Msg::Session(Some(s))) = parse(r#"{"type":"session","id":"{a}","name":"S","dir":"/x/","markers":[84.3,0]}"#) else { panic!() };
        assert_eq!(s.markers, vec![0.0, 84.3], "sorted");
        assert_eq!(parse(r#"{"type":"time","cursor":1.5,"playhead":2}"#), Some(Msg::Time { cursor: 1.5, playhead: Some(2.0) }));
        assert_eq!(parse(r#"{"type":"time","cursor":1.5}"#), Some(Msg::Time { cursor: 1.5, playhead: None }));
        let Some(Msg::Events(ev)) = parse(r#"{"type":"events","items":[{"id":"{e}","name":"ev","path":"event:/ev","kind":"Event",
            "spatial":{"min":1,"max":35,"is3D":true,"rolloff":0,"size":4,"minExtent":0,"extentMode":1}},{"id":"{m}","missing":true}]}"#) else { panic!() };
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].spatial.as_ref().unwrap().size_radius(), Some(2.0));
        assert!(ev[1].missing);
        let Some(Msg::Events(ev)) = parse(r#"{"type":"events","items":[{"id":"{e}","name":"ev","color":"Cyan Light 2",
            "children":[{"id":"{c}","parent":"{e}","name":"tail","color":"Red","scattered":true,"depth":1,"spatial":{"min":1,"max":5,"is3D":true}}]}]}"#) else { panic!() };
        assert_eq!(ev[0].color, "Cyan Light 2");
        assert_eq!((ev[0].children[0].name.as_str(), ev[0].children[0].scattered), ("tail", true));
        assert_eq!(parse(r#"{"type":"selection","ids":["{a}"]}"#), Some(Msg::Selection(vec!["{a}".into()])));
        assert_eq!(parse("not json"), None);
    }
}
