//! The event filter: an opt-in list of which events the view draws, in its
//! own window, as a folder tree like Studio's Event Browser.
//!
//! - **Draw only ticked events** is off by default (everything is drawn). When
//!   it's on, an event is drawn only once its box is ticked. Nothing is
//!   ticked to start with.
//! - The list holds every event the session's captures mention, and new ones
//!   are added as they appear.
//! - A folder's box ticks or unticks every event listed under it at the time;
//!   it shows a dash when only some are ticked.
//! - Child events (played by an event's instruments) follow their parent.

use crate::link::EventInfo;
use crate::view;
use eframe::egui::{self, Color32, RichText};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EventFilter {
    /// Draw only ticked events.
    pub on: bool,
    /// Ticked event ids (GUIDs). Kept across sessions and runs.
    pub checked: HashSet<String>,
    #[serde(skip)]
    pub open: bool,
    #[serde(skip)]
    pub search: String,
}

impl EventFilter {
    pub fn shows(&self, event: &str) -> bool {
        !self.on || self.checked.contains(event)
    }
}

/// One event seen in the session.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub instances: usize,
    pub colour: Color32,
}

/// Where an event goes in the tree, and its name there.
pub fn place(id: &str, info: Option<&EventInfo>, linked: bool) -> (Vec<String>, String) {
    match info {
        Some(i) if i.missing => (vec!["(not in this project)".into()], id.to_string()),
        Some(i) if !i.path.is_empty() => {
            let (top, rest) = match i.path.split_once(":/") {
                Some(("event", r)) => (None, r),
                Some(("snapshot", r)) => (Some("Snapshots"), r),
                Some((_, r)) => (None, r),
                None => (None, i.path.as_str()),
            };
            let mut parts: Vec<String> = rest.split('/').filter(|p| !p.is_empty()).map(String::from).collect();
            let name = parts.pop().unwrap_or_else(|| i.name.clone());
            if let Some(t) = top {
                parts.insert(0, t.to_string());
            }
            (parts, name)
        }
        Some(i) => (Vec::new(), i.name.clone()),
        None => (vec![if linked { "(waiting for Studio)" } else { "(no Studio link)" }.into()], id.to_string()),
    }
}

pub fn entries(counts: &HashMap<String, usize>, events: &HashMap<String, EventInfo>, linked: bool) -> Vec<Entry> {
    let mut v: Vec<Entry> = counts
        .iter()
        .map(|(id, &n)| {
            let info = events.get(id);
            let (folders, name) = place(id, info, linked);
            let mut path = folders.join("/");
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(&name);
            Entry { id: id.clone(), name, path, instances: n, colour: view::event_colour(info, id) }
        })
        .collect();
    v.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
    v
}

#[derive(Default, Debug)]
pub struct Folder {
    pub name: String,
    /// Sorted case-insensitively, like the Event Browser.
    pub folders: BTreeMap<String, Folder>,
    pub events: Vec<usize>,
}

pub fn tree(list: &[Entry]) -> Folder {
    let mut root = Folder::default();
    for (i, e) in list.iter().enumerate() {
        let mut parts: Vec<&str> = e.path.split('/').collect();
        parts.pop();
        let mut f = &mut root;
        for p in parts {
            f = f.folders.entry(p.to_lowercase()).or_insert_with(|| Folder { name: p.to_string(), ..Default::default() });
        }
        f.events.push(i);
    }
    root
}

impl Folder {
    pub fn ids<'a>(&self, list: &'a [Entry], out: &mut Vec<&'a str>) {
        out.extend(self.events.iter().map(|&i| list[i].id.as_str()));
        for f in self.folders.values() {
            f.ids(list, out);
        }
    }
}

/// Ticks every id when not all are ticked, else unticks them all.
pub fn toggle_all(checked: &mut HashSet<String>, ids: &[&str]) {
    if ids.iter().all(|id| checked.contains(*id)) {
        for id in ids {
            checked.remove(*id);
        }
    } else {
        checked.extend(ids.iter().map(|s| s.to_string()));
    }
}

fn swatch(ui: &mut egui::Ui, c: Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, c);
}

fn event_row(ui: &mut egui::Ui, e: &Entry, checked: &mut HashSet<String>, label: &str) {
    ui.horizontal(|ui| {
        let mut on = checked.contains(&e.id);
        if ui.checkbox(&mut on, "").changed() {
            if on {
                checked.insert(e.id.clone());
            } else {
                checked.remove(&e.id);
            }
        }
        swatch(ui, e.colour);
        ui.label(label).on_hover_text(&e.id);
        ui.label(RichText::new(format!("{}×", e.instances)).color(Color32::GRAY).small()).on_hover_text("Instances in this session");
    });
}

