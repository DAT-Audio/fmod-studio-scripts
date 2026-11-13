//! The gain estimate: every level change between the selected event and the
//! Master Bus, for each route the event can take, evaluated twice:
//! - **Now**: each parameter at its current value in Studio (`cursors`);
//! - **Default**: each parameter at its initial value.
//!
//! A route is the event played directly, or the event inside a parent event
//! (inside a grandparent…) up to a top-level event. Values a parent passes
//! into a child event set the child's parameter for that route.
//!
//! Counted: faders, instrument and container volume, Gain effects, VCAs, and
//! their automation (curve.rs). Listed but not counted: modulators, snapshots
//! ("would apply"), sends, and effects whose level depends on the signal.

use crate::curve;
use crate::model::{Automation, Condition, Edge, Focus, Graph, Instrument, Modulation, Node, ParamRef, Strip};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Now,
    Default,
}

/// Parameter values for one evaluation.
pub struct Values<'a> {
    focus: &'a Focus,
    cursors: &'a HashMap<String, f64>,
    /// Values passed in by a parent event instrument, for the event being evaluated.
    overrides: HashMap<String, f64>,
}

impl<'a> Values<'a> {
    pub fn new(focus: &'a Focus, cursors: &'a HashMap<String, f64>) -> Self {
        Values { focus, cursors, overrides: HashMap::new() }
    }

    /// The value of a parameter, and a note when it isn't a plain lookup.
    pub fn param(&self, p: &ParamRef, src: Source) -> (Option<f64>, Option<String>) {
        if p.timeline {
            return (Some(0.0), Some("timeline automation read at its start".into()));
        }
        if let Some(v) = self.overrides.get(&p.id) {
            return (Some(*v), Some(format!("{} = {} (passed in)", p.name, fmt(*v))));
        }
        let info = self.focus.params.get(&p.id);
        match src {
            Source::Now => (self.cursors.get(&p.id).copied().or_else(|| info.and_then(|i| i.cursor)), None),
            Source::Default => (info.and_then(|i| i.initial.or(i.min)), None),
        }
    }
}

/// A property's value: its automation (evaluated) if automated, else its base.
/// Several curves on one property are added (an assumption, noted).
pub fn property(base: Option<f64>, automation: &[Automation], name: &str, vals: &Values, src: Source, notes: &mut Vec<String>) -> f64 {
    let db = curve::is_db(name);
    let mut sum: Option<f64> = None;
    let mut n = 0;
    for a in automation.iter().filter(|a| a.property == name) {
        for c in &a.curves {
            let Some(p) = &c.param else { continue };
            let (x, note) = vals.param(p, src);
            let Some(x) = x else {
                push_once(notes, format!("{} has no value; {} automation skipped", p.name, name));
                continue;
            };
            if let Some(note) = note {
                push_once(notes, note);
            }
            if let Some(v) = curve::value_at(&c.points(), x, db) {
                sum = Some(sum.unwrap_or(0.0) + v);
                n += 1;
                push_once(notes, format!("{name} automated on {} (at {})", p.name, fmt(x)));
            }
        }
    }
    if n > 1 {
        push_once(notes, format!("{n} curves on {name}, added together (assumed)"));
    }
    sum.unwrap_or_else(|| base.unwrap_or(0.0))
}

fn push_once(notes: &mut Vec<String>, s: String) {
    if !notes.contains(&s) {
        notes.push(s);
    }
}

pub fn fmt(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == r.trunc() { format!("{r:.0}") } else { format!("{r}") }
}

