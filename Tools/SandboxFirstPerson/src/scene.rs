//! What the Studio script sends: the followed Sandbox scene and its listener.

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

impl Emitter {
    pub fn is_snapshot(&self) -> bool {
        self.kind == "Snapshot"
    }
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

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Emitter {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub path: String,
    /// The event's entity: "Event", or "Snapshot" for a snapshot in the Sandbox.
    #[serde(default)]
    pub kind: String,
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub rot: f32,
    #[serde(rename = "loop", default)]
    pub loop_: bool,
    #[serde(default)]
    pub spatial: Option<Spatial>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Scene {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub listeners: u32,
    #[serde(default)]
    pub emitters: Vec<Emitter>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub x: f32,
    pub z: f32,
    pub rot: f32,
}

/// One line from the script.
#[derive(Debug, PartialEq)]
pub enum Msg {
    Hello { studio: String, script: String, project: String },
    Scene(Option<Scene>),
    Listener(Listener),
}

pub fn parse(line: &str) -> Option<Msg> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let f = |k: &str| v.get(k).and_then(|x| x.as_f64()).map(|x| x as f32);
    match v.get("type")?.as_str()? {
        "hello" => Some(Msg::Hello { studio: s("studio"), script: s("script"), project: s("project") }),
        "scene" if v.get("none").and_then(|x| x.as_bool()) == Some(true) => Some(Msg::Scene(None)),
        "scene" => serde_json::from_value::<Scene>(v).ok().map(|sc| Msg::Scene(Some(sc))),
        "listener" => Some(Msg::Listener(Listener { x: f("x")?, z: f("z")?, rot: f("rot")? })),
        _ => None,
    }
}

/// A small made-up scene for `--demo` and screenshots.
pub fn demo() -> Scene {
    let sp = |min: f32, max: f32, size: f32, mode: i32| Some(Spatial {
        min,
        max,
        is_3d: true,
        rolloff: 2,
        size,
        min_extent: 0.0,
        extent_mode: mode,
        spatializer: "spatializer".into(),
        track: "Master".into(),
        ..Default::default()
    });
    let e = |id: &str, name: &str, x: f32, z: f32, s: Option<Spatial>| Emitter {
        id: id.into(),
        name: name.into(),
        path: format!("event:/Demo/{name}"),
        kind: "Event".into(),
        x,
        z,
        rot: 0.0,
        loop_: false,
        spatial: s,
    };
    Scene {
        id: "demo".into(),
        name: "Demo Scene".into(),
        listeners: 1,
        emitters: vec![
            e("a", "Waterfall", -6.0, 14.0, sp(2.0, 18.0, 6.0, 1)),
            e("b", "Market Crowd", 9.0, 8.0, sp(1.0, 12.0, 0.0, 0).map(|mut s| {
                s.preset = "s_map_emitter_medium".into();
                s.others = vec![Spatial { min: 0.5, max: 4.0, is_3d: true, track: "Jingle".into(), override_range: true, ..Default::default() }];
                s
            })),
            e("c", "Bird Call", 2.0, 30.0, sp(1.0, 25.0, 0.0, 0)),
            e("d", "UI Click (2D)", -3.0, 4.0, Some(Spatial { min: 1.0, max: 20.0, ..Default::default() })),
        ],
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
        assert_eq!(parse(r#"{"type":"scene","none":true}"#), Some(Msg::Scene(None)));
        assert_eq!(parse(r#"{"type":"listener","x":1.5,"z":-2,"rot":45}"#), Some(Msg::Listener(Listener { x: 1.5, z: -2.0, rot: 45.0 })));
        let line = r#"{"type":"scene","id":"{a}","name":"S","listeners":1,"emitters":[
            {"id":"{e}","name":"ev","path":"event:/ev","x":10,"z":4,"rot":0,"loop":false,
             "spatial":{"min":1,"max":35,"is3D":true,"rolloff":0,"size":4,"minExtent":0,"extentMode":1,"spatializer":"spatializer",
                        "track":"Master","preset":"p","override":false,"others":[{"min":1,"max":8,"is3D":true,"track":"Jingle","override":true}]}},
            {"id":"{f}","name":"(no event)","path":"","x":0,"z":0,"rot":0,"loop":false,"spatial":null}]}"#;
        let Some(Msg::Scene(Some(sc))) = parse(line) else { panic!("scene didn't parse") };
        assert_eq!(sc.emitters.len(), 2);
        let s = sc.emitters[0].spatial.as_ref().unwrap();
        assert!(s.is_3d && s.max == 35.0 && s.size_radius() == Some(2.0));
        assert_eq!((s.preset.as_str(), s.others.len(), s.others[0].max, s.others[0].track.as_str()), ("p", 1, 8.0, "Jingle"));
        assert!(sc.emitters[1].spatial.is_none());
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"{"type":"other"}"#), None);
    }
}