fn folder_ui(ui: &mut egui::Ui, f: &Folder, path: &str, list: &[Entry], checked: &mut HashSet<String>) {
    for (key, sub) in &f.folders {
        let id = format!("{path}/{key}");
        let mut ids = Vec::new();
        sub.ids(list, &mut ids);
        let n_on = ids.iter().filter(|i| checked.contains(**i)).count();
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(&id), true)
            .show_header(ui, |ui| {
                let mut all = n_on == ids.len() && !ids.is_empty();
                let r = ui.add(egui::Checkbox::new(&mut all, "").indeterminate(n_on > 0 && n_on < ids.len()));
                if r.clicked() {
                    toggle_all(checked, &ids);
                }
                ui.label(RichText::new(&sub.name).strong());
                ui.label(RichText::new(format!("{n_on}/{}", ids.len())).color(Color32::GRAY).small());
            })
            .body(|ui| folder_ui(ui, sub, &id, list, checked));
    }
    for &i in &f.events {
        event_row(ui, &list[i], checked, &list[i].name);
    }
}

/// The window's contents.
pub fn contents(ui: &mut egui::Ui, filter: &mut EventFilter, list: &[Entry]) {
    ui.checkbox(&mut filter.on, RichText::new("Draw only ticked events").strong());
    ui.label(
        RichText::new(if filter.on {
            "Only ticked events are drawn. Ticking a folder ticks every event listed under it."
        } else {
            "Off: every event is drawn. Turn on, then tick the events to draw."
        })
        .color(Color32::GRAY),
    );
    ui.separator();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut filter.search).hint_text("Search").desired_width(140.0));
        let ids: Vec<&str> = list.iter().map(|e| e.id.as_str()).collect();
        if ui.button("Tick all").clicked() {
            filter.checked.extend(ids.iter().map(|s| s.to_string()));
        }
        if ui.button("Untick all").clicked() {
            for id in &ids {
                filter.checked.remove(*id);
            }
        }
        let n = list.iter().filter(|e| filter.checked.contains(&e.id)).count();
        ui.label(RichText::new(format!("{n} of {} ticked", list.len())).color(Color32::GRAY));
    });
    ui.add_space(4.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if list.is_empty() {
            ui.label(RichText::new("No events in this session's captures yet.").color(Color32::GRAY));
        }
        let q = filter.search.trim().to_lowercase();
        if q.is_empty() {
            folder_ui(ui, &tree(list), "", list, &mut filter.checked);
        } else {
            for e in list.iter().filter(|e| e.path.to_lowercase().contains(&q) || e.id.to_lowercase().contains(&q)) {
                event_row(ui, e, &mut filter.checked, &e.path);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(path: &str) -> EventInfo {
        EventInfo { id: path.into(), name: path.rsplit('/').next().unwrap().into(), path: path.into(), ..Default::default() }
    }

    #[test]
    fn events_sit_in_their_browser_folders() {
        assert_eq!(place("{a}", Some(&info("event:/amb/city/Fire")), true), (vec!["amb".into(), "city".into()], "Fire".into()));
        assert_eq!(place("{s}", Some(&info("snapshot:/Pause")), true), (vec!["Snapshots".into()], "Pause".into()));
        let missing = EventInfo { missing: true, ..Default::default() };
        assert_eq!(place("{m}", Some(&missing), true).0, vec!["(not in this project)".to_string()]);
        assert_eq!(place("{w}", None, true).0, vec!["(waiting for Studio)".to_string()]);
    }

    #[test]
    fn tree_and_folder_ticking() {
        let mut events = HashMap::new();
        for (id, p) in [("{a}", "event:/amb/Fire"), ("{b}", "event:/amb/Wind"), ("{c}", "event:/ui/Click")] {
            events.insert(id.to_string(), EventInfo { id: id.into(), ..info(p) });
        }
        let counts: HashMap<String, usize> = ["{a}", "{b}", "{c}"].iter().map(|s| (s.to_string(), 1)).collect();
        let list = entries(&counts, &events, true);
        let t = tree(&list);
        assert_eq!(t.folders.keys().collect::<Vec<_>>(), vec!["amb", "ui"]);
        let mut ids = Vec::new();
        t.folders["amb"].ids(&list, &mut ids);
        assert_eq!(ids.len(), 2);
        let mut checked = HashSet::new();
        checked.insert("{a}".to_string());
        toggle_all(&mut checked, &ids);
        assert!(checked.contains("{a}") && checked.contains("{b}"), "some ticked: the folder ticks all");
        toggle_all(&mut checked, &ids);
        assert!(checked.is_empty(), "all ticked: the folder unticks all");
    }

    #[test]
    fn off_shows_everything_on_shows_only_ticked() {
        let mut f = EventFilter::default();
        assert!(f.shows("{a}"));
        f.on = true;
        assert!(!f.shows("{a}"), "on: nothing is drawn until ticked");
        f.checked.insert("{a}".into());
        assert!(f.shows("{a}"));
    }
}