pub fn fmt_db(v: f64) -> String {
    if v == f64::NEG_INFINITY || v <= curve::SILENCE_DB {
        "silent".into()
    } else if v.abs() < 0.005 {
        "0.0 dB".into()
    } else {
        format!("{:+.1} dB", v).replace('-', "−")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Step {
    /// Where: "Event 'A' master track", "Event instrument in 'B'", "Bus 'SFX'"…
    pub place: String,
    /// What: "fader", "volume", "Gain effect", "VCA 'x'".
    pub what: String,
    pub now: f64,
    pub default: f64,
    pub notes: Vec<String>,
}

/// One stage of a route (an instrument, a track, an event's master track, a
/// bus…): what it changes, and the running level after it.
#[derive(Clone, Debug, Default)]
pub struct Stage {
    pub place: String,
    pub steps: Vec<Step>,
    pub now: f64,
    pub default: f64,
    /// Level after this stage, from the source (0 dB).
    pub now_total: f64,
    pub default_total: f64,
    pub snapshots: Vec<String>,
    pub uncounted: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Route {
    /// Events from the selected one up to the top-level one.
    pub events: Vec<String>,
    pub direct: bool,
    pub steps: Vec<Step>,
    pub now: f64,
    pub default: f64,
    /// Things on the way that change the level but aren't counted.
    pub uncounted: Vec<String>,
    pub snapshots: Vec<String>,
    /// Trigger conditions on the way, checked at Now values: false if one fails.
    pub plays_now: bool,
    pub blocked_by: Vec<String>,
    pub truncated: bool,
    /// Every place the sound passes, in order (even those that change nothing).
    pub places: Vec<String>,
    /// (place, line) for snapshots and things not counted, per place.
    pub snapshot_at: Vec<(String, String)>,
    pub uncounted_at: Vec<(String, String)>,
    pub stages: Vec<Stage>,
}

impl Route {
    pub fn hops(&self) -> usize {
        self.events.len().saturating_sub(1)
    }
    pub fn title(&self) -> String {
        if self.direct { format!("{} (played directly)", self.events[0]) } else { self.events.join("  in  ") }
    }
}

struct Builder<'a> {
    vals: Values<'a>,
    route: Route,
}

impl Builder<'_> {
    fn step(&mut self, place: &str, what: &str, base: Option<f64>, automation: &[Automation], modulation: &[Modulation], prop: &str) {
        let mut notes = Vec::new();
        let now = property(base, automation, prop, &self.vals, Source::Now, &mut notes);
        let default = property(base, automation, prop, &self.vals, Source::Default, &mut notes);
        for m in modulation.iter().filter(|m| m.property == prop) {
            notes.push(format!("{} on {prop}: varies, not counted", m.kind));
        }
        let automated = automation.iter().any(|a| a.property == prop);
        // Skip steps that change nothing and aren't automated.
        if !automated && now == 0.0 && default == 0.0 && notes.is_empty() {
            return;
        }
        self.route.steps.push(Step { place: place.into(), what: what.into(), now, default, notes });
    }

    fn visit(&mut self, place: &str) {
        if !self.route.places.iter().any(|p| p == place) {
            self.route.places.push(place.to_string());
        }
    }

    fn not_counted(&mut self, place: &str, what: String) {
        self.route.uncounted_at.push((place.to_string(), what.clone()));
        self.route.uncounted.push(format!("{place}: {what}"));
    }

    fn strip(&mut self, s: &Strip, place: &str) {
        self.visit(place);
        self.step(place, "fader", s.volume, &s.automation, &s.modulation, "volume");
        for fx in &s.effects {
            if fx.fader {
                continue;
            }
            let name = if fx.name.is_empty() { fx.kind.clone() } else { format!("{} '{}'", fx.kind, fx.name) };
            if fx.bypass {
                continue;
            }
            if fx.entity == "GainEffect" {
                self.step(place, &format!("{name} effect"), fx.gain, &fx.automation, &fx.modulation, "gain");
            } else if fx.entity == "MixerSend" {
                self.not_counted(place, format!("send to '{}' at {} (a copy; the main path continues)", fx.to.clone().unwrap_or_default(), fx.level.map(fmt_db).unwrap_or_default()));
            } else if let Some(sp) = &fx.spatial {
                let range = format!("{}–{}", sp.min.map(fmt).unwrap_or("?".into()), sp.max.map(fmt).unwrap_or("?".into()));
                let range = if sp.overrides { range } else { format!("{range} (event min/max)") };
                self.not_counted(place, format!("{name}, distance attenuation (range {range})"));
            } else {
                self.not_counted(place, format!("{name} (level depends on the sound)"));
            }
            for sn in &fx.snapshots {
                self.snapshot(place, &format!("{name} {}", sn.property), sn);
            }
        }
        for v in &s.vcas {
            self.step(place, &format!("VCA '{}'", v.name), v.volume, &v.automation, &[], "volume");
            for sn in &v.snapshots {
                self.snapshot(place, &format!("VCA '{}'", v.name), sn);
            }
        }
        for sn in &s.snapshots {
            self.snapshot(place, "", sn);
        }
    }

    fn snapshot(&mut self, place: &str, what: &str, sn: &crate::model::SnapshotOn) {
        let name = sn.snapshot.as_ref().map(|r| r.name.clone()).unwrap_or_default();
        let line = if sn.scoped {
            format!("Snapshot '{name}' scopes {place}")
        } else {
            let what = if what.is_empty() { sn.property.clone() } else { what.to_string() };
            format!(
                "Snapshot '{name}' would set {place} {what} to {} at intensity {}%",
                sn.value.map(|v| if curve::is_db(&sn.property) { fmt_db(v) } else { fmt(v) }).unwrap_or("?".into()),
                sn.intensity.map(fmt).unwrap_or("?".into())
            )
        };
        if !self.route.snapshots.contains(&line) {
            self.route.snapshot_at.push((place.to_string(), line.clone()));
        }
        push_once(&mut self.route.snapshots, line);
    }

    fn instrument(&mut self, i: &Instrument, place: &str, what: &str) {
        self.visit(place);
        self.step(place, what, i.volume, &i.automation, &i.modulation, "volume");
        self.conditions(&i.conditions, place, what, i.condition_mode);
        if i.probability_on && i.probability.is_some_and(|p| p < 100.0) {
            self.not_counted(place, format!("{what} plays {}% of the time", fmt(i.probability.unwrap_or(100.0))));
        }
    }

    fn conditions(&mut self, cs: &[Condition], place: &str, what: &str, mode: Option<f64>) {
        let mut results = Vec::new();
        for c in cs.iter().filter(|c| c.kind == "parameter") {
            let Some(p) = &c.param else { continue };
            let (Some(v), _) = self.vals.param(p, Source::Now) else { continue };
            let inside = c.min.is_none_or(|m| v >= m) && c.max.is_none_or(|m| v <= m);
            results.push((inside != c.inverted, format!("{place}: {what} needs {} {}–{}{} (now {})", p.name,
                c.min.map(fmt).unwrap_or_default(), c.max.map(fmt).unwrap_or_default(), if c.inverted { ", inverted" } else { "" }, fmt(v))));
        }
        if results.is_empty() {
            return;
        }
        // Mode 0 is taken as "all must be met", 1 as "any" (unverified enum).
        let any = mode == Some(1.0);
        let met = if any { results.iter().any(|r| r.0) } else { results.iter().all(|r| r.0) };
        if !met {
            self.route.plays_now = false;
            for (ok, text) in results {
                if !ok {
                    self.route.blocked_by.push(text);
                }
            }
        }
    }

    /// The event's own master track (its output), with its passed-in values.
    fn event_master(&mut self, n: &Node) {
        if let Some(m) = &n.master {
            self.strip(m, &format!("'{}' master track", n.name));
        }
    }

    fn edge(&mut self, e: &Edge, parent: &Node) {
        let place = format!("in '{}'", parent.name);
        for c in &e.containers {
            let what = format!("{} instrument", c.container.clone().unwrap_or("container".into()));
            self.instrument(c, &place, &what);
        }
        self.instrument(&e.instrument, &place, "event instrument");
        if let Some(t) = &e.track {
            if let Some(s) = &t.strip {
                self.strip(s, &format!("'{}' track '{}'", parent.name, t.name));
            }
        }
    }

    fn mixer(&mut self, n: &Node) {
        match &n.route {
            Some(r) => {
                for s in r {
                    let place = if s.entity == "MixerInput" { format!("'{}' in the mixer", n.name) } else { s.label() };
                    self.strip(s, &place);
                }
            }
            None => self.route.truncated = true,
        }
    }
}

fn passed_values(e: &Edge) -> HashMap<String, f64> {
    let mut m = HashMap::new();
    for p in &e.instrument.params {
        if let (Some(r), Some(v)) = (&p.param, p.value) {
            m.insert(r.id.clone(), v);
        }
    }
    m
}

/// Every chain of event links from `start` up to an event nothing in the graph uses.
fn chains<'g>(g: &'g Graph, start: &'g str) -> (Vec<Vec<&'g Edge>>, bool) {
    let mut out = Vec::new();
    let mut capped = false;
    fn walk<'g>(g: &'g Graph, id: &'g str, path: &mut Vec<&'g Edge>, out: &mut Vec<Vec<&'g Edge>>, capped: &mut bool) {
        if out.len() >= 200 {
            *capped = true;
            return;
        }
        let start = path.first().map_or(id, |e| e.child_id.as_str());
        let ups: Vec<&Edge> = g
            .users_of(id)
            .filter(|e| (e.kind.is_empty() || e.kind == "event") && !path.iter().any(|p| p.parent_id == e.parent_id) && e.parent_id != start)
            .collect();
        if ups.is_empty() {
            if !path.is_empty() {
                out.push(path.clone());
            }
            return;
        }
        for e in ups {
            path.push(e);
            walk(g, &e.parent_id, path, out, capped);
            path.pop();
        }
    }
    walk(g, start, &mut Vec::new(), &mut out, &mut capped);
    (out, capped)
}

