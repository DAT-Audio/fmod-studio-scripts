//! A profiler session's recordings, loaded from its folder and placed on the
//! session timeline, and what was going on at a given time.
//!
//! Recording N (`apicommands-NNNN.txt`) starts at the Nth recording marker the
//! script sends. Without markers (standalone use) recordings are laid end to
//! end. Files are read incrementally, so a recording in progress is followed
//! by `refresh` as it grows (whether Studio writes it while recording is
//! unverified).
//!
//! A session recorded since the last project save lives under
//! `<project>/.unsaved/` until the save moves it, so a session can have two
//! folders. Each recording is read from the first folder that has it.

use crate::camera::V3;
use crate::capture::{Activity, ListenerKey, Parser, Pose, at};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A recording that grew this recently is still being recorded.
const LIVE_FOR: Duration = Duration::from_secs(3);

pub struct RecFile {
    pub num: u32,
    pub path: PathBuf,
    pub parser: Parser,
    read: u64,
    /// When the file last grew after it was first read.
    grew: Option<Instant>,
}

pub struct Loaded {
    /// Where to look, in order: the unsaved folder first, then the saved one.
    pub dirs: Vec<PathBuf>,
    pub recs: Vec<RecFile>,
}

fn rec_number(name: &str) -> Option<u32> {
    name.strip_prefix("apicommands-")?.strip_suffix(".txt")?.parse().ok()
}

impl Loaded {
    pub fn open_dirs(dirs: Vec<PathBuf>) -> Loaded {
        let mut l = Loaded { dirs, recs: Vec::new() };
        l.refresh();
        l
    }

    /// Picks up new recordings and anything appended. True when something changed.
    pub fn refresh(&mut self) -> bool {
        let mut changed = false;
        let mut found: Vec<(u32, PathBuf)> = Vec::new();
        for dir in &self.dirs {
            let Ok(rd) = std::fs::read_dir(dir) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if let Some(num) = rec_number(&name) {
                    if !found.iter().any(|f| f.0 == num) {
                        found.push((num, e.path()));
                    }
                }
            }
        }
        // A recording missing from both folders (mid-save) keeps what was read.
        for (num, path) in found {
            match self.recs.iter_mut().find(|r| r.num == num) {
                None => {
                    self.recs.push(RecFile { num, path, parser: Parser::default(), read: 0, grew: None });
                    changed = true;
                }
                // Moved by a save: the same text in its new place, unless it's shorter.
                Some(r) if r.path != path => {
                    let size = std::fs::metadata(&path).map_or(0, |m| m.len());
                    if size < r.read {
                        *r = RecFile { num, path, parser: Parser::default(), read: 0, grew: None };
                        changed = true;
                    } else {
                        r.path = path;
                    }
                }
                Some(_) => {}
            }
        }
        self.recs.sort_by_key(|r| r.num);
        for r in &mut self.recs {
            changed |= r.read_more();
        }
        changed
    }

    /// Where each recording starts on the session timeline.
    pub fn offsets(&self, markers: &[f64]) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::with_capacity(self.recs.len());
        for i in 0..self.recs.len() {
            // By number: a recording made without API capture has a marker
            // (and a capture-NNNN.wav) but no apicommands file.
            let o = match markers.get(self.recs[i].num as usize) {
                Some(&m) => m,
                None => out.last().map_or(0.0, |&p| p + self.recs[i - 1].parser.rec.len),
            };
            out.push(o);
        }
        out
    }

    pub fn total_len(&self, markers: &[f64]) -> f64 {
        let off = self.offsets(markers);
        off.last().map_or(0.0, |&o| o + self.recs.last().map_or(0.0, |r| r.parser.rec.len))
    }

    /// The recording playing at session time `t` and the time within it.
    /// `gap`: `t` is past that recording's end (between recordings, or after the last).
    pub fn locate(&self, t: f64, markers: &[f64]) -> Option<(usize, f64, bool)> {
        let off = self.offsets(markers);
        let i = off.partition_point(|&o| o <= t).checked_sub(1).or((!off.is_empty()).then_some(0))?;
        let len = self.recs[i].parser.rec.len;
        let local = (t - off[i]).max(0.0);
        // Past the end of a recording still being written: show its latest moment.
        let live = i + 1 == self.recs.len() && self.recs[i].live();
        Some((i, local.min(len), local > len + 0.05 && !live))
    }
}

