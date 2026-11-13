//! Reads the profiler's API command captures: the 3D story of a session.
//!
//! Studio saves every recording of a profiler session as plain text next to
//! the session: `Metadata/ProfilerSession/{id}/apicommands-NNNN.txt`, one file
//! per recording. Each
//! line is one runtime API call the game made, e.g.
//!
//! ```text
//! Playback::frame, 1024
//! System::setListenerAttributes, 0, {{x,y,z},{vel},{fwd},{up}}, {ax,ay,az}, true
//! EventDescription::createInstance, 2097664, 2242048
//! EventInstance::set3DAttributes, 2242048, {{x,y,z},{vel},{fwd},{up}}
//! ```
//!
//! - `Playback::frame, N` advances time by N output samples. Summed over a
//!   file they equal that recording's capture-NNNN.wav length (checked: 84.27 s
//!   and 46.2 s at 48 kHz), and recording N starts on the session timeline at
//!   the Nth `ProfilerRecordingMarker`'s position.
//! - `setListenerAttributes`' last two arguments are the **attenuation
//!   position** and whether it's used: a game can attenuate from a point other
//!   than the listener (the "listener probe"). Older captures may lack them.
//! - Handles are reused after release, so every `createInstance` starts a new
//!   instance record.
//!
//! Parsing is incremental (`Parser::feed`) so a recording that is still being
//! written can be followed by reading only what was appended.

use crate::camera::V3;