/// dB addition where silence stays silent.
fn add_db(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY || b == f64::NEG_INFINITY || b <= curve::SILENCE_DB { f64::NEG_INFINITY } else { a + b }
}

fn finish(mut r: Route) -> Route {
    r.now = r.steps.iter().fold(0.0, |a, s| add_db(a, s.now));
    r.default = r.steps.iter().fold(0.0, |a, s| add_db(a, s.default));
    let (mut now_total, mut default_total) = (0.0, 0.0);
    for place in &r.places {
        let steps: Vec<Step> = r.steps.iter().filter(|s| &s.place == place).cloned().collect();
        let now = steps.iter().fold(0.0, |a, s| add_db(a, s.now));
        let default = steps.iter().fold(0.0, |a, s| add_db(a, s.default));
        now_total = add_db(now_total, now);
        default_total = add_db(default_total, default);
        let at = |v: &Vec<(String, String)>| v.iter().filter(|(p, _)| p == place).map(|(_, l)| l.clone()).collect::<Vec<_>>();
        r.stages.push(Stage { place: place.clone(), steps, now, default, now_total, default_total, snapshots: at(&r.snapshot_at), uncounted: at(&r.uncounted_at) });
    }
    r
}

/// All routes for the (first) selected event, loudest (Now) first.
#[cfg(test)]
pub fn routes(focus: &Focus, cursors: &HashMap<String, f64>) -> Vec<Route> {
    let id = focus.graph.as_ref().map(|g| g.focus_id.clone()).unwrap_or_default();
    routes_for(focus, cursors, &id)
}