impl RecFile {
    /// Still being recorded: the file grew in the last few seconds.
    pub fn live(&self) -> bool {
        self.grew.is_some_and(|g| g.elapsed() < LIVE_FOR)
    }

    fn read_more(&mut self) -> bool {
        let Ok(mut f) = std::fs::File::open(&self.path) else { return false };
        let Ok(size) = f.metadata().map(|m| m.len()) else { return false };
        if size <= self.read {
            return false;
        }
        if f.seek(SeekFrom::Start(self.read)).is_err() {
            return false;
        }
        let mut buf = Vec::with_capacity((size - self.read) as usize);
        if f.take(size - self.read).read_to_end(&mut buf).is_err() {
            return false;
        }
        // Whole lines only, so a UTF-8 character is never split.
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return false };
        self.parser.feed(&String::from_utf8_lossy(&buf[..=end]));
        if self.read > 0 {
            self.grew = Some(Instant::now());
        }
        self.read += end as u64 + 1;
        true
    }
}

/// One event instance as it is at a moment.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveInstance {
    /// Index in the recording's instance list (stable while it lives).
    pub index: usize,
    pub handle: u64,
    pub event: String,
    pub pose: Pose,
    pub activity: Activity,
    pub min_override: Option<f32>,
    pub max_override: Option<f32>,
}

/// Everything at one moment of a session.
#[derive(Clone, Debug, Default)]
pub struct Moment {
    pub rec: usize,
    pub local: f64,
    pub gap: bool,
    pub right_handed: bool,
    /// Per listener index (None: not set yet in this recording).
    pub listeners: Vec<Option<ListenerKey>>,
    /// The listener the game updates most in this recording (Auto follows it).
    pub busiest: usize,
    pub instances: Vec<LiveInstance>,
    /// Instances alive but with no 3D position (2D, or never placed).
    pub unplaced: usize,
    /// Instances left out by the event filter.
    pub hidden: usize,
    /// The floor height (see Recording::ground).
    pub ground: Option<f32>,
}

impl Loaded {
    pub fn moment(&self, t: f64, markers: &[f64], oneshot_hold: f64, ground: Option<f32>) -> Option<Moment> {
        let (ri, local, gap) = self.locate(t, markers)?;
        let rec = &self.recs[ri].parser.rec;
        let mut m = Moment {
            rec: ri,
            local,
            gap,
            right_handed: rec.right_handed,
            listeners: (0..rec.listeners.len()).map(|i| rec.listener_at(i, local).copied()).collect(),
            busiest: rec.busiest_listener(),
            ground,
            ..Default::default()
        };
        if gap {
            return Some(m);
        }
        for (index, inst) in rec.instances.iter().enumerate() {
            let Some(activity) = inst.activity(local, oneshot_hold) else { continue };
            match inst.pose_at(local) {
                Some(&pose) => m.instances.push(LiveInstance {
                    index,
                    handle: inst.handle,
                    event: inst.event.clone(),
                    pose,
                    activity,
                    min_override: at(&inst.min_override, local).copied(),
                    max_override: at(&inst.max_override, local).copied(),
                }),
                None => m.unplaced += 1,
            }
        }
        Some(m)
    }
}

impl Moment {
    /// The listener the view follows (falls back to the first one set).
    pub fn listener(&self, idx: usize) -> Option<&ListenerKey> {
        self.listeners.get(idx).and_then(|l| l.as_ref()).or_else(|| self.listeners.iter().flatten().next())
    }