/// FMOD_STUDIO_EVENT_PROPERTY_MINIMUM_DISTANCE / _MAXIMUM_DISTANCE.
const PROP_MIN_DISTANCE: i64 = 3;
const PROP_MAX_DISTANCE: i64 = 4;
/// FMOD_INIT_3D_RIGHTHANDED in `System::initialize`'s `flags`.
const INIT_3D_RIGHTHANDED: u64 = 0x4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub pos: V3,
    pub fwd: V3,
    pub up: V3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListenerKey {
    pub t: f64,
    pub pose: Pose,
    /// The attenuation position, when the game sets one (the probe).
    pub atten: Option<V3>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Instance {
    pub handle: u64,
    /// The event's GUID (or path), when the capture says which event it is.
    pub event: String,
    pub created: f64,
    pub starts: Vec<f64>,
    pub stops: Vec<f64>,
    pub released: Option<f64>,
    pub poses: Vec<(f64, Pose)>,
    /// Per-instance min/max distance overrides (setProperty), over time.
    pub min_override: Vec<(f64, f32)>,
    pub max_override: Vec<(f64, f32)>,
}

/// One recording (one apicommands file), in its own time (seconds from its start).
#[derive(Clone, Debug, Default)]
pub struct Recording {
    pub version: String,
    pub sample_rate: f64,
    pub right_handed: bool,
    pub len: f64,
    /// Indexed by listener index.
    pub listeners: Vec<Vec<ListenerKey>>,
    pub num_listeners: usize,
    pub instances: Vec<Instance>,
}

pub struct Parser {
    pub rec: Recording,
    samples: u64,
    /// EventDescription handle -> GUID or path.
    descs: std::collections::HashMap<u64, String>,
    /// Live EventInstance handle -> index in rec.instances.
    live: std::collections::HashMap<u64, usize>,
    /// An unfinished last line from the previous feed.
    partial: String,
}

impl Default for Parser {
    fn default() -> Self {
        Parser {
            rec: Recording { sample_rate: 48000.0, num_listeners: 1, ..Default::default() },
            samples: 0,
            descs: Default::default(),
            live: Default::default(),
            partial: String::new(),
        }
    }
}

/// All numbers in a string, braces and spaces ignored ("{{1,2,3},{..}}" -> [1,2,3,..]).
fn numbers(s: &str) -> Vec<f64> {
    s.split(|c: char| c == ',' || c == '{' || c == '}' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f64>().ok())
        .collect()
}

fn v3(n: &[f64]) -> V3 {
    [n[0] as f32, n[1] as f32, n[2] as f32]
}

fn pose(n: &[f64]) -> Option<Pose> {
    // {pos},{vel},{fwd},{up}
    (n.len() >= 12).then(|| Pose { pos: v3(&n[0..3]), fwd: v3(&n[6..9]), up: v3(&n[9..12]) })
}

/// "key: value" from the capture header comment.
fn header_value(line: &str, key: &str) -> Option<String> {
    // A whole key only: "flags" must not match inside "studioflags".
    let pat = format!("{key}: ");
    let i = line.match_indices(&pat).find(|(i, _)| *i == 0 || !line.as_bytes()[i - 1].is_ascii_alphanumeric())?.0 + pat.len();
    let rest = &line[i..];
    let end = rest.find([',', ')']).unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

impl Parser {
    pub fn time(&self) -> f64 {
        self.samples as f64 / self.rec.sample_rate
    }

    /// Feeds more text. Only complete lines are used; the rest waits for the next feed.
    pub fn feed(&mut self, text: &str) {
        let mut buf = std::mem::take(&mut self.partial);
        buf.push_str(text);
        let complete = match buf.rfind('\n') {
            Some(i) => i + 1,
            None => {
                self.partial = buf;
                return;
            }
        };
        for line in buf[..complete].lines() {
            self.line(line.trim_end_matches('\r'));
        }
        self.partial = buf[complete..].to_string();
        self.rec.len = self.time();
    }

    fn line(&mut self, line: &str) {
        if let Some(c) = line.strip_prefix('#') {
            if c.contains("command capture") {
                self.rec.version = c.trim().trim_end_matches("command capture").trim().to_string();
            }
            if c.contains("setSoftwareFormat(") {
                if let Some(r) = header_value(c, "samplerate").and_then(|v| v.parse::<f64>().ok()).filter(|r| *r > 0.0) {
                    self.rec.sample_rate = r;
                }
            }
            if c.contains("Studio::System::initialize(") {
                if let Some(f) = header_value(c, "flags").and_then(|v| u64::from_str_radix(v.trim_start_matches("0x"), 16).ok()) {
                    self.rec.right_handed = f & INIT_3D_RIGHTHANDED != 0;
                }
            }
            return;
        }
        let (cmd, args) = match line.find(", ") {
            Some(i) => (&line[..i], &line[i + 2..]),
            None => (line, ""),
        };
        let t = self.time();
        match cmd {
            "Playback::frame" => {
                if let Ok(n) = args.trim().parse::<u64>() {
                    self.samples += n;
                }
            }
            "System::getEvent" | "System::getEventByID" | "Bank::getEventListItem" => {
                // getEvent(ByID), <guid or path>, <handle> / getEventListItem, <bank>, <index>, <guid>, <handle>
                let parts: Vec<&str> = args.split(", ").collect();
                if let (Some(id), Some(h)) = (parts.get(parts.len().wrapping_sub(2)), parts.last().and_then(|h| h.trim().parse::<u64>().ok())) {
                    self.descs.insert(h, id.trim().to_string());
                }
            }
            "EventDescription::createInstance" => {
                let n = numbers(args);
                if n.len() >= 2 {
                    let (d, h) = (n[0] as u64, n[1] as u64);
                    let event = self.descs.get(&d).cloned().unwrap_or_default();
                    self.rec.instances.push(Instance { handle: h, event, created: t, ..Default::default() });
                    self.live.insert(h, self.rec.instances.len() - 1);
                }
            }
            "EventInstance::getDescription" | "EventDescription::getInstanceListItem" => {
                // getDescription, <instance>, <desc> / getInstanceListItem, <desc>, <index>, <instance>.
                // Names the event of an instance the capture didn't see created.
                let n = numbers(args);
                if n.len() < 2 {
                    return;
                }
                let (h, d) = if cmd == "EventInstance::getDescription" { (n[0] as u64, n[1] as u64) } else { (n[n.len() - 1] as u64, n[0] as u64) };
                let event = self.descs.get(&d).cloned().unwrap_or_default();
                match self.live.get(&h) {
                    Some(&i) if self.rec.instances[i].event.is_empty() => self.rec.instances[i].event = event,
                    Some(_) => {}
                    None => {
                        self.rec.instances.push(Instance { handle: h, event, created: t, ..Default::default() });
                        self.live.insert(h, self.rec.instances.len() - 1);
                    }
                }
            }
            "EventInstance::set3DAttributes" | "EventInstance::start" | "EventInstance::stop" | "EventInstance::release" | "EventInstance::setProperty" => {
                let n = numbers(args);
                let Some(&h) = n.first() else { return };
                let Some(&i) = self.live.get(&(h as u64)) else { return };
                let inst = &mut self.rec.instances[i];
                match cmd {
                    "EventInstance::set3DAttributes" => {
                        if let Some(p) = pose(&n[1..]) {
                            inst.poses.push((t, p));
                        }
                    }
                    "EventInstance::start" => inst.starts.push(t),
                    "EventInstance::stop" => inst.stops.push(t),
                    "EventInstance::release" => {
                        inst.released = Some(t);
                        self.live.remove(&(h as u64));
                    }
                    _ => {
                        if n.len() >= 3 {
                            match n[1] as i64 {
                                PROP_MIN_DISTANCE => inst.min_override.push((t, n[2] as f32)),
                                PROP_MAX_DISTANCE => inst.max_override.push((t, n[2] as f32)),
                                _ => {}
                            }
                        }
                    }
                }
            }
            "System::setNumListeners" => {
                if let Some(&n) = numbers(args).first() {
                    self.rec.num_listeners = (n as usize).max(1);
                }
            }
            "System::setListenerAttributes" => {
                // index, {{pos},{vel},{fwd},{up}}[, {atten}, useAtten]
                let n = numbers(args);
                let Some(p) = n.get(1..).and_then(pose) else { return };
                let idx = n[0] as usize;
                let atten = if n.len() >= 16 && args.trim_end().ends_with("true") { Some(v3(&n[13..16])) } else { None };
                if idx < 8 {
                    if self.rec.listeners.len() <= idx {
                        self.rec.listeners.resize(idx + 1, Vec::new());
                    }
                    self.rec.listeners[idx].push(ListenerKey { t, pose: p, atten });
                }
            }
            _ => {}
        }
    }
}

/// The last entry at or before `t` in a time-sorted list.
pub fn at<T>(list: &[(f64, T)], t: f64) -> Option<&T> {
    let i = list.partition_point(|(k, _)| *k <= t);
    i.checked_sub(1).map(|i| &list[i].1)
}

fn last_at_or_before(times: &[f64], t: f64) -> Option<f64> {
    let i = times.partition_point(|k| *k <= t);
    i.checked_sub(1).map(|i| times[i])
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Activity {
    /// Created, not started yet (or stopped, still held by the game).
    Idle,
    Playing,
    /// Released without a stop: a fire-and-forget one-shot. It plays on until
    /// it ends, which the capture doesn't say; shown for `oneshot_hold` seconds.
    OneShot,
}

impl Instance {
    /// What the instance is doing at `t`, or None when it doesn't exist then.
    pub fn activity(&self, t: f64, oneshot_hold: f64) -> Option<Activity> {
        if t < self.created {
            return None;
        }
        let start = last_at_or_before(&self.starts, t);
        let stop = last_at_or_before(&self.stops, t);
        let playing = match (start, stop) {
            (Some(s), Some(e)) => s > e,
            (Some(_), None) => true,
            _ => false,
        };
        match self.released {
            Some(r) if t >= r => {
                // After release it lives on only while it plays.
                let s = start?;
                if !playing {
                    return None;
                }
                (t - s.max(r) <= oneshot_hold).then_some(Activity::OneShot)
            }
            _ => Some(if playing { Activity::Playing } else { Activity::Idle }),
        }
    }

    pub fn pose_at(&self, t: f64) -> Option<&Pose> {
        at(&self.poses, t)
    }
}

impl Recording {
    pub fn listener_at(&self, idx: usize, t: f64) -> Option<&ListenerKey> {
        let keys = self.listeners.get(idx)?;
        let i = keys.partition_point(|k| k.t <= t);
        i.checked_sub(1).map(|i| &keys[i]).or(keys.first())
    }

    /// The listener updated most often: the one a game actually drives.
    pub fn busiest_listener(&self) -> usize {
        // max_by_key keeps the last of equals; prefer the lowest index.
        self.listeners.iter().enumerate().rev().max_by_key(|(_, k)| k.len()).map_or(0, |(i, _)| i)
    }

    /// Distinct event ids (GUIDs or paths) seen.
    pub fn events(&self) -> Vec<String> {
        let mut v: Vec<String> = self.instances.iter().map(|i| i.event.clone()).filter(|e| !e.is_empty()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// A low height that most event instances sit on or above: where the
    /// floor grid goes. The 5th percentile of instance heights.
    pub fn ground(&self) -> Option<f32> {
        let mut ys: Vec<f32> = self.instances.iter().flat_map(|i| i.poses.iter().step_by(8).map(|(_, p)| p.pos[1])).collect();
        if ys.is_empty() {
            return None;
        }
        ys.sort_by(f32::total_cmp);
        Some(ys[ys.len() / 20])
    }
}

/// Parses a whole capture text at once.
#[cfg(test)]
pub fn parse(text: &str) -> Recording {
    let mut p = Parser::default();
    p.feed(text);
    if !text.ends_with('\n') {
        p.feed("\n");
    }
    p.rec
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Playback::frame, 0
Playback::version, 2, 131632
#
# FMOD v2.02.30 command capture
#   System::setSoftwareFormat(samplerate: 48000, speakermode: 6, numrawspeakers: 6)
#   Studio::System::initialize(maxchannels: 1024, studioflags: 0x00000024, flags: 0x00520000)
#
System::getEvent, {42c3f7d8-6fa0-4295-accd-000cd27ff096}, 2177536
Bank::getEventListItem, 0, 1, {8cdd9418-3f4d-48fa-9e01-277600657a8e}, 2179168
System::setListenerAttributes, 0, {{0,0,0},{0,0,0},{0,0,1},{0,1,0}}, {0,0,0}, false
Playback::frame, 24000
EventDescription::createInstance, 2177536, 2290176
EventInstance::set3DAttributes, 2290176, {{4085.15,3.18799,2567.48},{0,0,0},{-0.342285,-0,-0.939596},{0,1,0}}
EventInstance::start, 2290176
System::setListenerAttributes, 1, {{10,132.708,20},{0,0,0},{0,-0.866025,0.5},{0,0.5,0.866025}}, {10,2,25}, true
Playback::frame, 24000
EventInstance::setProperty, 2290176, 4, 250
EventInstance::release, 2290176
EventDescription::createInstance, 2179168, 2290176
EventInstance::stop, 2290176, 0
System::getEventByID, {a8dde37f-d3ae-4a9e-b832-7d63226af05f}, 2263552
EventInstance::getDescription, 5097216, 2263552
Playback::frame, 48000
";

    #[test]
    fn reads_time_listeners_probe_and_instances() {
        let r = parse(SAMPLE);
        assert_eq!(r.version, "FMOD v2.02.30");
        assert!(!r.right_handed);
        assert!((r.len - 2.0).abs() < 1e-9, "96000 samples at 48 kHz");
        assert_eq!(r.listeners.len(), 2);
        assert_eq!(r.listeners[0][0].atten, None, "useAttenuation false: no probe");
        assert_eq!(r.listeners[1][0].atten, Some([10.0, 2.0, 25.0]));
        assert_eq!(r.listeners[1][0].pose.fwd, [0.0, -0.866025, 0.5]);
        assert_eq!(r.instances.len(), 3, "a reused handle is a new instance; an unseen one gets a record");
        assert_eq!(r.instances[2].event, "{a8dde37f-d3ae-4a9e-b832-7d63226af05f}");
        let a = &r.instances[0];
        assert_eq!(a.event, "{42c3f7d8-6fa0-4295-accd-000cd27ff096}");
        assert_eq!((a.created, a.starts.clone(), a.released), (0.5, vec![0.5], Some(1.0)));
        assert_eq!(a.max_override, vec![(1.0, 250.0)]);
        assert_eq!(a.pose_at(0.6).unwrap().pos, [4085.15, 3.18799, 2567.48]);
        assert_eq!(r.instances[1].event, "{8cdd9418-3f4d-48fa-9e01-277600657a8e}");
        assert_eq!(r.instances[1].stops, vec![1.0], "the stop went to the new instance");
        assert_eq!(r.events().len(), 3);
    }

    #[test]
    fn activity_over_time() {
        let r = parse(SAMPLE);
        let a = &r.instances[0];
        assert_eq!(a.activity(0.4, 3.0), None, "not created yet");
        assert_eq!(a.activity(0.5, 3.0), Some(Activity::Playing));
        assert_eq!(a.activity(1.2, 3.0), Some(Activity::OneShot), "released while playing");
        assert_eq!(a.activity(4.1, 3.0), None, "one-shot hold over");
        let b = &r.instances[1];
        assert_eq!(b.activity(1.0, 3.0), Some(Activity::Idle), "created and stopped at once");
    }

    #[test]
    fn feeding_in_pieces_matches_one_parse() {
        let mut p = Parser::default();
        for chunk in SAMPLE.as_bytes().chunks(37) {
            p.feed(std::str::from_utf8(chunk).unwrap());
        }
        let whole = parse(SAMPLE);
        assert_eq!(p.rec.instances, whole.instances);
        assert_eq!(p.rec.listeners, whole.listeners);
        assert_eq!(p.rec.len, whole.len);
    }

    #[test]
    fn right_handed_flag_and_old_listener_lines() {
        let r = parse("#   Studio::System::initialize(maxchannels: 64, studioflags: 0x0, flags: 0x00000004)\nSystem::setListenerAttributes, 0, {{1,2,3},{0,0,0},{0,0,1},{0,1,0}}\n");
        assert!(r.right_handed);
        assert_eq!(r.listeners[0][0].atten, None);
        assert_eq!(r.listeners[0][0].pose.pos, [1.0, 2.0, 3.0]);
    }

    /// Checks the parser against a recorded top-down game session, when
    /// PFP_REAL_CAPTURE names its capture file.
    #[test]
    fn real_capture_if_present() {
        let Ok(path) = std::env::var("PFP_REAL_CAPTURE") else { return };
        let text = std::fs::read_to_string(&path).expect("PFP_REAL_CAPTURE");
        let r = parse(&text);
        assert!((r.len - 46.208).abs() < 0.01, "len {}", r.len);
        assert!(r.instances.len() > 100);
        assert!(r.instances.iter().filter(|i| !i.poses.is_empty()).count() > 10);
        assert_eq!(r.listeners.len(), 4, "listener 0 is set once, 1-3 every frame");
        assert_eq!(r.busiest_listener(), 1);
    }
}