/// All routes for one selected event, loudest (Now) first.
pub fn routes_for(focus: &Focus, cursors: &HashMap<String, f64>, event_id: &str) -> Vec<Route> {
    let Some(g) = &focus.graph else { return Vec::new() };
    let Some(f) = g.node(event_id).filter(|n| n.is_event()) else { return Vec::new() };
    let mut out = Vec::new();

    // Played directly: its own master track, then its route through the mixer.
    let mut b = Builder { vals: Values::new(focus, cursors), route: Route { events: vec![f.name.clone()], direct: true, plays_now: true, ..Default::default() } };
    b.event_master(f);
    b.mixer(f);
    out.push(finish(b.route));

    let (chains, capped) = chains(g, &f.id);
    for chain in chains {
        let mut b = Builder { vals: Values::new(focus, cursors), route: Route { events: vec![f.name.clone()], plays_now: true, ..Default::default() } };
        // The focus, with whatever its first parent passes in.
        b.vals.overrides = passed_values(chain[0]);
        b.event_master(f);
        for (k, e) in chain.iter().enumerate() {
            let Some(parent) = g.node(&e.parent_id) else { break };
            b.route.events.push(parent.name.clone());
            // The instrument and track live in the parent, which gets its own passed-in values.
            b.vals.overrides = chain.get(k + 1).map(|n| passed_values(n)).unwrap_or_default();
            b.edge(e, parent);
            b.event_master(parent);
            if k + 1 == chain.len() {
                b.mixer(parent);
            }
        }
        b.route.truncated |= capped;
        out.push(finish(b.route));
    }
    sort(&mut out, false);
    out
}