    /// Where attenuation is measured from for a listener: its probe (the
    /// attenuation position) when the game sets one, else the listener.
    pub fn hearing_point(l: &ListenerKey) -> V3 {
        l.atten.unwrap_or(l.pose.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pfp_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const REC: &str = "System::getEvent, {e}, 1
System::setListenerAttributes, 0, {{0,0,0},{0,0,0},{0,0,1},{0,1,0}}, {0,0,5}, true
EventDescription::createInstance, 1, 10
EventInstance::set3DAttributes, 10, {{3,0,4},{0,0,0},{0,0,1},{0,1,0}}
EventInstance::start, 10
EventDescription::createInstance, 1, 11
Playback::frame, 48000
EventInstance::stop, 10, 0
Playback::frame, 48000
";

    #[test]
    fn recordings_sit_at_their_markers_or_end_to_end() {
        let d = tmp("markers");
        write(&d, "apicommands-0000.txt", REC);
        write(&d, "apicommands-0001.txt", REC);
        let l = Loaded::open_dirs(vec![d.clone()]);
        assert_eq!(l.recs.len(), 2);
        assert_eq!(l.offsets(&[]), vec![0.0, 2.0], "end to end without markers");
        assert_eq!(l.offsets(&[0.0, 10.0]), vec![0.0, 10.0]);
        assert_eq!(l.locate(11.5, &[0.0, 10.0]), Some((1, 1.5, false)));
        let (i, local, gap) = l.locate(5.0, &[0.0, 10.0]).unwrap();
        assert_eq!((i, local, gap), (0, 2.0, true), "between recordings");
        assert_eq!(l.total_len(&[0.0, 10.0]), 12.0);
    }

    #[test]
    fn a_recording_without_api_capture_keeps_the_others_on_their_markers() {
        let d = tmp("skip");
        write(&d, "apicommands-0000.txt", REC);
        write(&d, "apicommands-0002.txt", REC);
        let l = Loaded::open_dirs(vec![d.clone()]);
        assert_eq!(l.offsets(&[0.0, 10.0, 20.0]), vec![0.0, 20.0]);
    }

    #[test]
    fn a_moment_has_listener_probe_and_playing_instances() {
        let d = tmp("moment");
        write(&d, "apicommands-0000.txt", REC);
        let l = Loaded::open_dirs(vec![d.clone()]);
        let m = l.moment(0.5, &[], 3.0, None).unwrap();
        let li = m.listener(0).unwrap();
        assert_eq!(Moment::hearing_point(li), [0.0, 0.0, 5.0], "the probe, not the listener");
        assert_eq!(m.instances.len(), 1);
        assert_eq!(m.instances[0].pose.pos, [3.0, 0.0, 4.0]);
        assert_eq!(m.instances[0].activity, Activity::Playing);
        assert_eq!(m.unplaced, 1, "instance 11 has no position");
        let m = l.moment(1.5, &[], 3.0, None).unwrap();
        assert_eq!(m.instances[0].activity, Activity::Idle, "stopped at 1 s");
    }

    #[test]
    fn growing_files_are_followed() {
        let d = tmp("grow");
        let (a, b) = REC.split_at(REC.find("Playback::frame").unwrap() + 5);
        write(&d, "apicommands-0000.txt", a);
        let mut l = Loaded::open_dirs(vec![d.clone()]);
        assert_eq!(l.recs[0].parser.rec.len, 0.0);
        write(&d, "apicommands-0000.txt", &format!("{a}{b}"));
        assert!(l.refresh());
        assert_eq!(l.recs[0].parser.rec.len, 2.0, "the half line was kept and finished");
        assert!(!l.refresh(), "nothing new");
        write(&d, "apicommands-0001.txt", REC);
        assert!(l.refresh());
        assert_eq!(l.recs.len(), 2);
    }

    #[test]
    fn a_growing_last_recording_is_live_and_has_no_gap_past_its_end() {
        let d = tmp("live");
        let (a, b) = REC.split_at(REC.find("EventInstance::stop").unwrap());
        write(&d, "apicommands-0000.txt", a);
        let mut l = Loaded::open_dirs(vec![d.clone()]);
        assert!(!l.recs[0].live(), "a file read once isn't live");
        assert!(l.locate(5.0, &[]).unwrap().2, "past the end of a finished recording");
        write(&d, "apicommands-0000.txt", &format!("{a}{b}"));
        assert!(l.refresh());
        assert!(l.recs[0].live());
        assert_eq!(l.locate(5.0, &[]), Some((0, 2.0, false)), "its latest moment");
    }

    #[test]
    fn unsaved_and_saved_folders_merge_and_a_save_moves_without_rereading() {
        let unsaved = tmp("unsaved");
        let saved = tmp("saved");
        write(&saved, "apicommands-0000.txt", REC);
        write(&unsaved, "apicommands-0001.txt", REC);
        let mut l = Loaded::open_dirs(vec![unsaved.clone(), saved.clone()]);
        assert_eq!(l.recs.len(), 2, "one recording from each folder");
        assert_eq!(l.recs[1].path, unsaved.join("apicommands-0001.txt"));
        // The project is saved: the folder moves.
        std::fs::rename(unsaved.join("apicommands-0001.txt"), saved.join("apicommands-0001.txt")).unwrap();
        assert!(!l.refresh(), "same content, only the path changed");
        assert_eq!(l.recs[1].path, saved.join("apicommands-0001.txt"));
        assert_eq!(l.recs[1].parser.rec.len, 2.0);
    }
}