pub fn sort(routes: &mut [Route], by_length: bool) {
    routes.sort_by(|a, b| {
        if by_length {
            b.hops().cmp(&a.hops()).then(b.now.total_cmp(&a.now))
        } else {
            b.now.total_cmp(&a.now).then(b.hops().cmp(&a.hops()))
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::fixtures::*;

    fn by_title<'a>(rs: &'a [Route], top: &str) -> &'a Route {
        rs.iter().find(|r| r.events.last().map(String::as_str) == Some(top) && !(r.direct && top != r.events[0])).unwrap()
    }

    #[test]
    fn routes_for_the_leaf_event() {
        let f = focus(EVENT_LEAF);
        let cursors = HashMap::new();
        let rs = routes(&f, &cursors);
        // Direct, and through B>C, D, E.
        assert_eq!(rs.len(), 4, "{:?}", rs.iter().map(Route::title).collect::<Vec<_>>());
        let direct = rs.iter().find(|r| r.direct).unwrap();
        // Input -1, Foley -2, SFX -3, VCA -4, Gain -6.
        assert!((direct.now + 16.0).abs() < 1e-9, "direct {}", direct.now);
        assert!(direct.snapshots.iter().any(|s| s.contains("Duck")), "{:?}", direct.snapshots);
        let via_c = by_title(&rs, "C_top");
        assert_eq!(via_c.events, vec!["A_leaf", "B_mid", "C_top"]);
        // B>A instrument -3, B track automated on Intensity: cursor 0 → -20 dB.
        assert!((via_c.now + 23.0).abs() < 1e-6, "now {}", via_c.now);
        // Default: Intensity starts at 2 → -20 + 20·u(0.2, bend 0.5).
        let u = curve::shaped(0.2, 0.5, false);
        assert!((via_c.default - (-3.0 + -20.0 + 20.0 * u)).abs() < 1e-6, "default {}", via_c.default);
        // Its condition (Intensity 3..7) isn't met at 0.
        assert!(!via_c.plays_now && via_c.blocked_by[0].contains("Intensity"), "{:?}", via_c.blocked_by);
        let via_d = by_title(&rs, "D_multi");
        assert!((via_d.now + 2.0).abs() < 1e-9, "Multi -2: {}", via_d.now);
        // Loudest first.
        assert!(rs.windows(2).all(|w| w[0].now >= w[1].now));
    }

    #[test]
    fn routes_ignore_asset_and_mixer_links() {
        // In asset mode, A's routes are still A's event routes.
        let f = focus(ASSET);
        let g = f.graph.as_ref().unwrap();
        let a = g.nodes.iter().find(|n| n.name == "A_leaf").unwrap();
        let rs = routes_for(&f, &HashMap::new(), &a.id);
        assert_eq!(rs.len(), 4, "{:?}", rs.iter().map(Route::title).collect::<Vec<_>>());
        assert!(routes_for(&f, &HashMap::new(), &g.focus_id).is_empty(), "an asset has no routes of its own");
    }

    #[test]
    fn stages_add_up_in_order() {
        let f = focus(EVENT_LEAF);
        let rs = routes(&f, &HashMap::new());
        let direct = rs.iter().find(|r| r.direct).unwrap();
        // A's master track, then its input, Foley, SFX (−3, VCA −4, Gain −6), the Master Bus.
        let totals: Vec<f64> = direct.stages.iter().map(|s| s.default_total).collect();
        assert_eq!(totals.last().copied(), Some(direct.default));
        assert!(totals.windows(2).all(|w| w[1] <= w[0]), "{totals:?}");
        let sfx = direct.stages.iter().find(|s| s.place.contains("SFX")).unwrap();
        assert!((sfx.default + 13.0).abs() < 1e-9, "SFX stage {}", sfx.default);
        assert!(sfx.snapshots.iter().any(|l| l.contains("Duck")));
        // Stages that change nothing are still listed (the Master Bus).
        assert!(direct.stages.iter().any(|s| s.place.contains("Master Bus") && s.steps.is_empty()));
    }

    #[test]
    fn cursors_move_the_now_column() {
        let f = focus(EVENT_LEAF);
        let id = f.params.values().find(|p| p.name == "Intensity").unwrap().id.clone();
        let cursors = HashMap::from([(id, 10.0)]);
        let rs = routes(&f, &cursors);
        let via_c = by_title(&rs, "C_top");
        assert!((via_c.now + 3.0).abs() < 1e-6, "Intensity 10 → track at 0 dB: {}", via_c.now);
        let mut by_len = rs.clone();
        sort(&mut by_len, true);
        assert_eq!(by_len[0].events.last().unwrap(), "C_top");
    }
}
