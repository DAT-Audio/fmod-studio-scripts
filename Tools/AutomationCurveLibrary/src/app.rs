//! The Curve Library window.
//!
//! - Left: the libraries. Factory (read-only) first, then the user's libraries,
//!   with their folders and curves. Right-click anything for its actions.
//! - Centre: the selected curve on the canvas (view-only for read-only curves).
//! - Right: exact point values, or the Studio tab (when started by Studio with
//!   `--bridge`): what is selected in Studio, applying the selected curve to
//!   it, and saving curves from it into a library.
//!
//! Point edits stay in the app until **Save** (Cmd+S); unsaved curves are
//! marked with a dot, and closing with unsaved changes asks first. Library
//! structure changes (new, rename, move, copy, delete) happen straight away.
//! The libraries folder is watched, so libraries copied in (from a repo, say)
//! appear without a restart.

use crate::bridge::{ApplyCtx, Bridge, LinkState, Pending};
use crate::canvas::{Axis, Canvas, DisplayRange, Overlay, delete_selected, insert_point, move_group};
use crate::curve::{self, SegmentShape, Transform};
use crate::theme::{self, Look, pal};
use crate::library::{self, CurveData, Library, Point, Shelf, file_name, templates};
use crate::studio::{self, Range, Reply, Snapshot, Target};
use eframe::egui::{self, Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, RichText, Sense};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_UNDO: usize = 200;
/// Layout limits (points). The window's minimum size in main.rs fits all three.
const LEFT_MIN: f32 = 200.0;
const RIGHT_MIN: f32 = 380.0;
const MIN_GRAPH_WIDTH: f32 = 420.0;
const WATCH_EVERY: Duration = Duration::from_millis(1500);
/// Studio's selection is polled this often (cheap on the Studio side) …
const SELECTION_EVERY: Duration = Duration::from_millis(700);
/// … and the full list of shared effects this often.
const SHARED_EVERY: Duration = Duration::from_millis(8000);
const STORAGE_SELECTED: &str = "selected_curve";
/// Favourite curves (ids, as JSON), kept in the app's own settings on this
/// machine: personal, never written into shared library files.
const STORAGE_FAVOURITES: &str = "favourite_curves";
/// Whether the curve already in Studio is drawn behind yours ("1"/"0").
const STORAGE_SHOW_STUDIO_CURVE: &str = "show_studio_curve";
const STORAGE_AUTO_FIT: &str = "auto_fit";
/// The look ("dat" or "classic"); see theme.rs. A new key in 0.9.1: 0.9.0
/// saved its "dat" default for everyone, and Classic is the default again.
const STORAGE_LOOK: &str = "look_v2";
/// Set once the guide has been shown on first run.
const STORAGE_GUIDE_SEEN: &str = "guide_seen";
/// The user guide (GUIDE.md next to Cargo.toml), built into the app.
const GUIDE: &str = include_str!("../GUIDE.md");

/// The oldest Studio script this app works with (0.6.0 has the reply-ack
/// fix; 0.7.0 adds removeCurve for Undo last apply).
const MIN_SCRIPT: (u32, u32, u32) = (0, 8, 2);

/// A drag in the library tree carries the curve's path.
#[derive(Clone, Debug)]
struct DragCurve(PathBuf);

/// A bold, filled button for an action that changes things (Apply, Save, Copy).
fn primary(ui: &mut egui::Ui, enabled: bool, text: impl Into<String>) -> egui::Response {
    let text = text.into();
    let b = egui::Button::new(RichText::new(text).strong().color(if enabled { pal().on_accent } else { pal().dim }))
        .fill(if enabled { pal().accent } else { pal().raised })
        .min_size(egui::vec2(96.0, 30.0));
    ui.add_enabled(enabled, b)
}

/// A filled red button for destructive confirmations.
fn danger(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text.into()).strong().color(Color32::WHITE)).fill(pal().danger).min_size(egui::vec2(96.0, 30.0)))
}

/// A titled block in the Studio panel: a slightly darker card, so each part
/// (target, placement, apply, curves) reads as its own section.
fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_help(ui, title, "", add)
}

/// A card with a "(?)" beside its title that explains it on hover.
fn card_help<R>(ui: &mut egui::Ui, title: &str, help: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(pal().ground)
        .stroke(egui::Stroke::new(1.0, pal().hairline))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 5.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(14.0).strong().color(pal().strong));
                if !help.is_empty() {
                    ui.label(RichText::new("(?)").color(pal().dim)).on_hover_text(help);
                }
            });
            add(ui)
        })
        .inner
}

/// A small rounded label beside a name ("Shared effect", "3 uses").
fn pill(ui: &mut egui::Ui, text: &str, colour: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.16))
        .stroke(egui::Stroke::new(1.0, colour.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(7, 1))
        .show(ui, |ui| ui.label(RichText::new(text).small().color(colour)))
        .response
}


/// A unit short enough for a field suffix (" dB", " Hz", " %"); long notes
/// like "0-1 = 0-100%" are dropped.
fn short_unit(unit: &str) -> String {
    if unit.is_empty() || unit.len() > 9 || unit.contains(' ') { String::new() } else { format!(" {unit}") }
}

fn parse_version(v: &str) -> (u32, u32, u32) {
    let mut it = v.split('.').map(|p| p.trim().parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}


#[derive(Clone, Debug)]
enum DeleteWhat {
    Curve(PathBuf),
    Folder(PathBuf, Vec<String>),
    Library(PathBuf),
}

/// Things to do after the UI pass (the tree is drawn from a borrow of the shelf).
#[derive(Clone, Debug)]
enum Action {
    Select(PathBuf),
    /// Send curves to Studio's 2.04 curve presets (one request per folder).
    SendPresets(Vec<PathBuf>),
    /// Save the Studio edit's points into a library (asks for a name first).
    SaveStudioEdit { points: Vec<Point>, name: String, description: String },
    NewLibrary,
    NewFolder { lib: PathBuf, parent: Vec<String> },
    NewCurve { lib: PathBuf, folder: Vec<String> },
    CreateCurve { lib: PathBuf, folder: Vec<String>, data: CurveData },
    RenameCurve(PathBuf),
    RenameFolder { lib: PathBuf, folder: Vec<String> },
    RenameLibrary(PathBuf),
    Duplicate(PathBuf),
    CopyTo { path: PathBuf, lib: PathBuf, folder: Vec<String> },
    MoveTo { path: PathBuf, lib: PathBuf, folder: Vec<String> },
    /// A tree drag: put `src` into (lib, folder), before `before` (else last).
    /// From a read-only library it's a copy; otherwise a move or a reorder.
    Drop { src: PathBuf, lib: PathBuf, folder: Vec<String>, before: Option<PathBuf> },
    Delete(DeleteWhat),
    Reveal(PathBuf),
    SaveAll,
    ToggleFavourite(String),
}

impl Action {
    /// The library this action writes into, if any.
    fn writes_into(&self, shelf: &Shelf) -> Option<PathBuf> {
        match self {
            Action::CreateCurve { lib, .. } | Action::CopyTo { lib, .. } | Action::MoveTo { lib, .. } | Action::Drop { lib, .. } => Some(lib.clone()),
            Action::Duplicate(p) => shelf.find(p).map(|(l, _)| l.dir.clone()).filter(|d| shelf.library(d).is_some_and(|l| !l.read_only())),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
enum NameAction {
    NewLibrary,
    NewFolder { lib: PathBuf, parent: Vec<String> },
    NewCurve,
    RenameCurve(PathBuf),
    RenameFolder { lib: PathBuf, folder: Vec<String> },
    RenameLibrary(PathBuf),
    SaveCapture { points: Vec<Point>, description: String },
    /// Name the auto-created library, then carry on with `then`.
    NameDefault { lib: PathBuf, then: Box<Action> },
}

struct NameDialog {
    title: String,
    note: String,
    value: String,
    action: NameAction,
    template: usize,
    dest: usize,
    error: String,
    focused: bool,
}

/// A curve opened from Studio with "Edit here": it lives in the shelf's
/// scratch slot, and remembers exactly where it came from and the ranges it
/// was mapped with, so applying it puts it back unchanged apart from your edits.
#[derive(Clone)]
struct StudioEdit {
    ctx: ApplyCtx,
    target_key: String,
    range: Range,
    x: Axis,
    y: Axis,
    /// The points as opened (or as last applied), to tell whether it changed.
    applied: Vec<Point>,
    /// The library curve to go back to when done.
    back_to: Option<PathBuf>,
}

/// The Studio tab's apply form.
struct ApplyForm {
    target: Option<String>,
    /// With several things selected in Studio: the keys of the ones the
    /// apply goes to (all of them, until you untick some).
    include: BTreeSet<String>,
    /// The user picked a target by hand; Studio's next selection change takes over again.
    manual: bool,
    prev_selection: Vec<String>,
    property: Option<String>,
    param: Option<String>,
    range: Range,
    filled_for: (String, String, String),
    value_source: String,
    value_known: bool,
    position_estimated: bool,
}

impl Default for ApplyForm {
    fn default() -> Self {
        ApplyForm {
            target: None,
            include: BTreeSet::new(),
            manual: false,
            prev_selection: Vec::new(),
            property: None,
            param: None,
            range: Range { plo: 0.0, phi: 1.0, vlo: 0.0, vhi: 1.0 },
            filled_for: Default::default(),
            value_source: String::new(),
            value_known: false,
            position_estimated: false,
        }
    }
}

pub struct EditorApp {
    root: PathBuf,
    shelf: Shelf,
    last_stamp: u64,
    last_watch: Instant,
    selected: Option<PathBuf>,
    point: Option<usize>,
    canvas: Canvas,
    undo: Vec<(PathBuf, Vec<Point>)>,
    redo: Vec<(PathBuf, Vec<Point>)>,
    /// Curves with edits that aren't saved yet.
    unsaved: BTreeSet<PathBuf>,
    status: (String, Color32),
    display: DisplayRange,
    filter: String,
    name_dialog: Option<NameDialog>,
    confirm: Option<(DeleteWhat, String)>,
    /// Open the tree to the selected curve and scroll to it (once).
    reveal: bool,
    quit_prompt: bool,
    allow_close: bool,
    show_guide: bool,
    last_title: String,
    bridge: Option<Bridge>,
    snap: Option<Snapshot>,
    last_light: Instant,
    last_full: Instant,
    /// The "All points" table under the graph is open.
    show_points_table: bool,
    form: ApplyForm,
    /// The last successful apply, for "Undo last apply".
    last_apply: Vec<ApplyCtx>,
    /// A short message shown above the bottom bar for a few seconds.
    toast: Option<(String, Color32, Instant)>,
    /// Draw the target's current Studio curve (dashed, with its points) behind yours.
    show_studio_curve: bool,
    /// Zoom back out by itself a moment after leaving the graph.
    auto_fit: bool,
    /// Fold every tree folder once (the "Collapse all" button).
    collapse_all: bool,
    /// The library panel's area last frame, for context hints.
    tree_rect: egui::Rect,
    /// The Studio script's version, from its hello reply.
    script_version: Option<String>,
    /// Favourite curve ids (see STORAGE_FAVOURITES).
    favourites: BTreeSet<String>,
    /// The look (DAT Audio or Classic); see theme.rs.
    look: Look,
    /// Studio has FMOD 2.04's project curve presets (from its hello reply).
    studio_presets: bool,
    /// The library Studio's presets are imported into.
    preset_dest: usize,
    /// A Studio curve being edited in place (see StudioEdit).
    studio_edit: Option<StudioEdit>,
    /// Development: save the window to this PNG after a few frames, then quit.
    screenshot: Option<PathBuf>,
    frames: u32,
}

impl EditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, root: PathBuf, screenshot: Option<PathBuf>, bridge: bool) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        // ACL_LOOK=classic|dat overrides the saved look (development screenshots).
        let look = std::env::var("ACL_LOOK").ok().or_else(|| cc.storage.and_then(|s| s.get_string(STORAGE_LOOK))).map_or(Look::Classic, |k| Look::from_key(&k));
        theme::apply(&cc.egui_ctx, look);
        let _ = std::fs::create_dir_all(&root);
        let mut status = (format!("Libraries: {}", root.display()), pal().dim);
        match library::ensure_user_library(&root) {
            Ok(Some(dir)) => status = (format!("Created your library '{}' in {}", file_name(&dir), root.display()), pal().dim),
            Ok(None) => {}
            Err(e) => status = (format!("Couldn't create a user library: {e}"), pal().bad),
        }
        let remembered = cc.storage.and_then(|s| s.get_string(STORAGE_SELECTED)).map(PathBuf::from);
        let long_ago = Instant::now().checked_sub(Duration::from_secs(60)).unwrap_or_else(Instant::now);
        let mut app = EditorApp {
            shelf: Shelf::scan(&root),
            last_stamp: library::stamp(&root),
            root,
            last_watch: Instant::now(),
            selected: None,
            point: Some(0),
            canvas: Canvas::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            unsaved: BTreeSet::new(),
            status,
            display: DisplayRange::normalised(),
            filter: String::new(),
            name_dialog: None,
            confirm: None,
            reveal: true,
            quit_prompt: false,
            allow_close: false,
            // The guide opens by itself the first time (ACL_SHOW_GUIDE forces it, for screenshots).
            show_guide: std::env::var_os("ACL_SHOW_GUIDE").is_some()
                || (cc.storage.is_some() && cc.storage.and_then(|s| s.get_string(STORAGE_GUIDE_SEEN)).is_none()),
            last_title: String::new(),
            bridge: None,
            snap: None,
            last_light: long_ago,
            last_full: long_ago,
            show_points_table: false,
            form: ApplyForm::default(),
            last_apply: Vec::new(),
            toast: None,
            collapse_all: false,
            auto_fit: cc.storage.and_then(|s| s.get_string(STORAGE_AUTO_FIT)).is_none_or(|v| v != "0"),
            show_studio_curve: cc.storage.and_then(|s| s.get_string(STORAGE_SHOW_STUDIO_CURVE)).is_none_or(|v| v != "0"),
            tree_rect: egui::Rect::NOTHING,
            script_version: None,
            favourites: cc
                .storage
                .and_then(|s| s.get_string(STORAGE_FAVOURITES))
                .and_then(|j| serde_json::from_str::<BTreeSet<String>>(&j).ok())
                .unwrap_or_default(),
            screenshot,
            frames: 0,
            studio_edit: None,
            studio_presets: false,
            look,
            preset_dest: 0,
        };
        app.selected = remembered
            .filter(|p| app.shelf.find(p).is_some())
            .or_else(|| app.shelf.libraries.iter().flat_map(|l| l.curves.iter()).next().map(|c| c.path.clone()));
        if bridge {
            let ctx = cc.egui_ctx.clone();
            let mut b = Bridge::stdio(move || ctx.request_repaint());
            b.request("hello", serde_json::json!({}), Pending::Hello);
            app.bridge = Some(b);
        }
        app
    }

    // ── Libraries on disk ───────────────────────────────────────────────────

    fn current(&self) -> Option<(&Library, &library::Curve)> {
        self.selected.as_ref().and_then(|p| self.shelf.find(p))
    }

    fn current_editable(&self) -> bool {
        self.current().is_some_and(|(l, _)| !l.read_only())
    }

    /// Every message goes to one place: a toast above the bottom bar.
    fn set_status(&mut self, text: impl Into<String>, color: Color32) {
        let text = text.into();
        self.status = (text.clone(), color);
        self.toast = Some((text, color, Instant::now()));
    }

    /// Re-reads every library, keeping unsaved edits and the selection (by
    /// the curve's id if its file moved).
    fn rescan(&mut self) {
        let keep_id = self.current().map(|(_, c)| c.data.id.clone());
        let pending: Vec<(PathBuf, CurveData)> =
            self.unsaved.iter().filter_map(|p| self.shelf.find(p).map(|(_, c)| (p.clone(), c.data.clone()))).collect();
        let scratch = self.shelf.scratch.take();
        self.shelf = Shelf::scan(&self.root);
        self.shelf.scratch = scratch;
        self.last_stamp = library::stamp(&self.root);
        let mut lost = 0;
        for (p, data) in pending {
            match self.shelf.find_mut(&p) {
                Some(c) => {
                    let order = c.data.order; // the order on disk wins (it may have just been changed by a drag)
                    c.data = data;
                    c.data.order = order;
                }
                None => {
                    self.unsaved.remove(&p);
                    lost += 1;
                }
            }
        }
        if lost > 0 {
            self.set_status(format!("{lost} curve(s) with unsaved edits were removed or renamed on disk; those edits are gone"), pal().warn);
        }
        let still = self.selected.as_ref().is_some_and(|p| self.shelf.find(p).is_some());
        if !still {
            self.selected = keep_id
                .and_then(|id| self.shelf.libraries.iter().flat_map(|l| l.curves.iter()).find(|c| c.data.id == id).map(|c| c.path.clone()))
                .or_else(|| self.shelf.libraries.iter().flat_map(|l| l.curves.iter()).next().map(|c| c.path.clone()));
        }
        self.clamp_point();
    }

    fn select(&mut self, path: PathBuf) {
        // Opening a library curve closes a Studio edit (there's no "back to
        // it": pull the curve into the graph again).
        if !Shelf::is_scratch(&path)
            && let Some(e) = self.studio_edit.take()
        {
            let now = self.shelf.scratch.as_ref().and_then(|l| l.curves.first()).map(|c| c.data.points.clone());
            if now.is_some_and(|p| p != e.applied) {
                self.set_status(format!("Closed the edit of {} · {} without applying it", e.ctx.target_label, e.y.title), pal().warn);
            }
            self.shelf.scratch = None;
            self.undo.retain(|(p, _)| !Shelf::is_scratch(p));
            self.redo.retain(|(p, _)| !Shelf::is_scratch(p));
        }
        if self.selected.as_ref() != Some(&path) {
            self.point = Some(0);
            self.canvas.select_only(Some(0));
            // A new curve starts with the whole of it in view.
            self.canvas.fit();
        }
        self.selected = Some(path);
    }

    fn clamp_point(&mut self) {
        let n = self.current().map_or(0, |(_, c)| c.data.points.len());
        self.point = if n == 0 { None } else { Some(self.point.unwrap_or(0).min(n - 1)) };
    }

    /// Saves one curve if it has unsaved edits (a no-op otherwise, so it's
    /// safe before copying a read-only Factory curve).
    fn save_path(&mut self, path: &Path) -> Result<(), String> {
        if !self.unsaved.contains(path) {
            return Ok(());
        }
        let Some(c) = self.shelf.find_mut(path) else { return Ok(()) };
        c.data.modified = library::now_iso();
        let data = c.data.clone();
        library::save_curve(&self.root, path, &data)?;
        self.unsaved.remove(path);
        self.last_stamp = library::stamp(&self.root);
        Ok(())
    }

    fn save_all(&mut self) {
        let paths: Vec<PathBuf> = self.unsaved.iter().cloned().collect();
        let mut errors = Vec::new();
        for p in &paths {
            if let Err(e) = self.save_path(p) {
                errors.push(e);
            }
        }
        if errors.is_empty() {
            self.set_status(format!("Saved {} curve{}", paths.len(), if paths.len() == 1 { "" } else { "s" }), pal().ok);
        } else {
            self.set_status(format!("Not saved: {}", errors.join("; ")), pal().bad);
        }
    }

    fn revert_current(&mut self) {
        let Some(path) = self.selected.clone() else { return };
        match library::read_curve(&path) {
            Ok(data) => {
                if let Some(c) = self.shelf.find_mut(&path) {
                    c.data = data;
                }
                self.unsaved.remove(&path);
                self.clamp_point();
                self.set_status("Reverted to the saved curve", pal().dim);
            }
            Err(e) => self.set_status(e, pal().bad),
        }
    }

    fn set_points(&mut self, path: &Path, pts: Vec<Point>) {
        if let Some(c) = self.shelf.find_mut(path) {
            c.data.points = pts;
            if !Shelf::is_scratch(path) {
                self.unsaved.insert(path.to_path_buf());
            }
        }
    }

    fn push_undo(&mut self, path: PathBuf, pts: Vec<Point>) {
        self.undo.push((path, pts));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn step_history(&mut self, back: bool) {
        let entry = if back { self.undo.pop() } else { self.redo.pop() };
        let Some((path, pts)) = entry else { return };
        let Some(now) = self.shelf.find(&path).map(|(_, c)| c.data.points.clone()) else { return };
        if back { self.redo.push((path.clone(), now)) } else { self.undo.push((path.clone(), now)) }
        self.select(path.clone());
        self.set_points(&path, pts);
        self.canvas.select_only(self.point);
        self.clamp_point();
    }

    /// Rewrites stored paths after a library folder was renamed.
    fn remap_paths(&mut self, old: &Path, new: &Path) {
        let fix = |p: &PathBuf| p.strip_prefix(old).map(|rest| new.join(rest)).unwrap_or_else(|_| p.clone());
        self.selected = self.selected.as_ref().map(fix);
        self.unsaved = self.unsaved.iter().map(fix).collect();
        for (p, _) in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            *p = fix(p);
        }
    }

    fn run(&mut self, action: Action) {
        // Ask for a real name before the first write into the auto-created library.
        if let Some(lib) = action.writes_into(&self.shelf).or_else(|| match &action {
            Action::SaveAll => self.unsaved.iter().filter_map(|p| self.shelf.find(p)).find(|(l, _)| l.needs_naming()).map(|(l, _)| l.dir.clone()),
            _ => None,
        }) && self.shelf.library(&lib).is_some_and(Library::needs_naming)
        {
            let name = self.shelf.library(&lib).map(|l| l.name().to_string()).unwrap_or_default();
            self.open_name("Name your library", &name, NameAction::NameDefault { lib, then: Box::new(action) });
            if let Some(d) = self.name_dialog.as_mut() {
                d.note = "Your first library was created as 'My Curves'. Give it a name you'd recognise when it's shared (for example, your name or the project's).".into();
            }
            return;
        }
        let res: Result<Option<PathBuf>, String> = match action {
            Action::SendPresets(paths) => {
                self.send_presets(&paths);
                Ok(None)
            }
            Action::SaveStudioEdit { points, name, description } => {
                self.open_name("Save curve to library", &name, NameAction::SaveCapture { points, description });
                Ok(None)
            }
            Action::Select(p) => {
                self.select(p);
                Ok(None)
            }
            Action::NewLibrary => {
                self.open_name("New library", "My library", NameAction::NewLibrary);
                Ok(None)
            }
            Action::NewFolder { lib, parent } => {
                self.open_name("New folder", "New folder", NameAction::NewFolder { lib, parent });
                Ok(None)
            }
            Action::NewCurve { lib, folder } => {
                self.open_name("New curve", "New curve", NameAction::NewCurve);
                if let Some(d) = self.name_dialog.as_mut() {
                    d.dest = self.shelf.destinations().iter().position(|(l, f, _)| *l == lib && *f == folder).unwrap_or(0);
                }
                Ok(None)
            }
            Action::CreateCurve { lib, folder, data } => library::new_curve(&lib, &folder, &data).map(Some),
            Action::RenameCurve(p) => {
                let name = self.shelf.find(&p).map(|(_, c)| c.data.name.clone()).unwrap_or_default();
                self.open_name("Rename curve", &name, NameAction::RenameCurve(p));
                Ok(None)
            }
            Action::RenameFolder { lib, folder } => {
                let name = folder.last().cloned().unwrap_or_default();
                self.open_name("Rename folder", &name, NameAction::RenameFolder { lib, folder });
                Ok(None)
            }
            Action::RenameLibrary(dir) => {
                let name = self.shelf.library(&dir).map(|l| l.name().to_string()).unwrap_or_default();
                self.open_name("Rename library", &name, NameAction::RenameLibrary(dir));
                Ok(None)
            }
            Action::Duplicate(p) => {
                let target = self.shelf.find(&p).and_then(|(l, c)| {
                    if l.read_only() { self.shelf.first_writable().map(|w| (w.dir.clone(), Vec::new())) } else { Some((l.dir.clone(), c.folder.clone())) }
                });
                match target {
                    Some((lib, folder)) => self.save_path(&p).and_then(|_| library::copy_curve(&p, &lib, &folder)).map(Some),
                    None => Err("There's no library of yours to copy into. Make one with New library.".into()),
                }
            }
            Action::CopyTo { path, lib, folder } => self.save_path(&path).and_then(|_| library::copy_curve(&path, &lib, &folder)).map(Some),
            Action::MoveTo { path, lib, folder } => self.save_path(&path).and_then(|_| library::move_curve(&self.root, &path, &lib, &folder)).map(Some),
            Action::Drop { src, lib, folder, before } => self.drop_curve(&src, &lib, &folder, before.as_deref()),
            Action::Delete(what) => {
                let text = match &what {
                    DeleteWhat::Curve(p) => format!("Delete the curve '{}'?", self.shelf.find(p).map(|(_, c)| c.data.name.clone()).unwrap_or_default()),
                    DeleteWhat::Folder(_, f) => format!("Delete the folder '{}' and every curve in it?", f.join(" / ")),
                    DeleteWhat::Library(d) => format!(
                        "Delete the library '{}' and every curve in it? Its folder is removed from {}.",
                        self.shelf.library(d).map(|l| l.name().to_string()).unwrap_or_default(),
                        self.root.display()
                    ),
                };
                self.confirm = Some((what, text));
                Ok(None)
            }
            Action::Reveal(p) => {
                reveal(&p);
                Ok(None)
            }
            Action::SaveAll => {
                self.save_all();
                Ok(None)
            }
            Action::ToggleFavourite(id) => {
                if !self.favourites.remove(&id) {
                    self.favourites.insert(id);
                }
                Ok(None)
            }
        };
        match res {
            Ok(Some(new_path)) => {
                self.rescan();
                self.select(new_path);
                self.reveal = true;
                self.set_status("Done", pal().dim);
            }
            Ok(None) => {}
            Err(e) => self.set_status(e, pal().bad),
        }
    }

    /// Carries out a tree drag (see `Action::Drop`). Returns the curve's new path.
    fn drop_curve(&mut self, src: &Path, lib: &Path, folder: &[String], before: Option<&Path>) -> Result<Option<PathBuf>, String> {
        if self.shelf.library(lib).is_none_or(Library::read_only) {
            return Err("That library is read-only.".into());
        }
        if before == Some(src) {
            return Ok(None);
        }
        let from_read_only = self.shelf.find(src).is_some_and(|(l, _)| l.read_only());
        let dir = folder.iter().fold(lib.to_path_buf(), |d, f| d.join(f));
        let placed = if from_read_only {
            library::copy_curve(src, lib, folder)?
        } else if src.parent() == Some(dir.as_path()) {
            src.to_path_buf()
        } else {
            self.save_path(src)?;
            let new = library::move_curve(&self.root, src, lib, folder)?;
            if self.unsaved.remove(src) {
                self.unsaved.insert(new.clone());
            }
            if self.selected.as_deref() == Some(src) {
                self.selected = Some(new.clone());
            }
            new
        };
        self.rescan();
        // Order the folder: everything else as shown, with the dropped curve
        // before `before` (or last).
        let mut paths: Vec<PathBuf> = self
            .shelf
            .library(lib)
            .map(|l| l.curves_in(folder).into_iter().map(|c| c.path.clone()).filter(|p| *p != placed).collect())
            .unwrap_or_default();
        let at = before.and_then(|b| paths.iter().position(|p| p == b)).unwrap_or(paths.len());
        paths.insert(at, placed.clone());
        library::set_order(&self.root, &paths)?;
        if from_read_only {
            self.set_status("Copied from Factory into your library", pal().ok);
        }
        Ok(Some(placed))
    }

    fn open_name(&mut self, title: &str, value: &str, action: NameAction) {
        let dest = self.selected.as_ref().and_then(|p| self.shelf.find(p)).and_then(|(l, c)| {
            self.shelf.destinations().iter().position(|(dl, df, _)| *dl == l.dir && *df == c.folder)
        });
        self.name_dialog = Some(NameDialog {
            title: title.into(),
            note: String::new(),
            value: value.into(),
            action,
            template: 0,
            dest: dest.unwrap_or(0),
            error: String::new(),
            focused: false,
        });
    }

    /// Carries out a name dialog. Ok(true) closes it.
    fn commit_name(&mut self, d: &NameDialog, keep_default: bool) -> Result<bool, String> {
        let name = d.value.trim().to_string();
        if name.is_empty() && !keep_default {
            return Err("Enter a name.".into());
        }
        let dest = self.shelf.destinations().get(d.dest).map(|(l, f, _)| (l.clone(), f.clone()));
        let mut follow_up = None;
        let created: Option<PathBuf> = match &d.action {
            NameAction::NewLibrary => {
                library::create_library(&self.root, &name)?;
                None
            }
            NameAction::NewFolder { lib, parent } => {
                library::create_folder(lib, parent, &name)?;
                None
            }
            NameAction::NewCurve => {
                let (lib, folder) = dest.ok_or("Make a library of your own first (New library).")?;
                let pts = templates().get(d.template).map(|t| t.1.clone()).unwrap_or_default();
                follow_up = Some(Action::CreateCurve { lib, folder, data: CurveData::new(&name, pts, "") });
                None
            }
            NameAction::SaveCapture { points, description } => {
                let (lib, folder) = dest.ok_or("Make a library of your own first (New library).")?;
                follow_up = Some(Action::CreateCurve { lib, folder, data: CurveData::new(&name, points.clone(), description) });
                None
            }
            NameAction::RenameCurve(p) => {
                self.save_path(p)?;
                Some(library::rename_curve(&self.root, p, &name)?)
            }
            NameAction::RenameFolder { lib, folder } => {
                library::rename_folder(lib, folder, &name)?;
                None
            }
            NameAction::RenameLibrary(dir) => {
                let new = library::rename_library(dir, &name)?;
                self.remap_paths(dir, &new);
                None
            }
            NameAction::NameDefault { lib, then } => {
                let mut then = (**then).clone();
                if keep_default {
                    library::keep_library_name(lib)?;
                } else {
                    let new = library::rename_library(lib, &name)?;
                    self.remap_paths(lib, &new);
                    remap_action(&mut then, lib, &new);
                }
                follow_up = Some(then);
                None
            }
        };
        self.rescan();
        if let Some(p) = created {
            self.select(p);
            self.reveal = true;
        }
        if let Some(a) = follow_up {
            self.run(a);
        } else {
            self.set_status("Done", pal().dim);
        }
        Ok(true)
    }

    // ── Panels ──────────────────────────────────────────────────────────────

    fn library_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.heading("Libraries");
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new("New library…").wrap_mode(egui::TextWrapMode::Extend)).clicked() {
                actions.push(Action::NewLibrary);
            }
            if ui.add(egui::Button::new("Collapse all").wrap_mode(egui::TextWrapMode::Extend)).clicked() {
                self.collapse_all = true;
            }
        });
        ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Search curves").desired_width(f32::INFINITY));
        ui.add_space(4.0);
        if !self.shelf.problems.is_empty() {
            egui::CollapsingHeader::new(RichText::new(format!("{} file(s) couldn't be read", self.shelf.problems.len())).color(pal().warn).small())
                .id_salt("problems")
                .show(ui, |ui| {
                    for p in &self.shelf.problems {
                        ui.label(RichText::new(p).color(pal().warn).small());
                    }
                });
        }
        let dests = self.shelf.destinations();
        let reveal = self.reveal;
        let collapse = std::mem::take(&mut self.collapse_all);
        let presets = self.studio_presets && self.bridge.as_ref().is_some_and(Bridge::is_linked);
        let tree = Tree { presets, selected: self.selected.as_deref(), unsaved: &self.unsaved, dests: &dests, reveal, favourites: &self.favourites, collapse };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let filter = self.filter.trim().to_lowercase();
            if !filter.is_empty() {
                // A flat list: search matches and/or favourites, with where each lives.
                let mut any = false;
                for l in &self.shelf.libraries {
                    let hits = l.curves.iter().filter(|c| c.data.name.to_lowercase().contains(&filter));
                    for c in hits {
                        any = true;
                        let place = if c.folder.is_empty() { l.name().to_string() } else { format!("{} / {}", l.name(), c.folder.join(" / ")) };
                        ui.add_space(2.0);
                        let (r, star) = curve_row(
                            ui,
                            c,
                            egui::Button::selectable(tree.selected == Some(c.path.as_path()), format!("{}{}", c.data.name, tree.dot(&c.path))),
                            self.favourites.contains(&c.data.id),
                        );
                        ui.label(RichText::new(place).small().color(pal().dim));
                        if star {
                            actions.push(Action::ToggleFavourite(c.data.id.clone()));
                        }
                        if r.clicked() {
                            actions.push(Action::Select(c.path.clone()));
                        }
                        tree.curve_menu(&r, l, c, actions);
                    }
                }
                if !any {
                    ui.label(RichText::new("No curves match.").color(pal().dim));
                }
                return;
            }
            // Favourites first, so they're visible however the folders are
            // folded (each also keeps its star where it lives).
            let favs: Vec<(&Library, &library::Curve)> = self
                .shelf
                .libraries
                .iter()
                .flat_map(|l| l.curves.iter().map(move |c| (l, c)))
                .filter(|(_, c)| self.favourites.contains(&c.data.id))
                .collect();
            egui::CollapsingHeader::new(RichText::new(format!("Favourites ({})", favs.len())).strong())
                .id_salt("favourites")
                .default_open(true)
                .open(if collapse { Some(false) } else { None })
                .show(ui, |ui| {
                    if favs.is_empty() {
                        ui.add(egui::Label::new(RichText::new("Click the star beside a curve to add it here. Favourites are kept on this machine only.").color(pal().dim).small()).wrap());
                    }
                    for (l, c) in &favs {
                        let (r, star) = curve_row(
                            ui,
                            c,
                            egui::Button::selectable(tree.selected == Some(c.path.as_path()), format!("{}{}", c.data.name, tree.dot(&c.path))),
                            true,
                        );
                        let r = r.on_hover_text(format!("In {}", if c.folder.is_empty() { l.name().to_string() } else { format!("{} / {}", l.name(), c.folder.join(" / ")) }));
                        if star {
                            actions.push(Action::ToggleFavourite(c.data.id.clone()));
                        }
                        if r.clicked() {
                            actions.push(Action::Select(c.path.clone()));
                        }
                        tree.curve_menu(&r, l, c, actions);
                    }
                });
            for l in &self.shelf.libraries {
                let holds_selected = tree.selected.is_some_and(|s| s.starts_with(&l.dir));
                let title = if l.read_only() { format!("{}   (read-only)", l.name()) } else { l.name().to_string() };
                let r = egui::CollapsingHeader::new(RichText::new(title).strong())
                    .id_salt(("lib", &l.dir))
                    .default_open(true)
                    .open(if reveal && holds_selected { Some(true) } else if collapse { Some(false) } else { None })
                    .show(ui, |ui| tree.folder(ui, l, &[], actions));
                tree.drop_target(&r.header_response, l, &[], None, actions);
                r.header_response.context_menu(|ui| {
                    if !l.read_only() {
                        if ui.button("New curve…").clicked() {
                            actions.push(Action::NewCurve { lib: l.dir.clone(), folder: Vec::new() });
                            ui.close();
                        }
                        if ui.button("New folder…").clicked() {
                            actions.push(Action::NewFolder { lib: l.dir.clone(), parent: Vec::new() });
                            ui.close();
                        }
                        if ui.button("Rename library…").clicked() {
                            actions.push(Action::RenameLibrary(l.dir.clone()));
                            ui.close();
                        }
                    } else {
                        ui.label(RichText::new("Read-only: copy curves to your own library to change them.").small());
                    }
                    if ui.button(reveal_label()).clicked() {
                        actions.push(Action::Reveal(l.dir.clone()));
                        ui.close();
                    }
                    if tree.presets && ui.button("Send library to Studio's curve presets").on_hover_text(PRESET_HELP).clicked() {
                        actions.push(Action::SendPresets(l.curves.iter().map(|c| c.path.clone()).collect()));
                        ui.close();
                    }
                    if !l.read_only() {
                        ui.separator();
                        if ui.button("Delete library…").clicked() {
                            actions.push(Action::Delete(DeleteWhat::Library(l.dir.clone())));
                            ui.close();
                        }
                    }
                });
            }
            if self.shelf.libraries.is_empty() {
                ui.label(RichText::new(format!("No libraries in {}", self.root.display())).color(pal().warn));
            }
        });
        self.reveal = false;
    }

    fn centre(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some((lib, curve)) = self.current() else {
            ui.centered_and_justified(|ui| ui.label(RichText::new("Select a curve, or make one with New curve (right-click your library).").color(pal().dim)));
            return;
        };
        let (ro, path, name) = (lib.read_only(), curve.path.clone(), curve.data.name.clone());
        let place = if curve.folder.is_empty() { lib.name().to_string() } else { format!("{} / {}", lib.name(), curve.folder.join(" / ")) };
        let description = curve.data.description.clone();
        let dests = self.shelf.destinations();
        let unsaved = self.unsaved.contains(&path);
        let id = curve.data.id.clone();
        let fav = self.favourites.contains(&id);
        let scratch = Shelf::is_scratch(&path);
        if scratch {
            self.studio_edit_header(ui, actions);
        } else {
        ui.horizontal_wrapped(|ui| {
            ui.heading(&name);
            ui.label(RichText::new(&place).color(pal().dim));
            if ro {
                ui.label(RichText::new("read-only").color(pal().warn));
                if let Some((wl, wf, wlabel)) = dests.first()
                    && primary(ui, true, format!("Copy to '{wlabel}' to edit")).on_hover_text("Copies this curve into your library, where you can edit it (or drag it there in the list)").clicked()
                {
                    actions.push(Action::CopyTo { path: path.clone(), lib: wl.clone(), folder: wf.clone() });
                }
                if dests.len() > 1 {
                    ui.menu_button("Copy to…", |ui| {
                        for (l, f, label) in &dests {
                            if ui.button(label).clicked() {
                                actions.push(Action::CopyTo { path: path.clone(), lib: l.clone(), folder: f.clone() });
                                ui.close();
                            }
                        }
                    });
                }
            } else if unsaved {
                ui.label(RichText::new("unsaved").color(pal().accent));
                if ui.button("Revert").on_hover_text("Discard this curve's unsaved edits").clicked() {
                    self.revert_current();
                }
            }
            if star_button(ui, fav, 8.0, "Favourite this curve")
                .on_hover_text(if fav { "Favourite: click to remove. The star in the list's search row shows favourites only." } else { "Add to favourites (kept on this machine only)" })
                .clicked()
            {
                actions.push(Action::ToggleFavourite(id.clone()));
            }
        });
        if ro {
            if !description.is_empty() {
                ui.label(RichText::new(&description).color(pal().dim).small());
            }
        } else {
            let mut d = description.clone();
            let r = ui.add(egui::TextEdit::singleline(&mut d).hint_text("Description (optional)").desired_width(f32::INFINITY));
            if r.changed()
                && let Some(c) = self.shelf.find_mut(&path)
            {
                c.data.description = d;
                self.unsaved.insert(path.clone());
            }
        }
        ui.label(RichText::new("Range-free: applying stretches this shape over any parameter and property range; bends are unchanged.").color(pal().dim).small());
        }

        let before = self.shelf.find(&path).map(|(_, c)| c.data.points.clone()).unwrap_or_default();
        let mut pts = before.clone();
        let out = self.canvas.show(ui, &mut pts, &mut self.point, &self.display, !ro);
        if out.begin_edit {
            self.push_undo(path.clone(), before);
        }
        if out.changed {
            self.set_points(&path, pts);
        }
    }

    /// Under the graph: the selected point's exact values and the point
    /// actions, with every point's values one click away.
    fn points_strip(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.selected.clone() else { return };
        let editable = self.current_editable();
        let Some(before) = self.shelf.find(&path).map(|(_, c)| c.data.points.clone()) else { return };
        let n = before.len();
        if n == 0 {
            return;
        }
        let i = self.point.unwrap_or(0).min(n - 1);
        let mut pts = before.clone();
        let (mut begin, mut changed) = (false, false);
        ui.horizontal_wrapped(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            let n_sel = self.canvas.selection.len();
            let title = if n_sel > 1 { format!("Point {} of {n}  ({n_sel} selected)", i + 1) } else { format!("Point {} of {n}", i + 1) };
            ui.label(RichText::new(title).strong());
            if ui.add_enabled(i > 0, egui::Button::new("Previous").small()).on_hover_text("[ with the pointer over the graph").clicked() {
                self.point = Some(i - 1);
                self.canvas.select_only(self.point);
            }
            if ui.add_enabled(i + 1 < n, egui::Button::new("Next").small()).on_hover_text("] with the pointer over the graph").clicked() {
                self.point = Some(i + 1);
                self.canvas.select_only(self.point);
            }
            ui.separator();
            let lo = if i > 0 { pts[i - 1].x } else { 0.0 };
            let hi = if i + 1 < n { pts[i + 1].x } else { 1.0 };
            let last = i + 1 == n;
            let p = &mut pts[i];
            ui.add_enabled_ui(editable, |ui| {
                for (label, tip, v, range, speed, dec, on) in [
                    ("Position", "0 to 1 along the parameter", &mut p.x, lo..=hi, 0.001, 4, true),
                    ("Value", "0 to 1 of the value range", &mut p.y, 0.0..=1.0, 0.001, 4, true),
                    ("Bend", "Shapes the segment after this point, -1 to 1 (an S-curve's strength; 0 is straight in Studio)", &mut p.shape, -1.0..=1.0, 0.005, 3, !last),
                ] {
                    ui.label(RichText::new(label).color(pal().dim)).on_hover_text(tip);
                    let r = ui.add_enabled(on, egui::DragValue::new(v).range(range).speed(speed).fixed_decimals(dec));
                    begin |= r.drag_started() || r.gained_focus();
                    changed |= r.changed();
                }
                if ui.add_enabled(!last, egui::Checkbox::new(&mut p.s, "S-curve")).on_hover_text("Make the segment after this point an S-curve").changed() {
                    begin = true;
                    changed = true;
                }
            });
        });
        if begin {
            self.push_undo(path.clone(), before.clone());
        }
        if changed {
            self.set_points(&path, pts.clone());
        }
        ui.horizontal_wrapped(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            let pts = before.clone();
            ui.add_enabled_ui(editable, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add point").on_hover_text("Adds a point halfway along the selected segment").clicked() {
                        let mut pts = pts.clone();
                        let i = self.point.unwrap_or(0).min(pts.len().saturating_sub(2));
                        let (x, y) = if pts.len() >= 2 {
                            let x = (pts[i].x + pts[i + 1].x) / 2.0;
                            (x, curve::value_at(&pts, x))
                        } else {
                            (1.0, pts.first().map_or(0.5, |p| p.y))
                        };
                        self.push_undo(path.clone(), before.clone());
                        self.point = Some(insert_point(&mut pts, x, y));
                        self.canvas.select_only(self.point);
                        self.set_points(&path, pts);
                    }
                    let count = self.canvas.selection.len().max(1);
                    let label = if count > 1 { format!("Delete {count} points") } else { "Delete point".into() };
                    if ui.add_enabled(n > 2 && self.point.is_some(), egui::Button::new(label)).clicked() {
                        let mut pts = pts.clone();
                        self.push_undo(path.clone(), before.clone());
                        let sel = if self.canvas.selection.is_empty() { self.point.into_iter().collect() } else { self.canvas.selection.clone() };
                        delete_selected(&mut pts, &sel);
                        self.point = Some(self.point.unwrap_or(0).min(pts.len() - 1));
                        self.canvas.select_only(self.point);
                        self.set_points(&path, pts);
                    }
                });
                // Shape of the segment after the selected point.
                if let Some(i) = self.point.filter(|&i| i + 1 < n) {
                    ui.horizontal(|ui| {
                        ui.label(format!("Segment {}-{} shape", i + 1, i + 2));
                        ui.menu_button("Change…", |ui| {
                            for (shape, name) in SegmentShape::all() {
                                if shape == SegmentShape::Steps(2) || shape == SegmentShape::Wave(1) {
                                    ui.separator();
                                }
                                let label = if shape.is_native() { name.to_string() } else { format!("{name}  (adds points)") };
                                if ui.button(label).clicked() {
                                    self.push_undo(path.clone(), before.clone());
                                    self.set_points(&path, curve::reshape_segment(&before, i, shape));
                                    ui.close();
                                }
                            }
                        });
                    });
                }
            });
            ui.separator();
            // Whole-curve edits.
            let mut chosen = None;
            ui.add_enabled_ui(editable, |ui| {
                for (t, label, help) in Transform::all() {
                    if ui.button(label).on_hover_text(help).clicked() {
                        chosen = Some(t);
                    }
                }
            });
            if let Some(t) = chosen {
                self.push_undo(path.clone(), before.clone());
                let pts = curve::transform(&before, t);
                self.point = Some(self.point.unwrap_or(0).min(pts.len().saturating_sub(1)));
                self.canvas.select_only(self.point);
                self.set_points(&path, pts);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.show_points_table { "Hide all points" } else { "All points…" };
                if ui.button(label).on_hover_text("Every point's exact values in a table").clicked() {
                    self.show_points_table = !self.show_points_table;
                }
            });
        });
        if self.show_points_table {
            ui.separator();
            self.points_table(ui);
        }
    }

    /// Every point's exact values (the "All points" table under the graph).
    fn points_table(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.selected.clone() else {
            ui.label(RichText::new("Select a curve.").color(pal().dim));
            return;
        };
        let editable = self.current_editable();
        let Some(before) = self.shelf.find(&path).map(|(_, c)| c.data.points.clone()) else { return };
        let mut pts = before.clone();
        let n = pts.len();
        let shift = ui.ctx().input(|i| i.modifiers.shift);
        let (mut begin, mut changed, mut select) = (false, false, None);
        egui::ScrollArea::vertical().id_salt("points_table").max_height(200.0).show(ui, |ui| {
            egui::Grid::new("points").striped(true).num_columns(5).spacing([8.0, 4.0]).show(ui, |ui| {
                for (h, tip) in [
                    ("#", "Point number. Click to select; Shift-click to add."),
                    ("Position", "0 to 1 along the parameter."),
                    ("Value", "0 to 1 of the value range."),
                    ("Bend", "The shape of the segment after this point. For an S-curve, its strength (0 is straight in Studio)."),
                    ("S-curve", "Make the segment after this point an S-curve."),
                ] {
                    ui.label(RichText::new(h).strong()).on_hover_text(tip);
                }
                ui.end_row();
                for i in 0..n {
                    let lo = if i > 0 { pts[i - 1].x } else { 0.0 };
                    let hi = if i + 1 < n { pts[i + 1].x } else { 1.0 };
                    let marked = self.point == Some(i) || self.canvas.selection.contains(&i);
                    if ui.selectable_label(marked, format!("{}", i + 1)).clicked() {
                        select = Some(i);
                    }
                    let last = i + 1 == n;
                    let p = &mut pts[i];
                    for (k, (v, range, speed, dec)) in [(&mut p.x, lo..=hi, 0.001, 4), (&mut p.y, 0.0..=1.0, 0.001, 4), (&mut p.shape, -1.0..=1.0, 0.005, 3)]
                        .into_iter()
                        .enumerate()
                    {
                        ui.add_enabled_ui(editable && !(k == 2 && last), |ui| {
                            let r = ui.add(egui::DragValue::new(v).range(range).speed(speed).fixed_decimals(dec));
                            begin |= r.drag_started() || r.gained_focus();
                            changed |= r.changed();
                            if r.gained_focus() || r.drag_started() {
                                select = Some(i);
                            }
                        });
                    }
                    ui.add_enabled_ui(editable && !last, |ui| {
                        if ui.checkbox(&mut p.s, "").changed() {
                            begin = true;
                            changed = true;
                        }
                    });
                    ui.end_row();
                }
            });
        });
        if begin {
            self.push_undo(path.clone(), before.clone());
        }
        if changed {
            self.set_points(&path, pts.clone());
        }
        if let Some(i) = select {
            if shift {
                self.canvas.selection.insert(i);
            } else {
                self.canvas.select_only(Some(i));
            }
            self.point = Some(i);
        }
        ui.horizontal(|ui| {
        ui.label(RichText::new("Label values as").color(pal().dim)).on_hover_text("Display only, when not linked to a Studio target: curves are always stored 0-1.");
        egui::ComboBox::from_id_salt("display")
            .selected_text(if self.display.is_normalised() { "0-1".to_string() } else { format!("{} to {} {}", self.display.lo, self.display.hi, self.display.unit) })
            .show_ui(ui, |ui| {
                if ui.selectable_label(self.display.is_normalised(), "0-1").clicked() {
                    self.display = DisplayRange::normalised();
                }
                for (lo, hi, unit, label) in [
                    (-80.0, 10.0, "dB", "Volume / gain  (-80 to +10 dB)"),
                    (0.0, 100.0, "%", "Percent  (0-100%)"),
                    (10.0, 22000.0, "Hz", "Frequency  (10 Hz-22 kHz, linear)"),
                ] {
                    let d = DisplayRange { lo, hi, unit: unit.into() };
                    if ui.selectable_label(self.display == d, label).clicked() {
                        self.display = d;
                    }
                }
            });
        });
        if !self.display.is_normalised() {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.display.lo).speed(0.1).prefix("from "));
                ui.add(egui::DragValue::new(&mut self.display.hi).speed(0.1).prefix("to "));
                ui.add(egui::TextEdit::singleline(&mut self.display.unit).desired_width(40.0));
            });
            if self.display.unit == "dB" {
                ui.label(RichText::new("Studio draws dB on a fader scale, so curves look lower there. Values are exact.").color(pal().warn).small());
            }
        }
    }

    // ── Studio link ─────────────────────────────────────────────────────────

    fn link_tick(&mut self) {
        let Some(bridge) = self.bridge.as_mut() else { return };
        let replies = bridge.poll();
        if bridge.is_linked() && !bridge.is_waiting(|p| matches!(p, Pending::Snapshot)) {
            let full = self.last_full.elapsed() >= SHARED_EVERY;
            if full || self.last_light.elapsed() >= SELECTION_EVERY {
                self.last_light = Instant::now();
                if full {
                    self.last_full = Instant::now();
                }
                bridge.request("snapshot", serde_json::json!({ "shared": full }), Pending::Snapshot);
            }
        }
        for (what, result) in replies {
            match (what, result) {
                (Pending::Hello, Ok(v)) => {
                    self.script_version = v.get("script").and_then(serde_json::Value::as_str).map(str::to_string);
                    self.studio_presets = v.get("presets").and_then(serde_json::Value::as_bool).unwrap_or(false);
                }
                (Pending::Hello, Err(_)) => {}
                (Pending::Snapshot, Ok(v)) => match serde_json::from_value::<Reply>(v) {
                    Ok(reply) => {
                        let snap = Snapshot::merge(self.snap.as_ref(), reply);
                        self.follow_selection(&snap);
                        // Development: screenshots of the Studio edit (ACL_SHOT_EDIT).
                        if self.screenshot.is_some()
                            && self.studio_edit.is_none()
                            && std::env::var_os("ACL_SHOT_EDIT").is_some()
                            && let Some(t) = snap.selection.first().cloned()
                            && let Some((prop, c)) = t.properties.iter().find_map(|i| i.curves.first().map(|c| (i.name.clone(), c.parameter_id.clone())))
                        {
                            self.begin_studio_edit(&t, &snap, &prop, &c);
                        }
                        self.snap = Some(snap);
                    }
                    Err(e) => self.set_status(format!("Couldn't read Studio's reply: {e}"), pal().bad),
                },
                (Pending::Snapshot, Err(e)) => self.set_status(format!("Studio couldn't list targets: {e}"), pal().bad),
                (Pending::Undo(ctx), Ok(_)) => {
                    self.last_apply.retain(|c| !same_place(c, &ctx));
                    self.set_status(format!("Undone: {} · {} is back as it was", ctx.target_label, ctx.property), pal().ok);
                    self.refresh_snapshot();
                }
                (Pending::Undo(ctx), Err(e)) => self.set_status(format!("Couldn't undo on {} · {}: {e}", ctx.target_label, ctx.property), pal().bad),
                (Pending::Apply(ctx), Ok(v)) => {
                    let created = v.get("created").and_then(serde_json::Value::as_bool).unwrap_or(false);
                    let replaced = v.get("replaced").and_then(serde_json::Value::as_u64).unwrap_or(0);
                    let added = v.get("addedParameter").and_then(serde_json::Value::as_bool).unwrap_or(false);
                    self.set_status(
                        format!(
                            "Applied '{}' to {} · {} on {}{}{}",
                            ctx.curve_name,
                            ctx.target_label,
                            ctx.property,
                            ctx.param_name,
                            if created { " (new curve)".to_string() } else { format!(" (replaced {replaced} points)") },
                            if added { "; the parameter was added to the event" } else { "" }
                        ),
                        pal().ok,
                    );
                    if let Some(e) = self.studio_edit.as_mut()
                        && e.ctx.target_id == ctx.target_id
                        && e.ctx.property_name == ctx.property_name
                        && e.ctx.param_id == ctx.param_id
                    {
                        e.applied = self.shelf.scratch.as_ref().and_then(|l| l.curves.first()).map(|c| c.data.points.clone()).unwrap_or_default();
                    }
                    let name = ctx.curve_name.clone();
                    self.last_apply.push(ctx);
                    if self.last_apply.len() > 1 {
                        self.set_status(format!("Applied '{name}' to {} targets", self.last_apply.len()), pal().ok);
                    }
                    self.refresh_snapshot();
                }
                (Pending::Presets { lib }, Ok(v)) => {
                    let presets: Vec<library::StudioPreset> = v.get("presets").cloned().and_then(|p| serde_json::from_value(p).ok()).unwrap_or_default();
                    let base = vec!["Studio presets".to_string()];
                    match library::import_presets(&lib, &base, &presets) {
                        Ok((0, 0)) if presets.is_empty() => self.set_status("This project has no curve presets yet", pal().dim),
                        Ok((made, updated)) => {
                            self.rescan();
                            self.reveal = true;
                            let name = self.shelf.library(&lib).map(|l| l.name().to_string()).unwrap_or_default();
                            self.set_status(
                                format!("Imported Studio's curve presets into {name} / Studio presets: {made} new, {updated} updated, {} unchanged", presets.len() - made - updated),
                                pal().ok,
                            );
                        }
                        Err(e) => self.set_status(format!("Couldn't import the presets: {e}"), pal().bad),
                    }
                }
                (Pending::Presets { .. }, Err(e)) => self.set_status(format!("Studio couldn't list its curve presets: {e}"), pal().bad),
                (Pending::WritePresets { count, folder }, Ok(v)) => {
                    let made = v.get("created").and_then(serde_json::Value::as_u64).unwrap_or(0);
                    let replaced = v.get("replaced").and_then(serde_json::Value::as_u64).unwrap_or(0);
                    self.set_status(format!("Sent {count} curve(s) to Studio's curve presets, in {folder}: {made} new, {replaced} replaced. Save the project in Studio to keep them."), pal().ok);
                }
                (Pending::WritePresets { folder, .. }, Err(e)) => self.set_status(format!("Couldn't write curve presets to {folder}: {e}"), pal().bad),
                (Pending::Apply(ctx), Err(e)) => self.set_status(format!("Couldn't apply to {} · {}: {e}", ctx.target_label, ctx.property), pal().bad),
            }
        }
    }

    /// Follows Studio's selection: whatever was newly selected becomes the
    /// target, and a hand-picked target gives way when the selection changes.
    fn follow_selection(&mut self, snap: &Snapshot) {
        let keys: Vec<String> = snap.selection.iter().map(Target::key).collect();
        let fresh = keys.iter().find(|k| !self.form.prev_selection.contains(k)).cloned();
        if let Some(k) = fresh {
            self.form.manual = false;
            self.set_target(snap, &k);
        } else if !self.form.manual
            && let Some(first) = keys.first()
            && !self.form.target.as_ref().is_some_and(|t| keys.contains(t))
        {
            let first = first.clone();
            self.set_target(snap, &first);
        }
        if keys != self.form.prev_selection {
            // Tick everything that shares a property with the first selected
            // thing; the odd ones out start unticked.
            let first = snap.selection.first();
            self.form.include = snap
                .selection
                .iter()
                .filter(|t| first.is_some_and(|f| common_target(snap, &[f.clone(), (*t).clone()]).is_some_and(|c| !c.properties.is_empty())))
                .map(Target::key)
                .collect();
        }
        self.form.prev_selection = keys;
    }

    fn set_target(&mut self, snap: &Snapshot, key: &str) {
        self.form.target = Some(key.to_string());
        let t = snap.all_targets().into_iter().find(|t| t.key() == key);
        self.form.property = t.and_then(|t| t.prefer.as_ref().map(|p| p.property.clone()));
        self.form.param = t.and_then(|t| t.prefer.as_ref().and_then(|p| p.parameter_id.clone()));
    }

    /// Puts the target back as it was before the last apply: the previous
    /// points (sent as raw positions and values over a 0..1 identity range), or
    /// removes the curve if the apply created it.
    fn undo_last_apply(&mut self) {
        for ctx in self.last_apply.clone() {
            self.undo_apply(ctx);
        }
    }

    fn undo_apply(&mut self, ctx: ApplyCtx) {
        let Some(b) = self.bridge.as_mut() else { return };
        let sent = match &ctx.previous {
            Some(prev) if prev.len() >= 2 => {
                let pts: Vec<Point> = prev.iter().map(|q| Point { x: q.position, y: q.value, shape: q.shape, s: q.s }).collect();
                let args = serde_json::json!({
                    "targetId": ctx.target_id, "label": ctx.target_label, "property": ctx.property_name, "parameterId": ctx.param_id,
                    "points": pts, "range": Range { plo: 0.0, phi: 1.0, vlo: 0.0, vhi: 1.0 },
                });
                b.request("apply", args, Pending::Undo(ctx.clone()))
            }
            Some(_) => {
                self.set_status("The previous curve had a single point; undo it in Studio instead", pal().warn);
                return;
            }
            None => {
                let args = serde_json::json!({ "targetId": ctx.target_id, "label": ctx.target_label, "property": ctx.property_name, "parameterId": ctx.param_id });
                b.request("removeCurve", args, Pending::Undo(ctx.clone()))
            }
        };
        if sent {
            self.set_status("Undoing…", pal().dim);
        }
    }

    fn send_apply(&mut self, ctx: ApplyCtx, points: Vec<Point>, r: Range) {
        let args = serde_json::json!({
            "targetId": ctx.target_id, "label": ctx.target_label, "property": ctx.property_name, "parameterId": ctx.param_id, "points": points, "range": r,
        });
        if let Some(b) = self.bridge.as_mut()
            && b.request("apply", args, Pending::Apply(ctx))
        {
            self.set_status("Applying…", pal().dim);
        }
    }

    /// Writes curves into Studio's curve presets, under "Curve Library /
    /// <library> / <folders>", so they're grouped as they are here.
    fn send_presets(&mut self, paths: &[PathBuf]) {
        let mut groups: Vec<(Vec<String>, Vec<serde_json::Value>)> = Vec::new();
        for p in paths {
            let Some((lib, c)) = self.shelf.find(p) else { continue };
            let mut folder = vec!["Curve Library".to_string(), lib.name().to_string()];
            folder.extend(c.folder.iter().cloned());
            let curve = serde_json::json!({ "name": c.data.name, "points": c.data.points });
            match groups.iter_mut().find(|(f, _)| *f == folder) {
                Some((_, list)) => list.push(curve),
                None => groups.push((folder, vec![curve])),
            }
        }
        let Some(b) = self.bridge.as_mut() else { return };
        for (folder, curves) in groups {
            let label = folder.join(" / ");
            let count = curves.len();
            b.request("writePresets", serde_json::json!({ "folder": folder, "curves": curves }), Pending::WritePresets { count, folder: label });
        }
        self.set_status("Sending to Studio's curve presets…", pal().dim);
    }

    /// The Studio edit, while it's the curve on screen.
    fn editing_studio(&self) -> Option<&StudioEdit> {
        self.studio_edit.as_ref().filter(|_| self.selected.as_deref().is_some_and(Shelf::is_scratch))
    }

    /// Opens a curve that's already in Studio for editing here. Its ranges
    /// are widened to hold every point, so nothing is clipped and applying
    /// it back changes only what you edit.
    fn begin_studio_edit(&mut self, target: &Target, snap: &Snapshot, prop: &str, param: &str) {
        let Some(c) = target.curve(prop, param) else { return };
        let sug = studio::suggest(target, prop, param, snap);
        let timeline = snap.param(param).is_some_and(|p| p.timeline);
        let mut r = if timeline { Range { plo: 0.0, phi: c.points.last().map_or(1.0, |q| q.position.max(1e-6)), ..sug.range } } else { sug.range };
        let up = r.vhi >= r.vlo;
        for q in &c.points {
            r.plo = r.plo.min(q.position);
            r.phi = r.phi.max(q.position);
            if up {
                r.vlo = r.vlo.min(q.value);
                r.vhi = r.vhi.max(q.value);
            }
        }
        if r.phi <= r.plo {
            r.phi = r.plo + 1.0;
        }
        if r.vhi == r.vlo {
            r.vhi = r.vlo + 1.0;
        }
        let info = target.property(prop);
        let prop_label = info.map_or_else(|| prop.to_string(), |i| if i.label.is_empty() { i.name.clone() } else { i.label.clone() });
        let pts = studio::normalise(&c.points, &r);
        let name = format!("{prop_label} on {}", c.parameter_name);
        let data = CurveData::new(&name, pts.clone(), &format!("From {} in Studio.", target.title()));
        let back_to = self.selected.clone().filter(|p| !Shelf::is_scratch(p));
        let path = self.shelf.set_scratch(data);
        self.studio_edit = Some(StudioEdit {
            ctx: ApplyCtx {
                curve_name: name,
                target_label: target.title(),
                property: info.map(|i| i.display()).unwrap_or(prop.to_string()),
                param_name: c.parameter_name.clone(),
                target_id: target.id.clone(),
                property_name: prop.to_string(),
                param_id: param.to_string(),
                previous: Some(c.points.clone()),
            },
            target_key: target.key(),
            range: r,
            x: Axis { lo: r.plo, hi: r.phi, unit: if timeline { "s".into() } else { String::new() }, title: c.parameter_name.clone() },
            y: Axis { lo: r.vlo, hi: r.vhi, unit: info.map(|i| short_unit(&i.unit).trim().to_string()).unwrap_or_default(), title: prop_label },
            applied: pts,
            back_to,
        });
        self.select(path);
        self.canvas.fit();
        self.set_status("Editing Studio's curve: Apply to Studio writes it back", pal().dim);
    }

    /// Writes the Studio edit back where it came from.
    fn apply_studio_edit(&mut self) {
        let Some(e) = self.studio_edit.clone() else { return };
        let Some(points) = self.shelf.scratch.as_ref().and_then(|l| l.curves.first()).map(|c| c.data.points.clone()) else { return };
        let mut ctx = e.ctx.clone();
        // What's in Studio now, for Undo last apply.
        if let Some(t) = self.snap.as_ref().and_then(|s| s.all_targets().into_iter().find(|t| t.key() == e.target_key).cloned()) {
            ctx.previous = t.curve(&ctx.property_name, &ctx.param_id).map(|c| c.points.clone());
        }
        self.last_apply.clear();
        self.send_apply(ctx, points, e.range);
    }

    fn end_studio_edit(&mut self) {
        let back = self.studio_edit.take().and_then(|e| e.back_to);
        self.shelf.scratch = None;
        self.undo.retain(|(p, _)| !Shelf::is_scratch(p));
        self.redo.retain(|(p, _)| !Shelf::is_scratch(p));
        self.selected = back.filter(|p| self.shelf.find(p).is_some());
        if self.selected.is_none() {
            self.rescan();
        }
        self.clamp_point();
        self.canvas.select_only(self.point);
    }

    /// The header above the canvas while editing a Studio curve.
    fn studio_edit_header(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(e) = self.studio_edit.clone() else { return };
        let now = self.shelf.scratch.as_ref().and_then(|l| l.curves.first()).map(|c| c.data.points.clone()).unwrap_or_default();
        let changed = now != e.applied;
        let linked = self.bridge.as_ref().is_some_and(Bridge::is_linked);
        let busy = self.bridge.as_ref().is_some_and(|b| b.is_waiting(|p| matches!(p, Pending::Apply(_))));
        ui.horizontal_wrapped(|ui| {
            ui.heading(format!("{} · {} on {}", e.ctx.target_label, e.y.title, e.ctx.param_name));
            ui.label(RichText::new("editing in Studio").color(pal().accent));
            if changed {
                ui.label(RichText::new("not applied yet").color(pal().warn));
            }
        });
        ui.horizontal_wrapped(|ui| {
            if primary(ui, linked && changed && !busy, "Apply to Studio")
                .on_hover_text("Writes your edits back onto this curve in Studio (Undo last apply in the Studio tab reverses it)")
                .clicked()
            {
                self.apply_studio_edit();
            }
            if ui.button("Save to library…").on_hover_text("Keeps a copy of this curve (0 to 1) in one of your libraries").clicked() {
                let description = format!(
                    "From {} · {} on {}. Values mapped from {} to {}.",
                    e.ctx.target_label, e.ctx.property, e.ctx.param_name, trim_num(e.range.vlo), trim_num(e.range.vhi)
                );
                actions.push(Action::SaveStudioEdit { points: now.clone(), name: e.ctx.curve_name.clone(), description });
            }
            let done = if changed { "Discard edits" } else { "Done" };
            if ui.button(done).on_hover_text("Stops editing and goes back to your library (Studio keeps what you last applied)").clicked() {
                self.end_studio_edit();
            }
        });
        ui.label(
            RichText::new(format!(
                "Shown in Studio's units: {} {} to {} {} over {} {} to {}. Not in a library until you save it.",
                e.y.title, trim_num(e.range.vlo), trim_num(e.range.vhi), e.y.unit, e.ctx.param_name, trim_num(e.range.plo), trim_num(e.range.phi)
            ))
            .color(pal().dim)
            .small(),
        );
    }

    /// Real-unit axes and the curve already in Studio, while the Studio tab
    /// has a target, so the canvas shows what Apply will produce.
    fn studio_overlay(&self) -> Overlay {
        if let Some(e) = self.editing_studio() {
            let ghost = if self.show_studio_curve {
                self.snap
                    .as_ref()
                    .and_then(|snap| snap.all_targets().into_iter().find(|t| t.key() == e.target_key).cloned())
                    .and_then(|t| t.curve(&e.ctx.property_name, &e.ctx.param_id).map(|c| studio::normalise(&c.points, &e.range)))
            } else {
                None
            };
            return Overlay { x: Some(e.x.clone()), y: Some(e.y.clone()), ghost };
        }
        if self.bridge.is_none() {
            return Overlay::default();
        }
        let Some(snap) = &self.snap else { return Overlay::default() };
        let targets = snap.all_targets();
        let Some(t) = self.form.target.as_ref().and_then(|k| targets.iter().find(|t| t.key() == *k)) else { return Overlay::default() };
        let (Some(prop), Some(param)) = (self.form.property.as_ref(), self.form.param.as_ref()) else { return Overlay::default() };
        let r = self.form.range;
        let info = t.property(prop);
        let p = snap.param(param);
        let x = Axis {
            lo: r.plo,
            hi: r.phi,
            unit: if p.is_some_and(|p| p.timeline) { "s".into() } else { String::new() },
            title: p.map_or_else(|| "Position".to_string(), |p| p.name.clone()),
        };
        let y = Axis {
            lo: r.vlo,
            hi: r.vhi,
            unit: info.map(|i| short_unit(&i.unit).trim().to_string()).unwrap_or_default(),
            title: info.map_or_else(|| prop.clone(), |i| if i.label.is_empty() { i.name.clone() } else { i.label.clone() }),
        };
        let ghost = if self.show_studio_curve { t.curve(prop, param).map(|c| studio::normalise(&c.points, &r)) } else { None };
        Overlay { x: Some(x), y: Some(y), ghost }
    }

    fn refresh_snapshot(&mut self) {
        self.last_light = Instant::now().checked_sub(SELECTION_EVERY).unwrap_or_else(Instant::now);
        self.last_full = Instant::now().checked_sub(SHARED_EVERY).unwrap_or_else(Instant::now);
    }

    fn studio_panel(&mut self, ui: &mut egui::Ui) {
        match self.bridge.as_ref().map(|b| b.state.clone()) {
            None => {
                ui.label(RichText::new("Not linked to Studio.").color(pal().warn));
                ui.label(
                    RichText::new("Open the Curve Library from FMOD Studio to link it: Scripts > DAT-audio Utilities > Open Automation Curve Library. Libraries work without a link.")
                        .color(pal().dim)
                        .small(),
                );
                return;
            }
            Some(LinkState::Connecting) => {
                ui.label(RichText::new("Connecting to Studio…").color(pal().dim));
                return;
            }
            Some(LinkState::Lost(why)) => {
                ui.label(RichText::new(format!("Link lost: {why}")).color(pal().bad));
            }
            Some(LinkState::Linked { studio, project }) => {
                let name = Path::new(&project).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(project.clone());
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                    ui.painter().rect_filled(dot, CornerRadius::same(2), pal().ok);
                    ui.label(RichText::new(format!("Linked · FMOD Studio {studio}")).color(pal().ok));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new("Refresh").small().wrap_mode(egui::TextWrapMode::Extend))
                            .on_hover_text("Read Studio's selection and project again now (it's also read every second)")
                            .clicked()
                        {
                            self.refresh_snapshot();
                        }
                    });
                });
                ui.add(egui::Label::new(RichText::new(name).color(pal().dim).small()).truncate());
            }
        }
        if let Some(v) = &self.script_version
            && parse_version(v) < MIN_SCRIPT
        {
            egui::Frame::group(ui.style()).fill(pal().banner).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("Studio is running an older script (v{v}).")).strong().color(pal().warn));
                ui.label(
                    RichText::new(format!(
                        "Choose Scripts > Reload in FMOD Studio (or restart it), then reopen the Curve Library. This app needs script v{}.{}.{} or newer; the old one can make Studio sluggish and mislabel what's selected.",
                        MIN_SCRIPT.0, MIN_SCRIPT.1, MIN_SCRIPT.2
                    ))
                    .small(),
                );
            });
        }
        let linked = self.bridge.as_ref().is_some_and(Bridge::is_linked);
        let Some(snap) = self.snap.clone() else {
            ui.label(RichText::new("Waiting for Studio…").color(pal().dim));
            return;
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            self.apply_form(ui, &snap, linked);
        });
    }

    /// What's selected in Studio: each thing with where it is and pills for
    /// what kind it is; with several, a tick box each (all ticked to start).
    fn selection_card(&mut self, ui: &mut egui::Ui, snap: &Snapshot, ticked: &[Target], common: Option<&Target>) {
        let n = snap.selection.len();
        let mut pulled: Option<(Target, String, String, bool)> = None;
        let mut save_attenuation: Option<studio::Spatial> = None;
        let title = if n > 1 { format!("Selected in Studio ({n})") } else { "Selected in Studio".to_string() };
        card_help(
            ui,
            &title,
            "Follows your selection in any Studio window: an effect, an instrument, a track, a bus, an event or a point on an automation curve. With several selected, Apply goes to every ticked one, and the lists below offer what they all have.",
            |ui| {
                if n == 0 {
                    ui.label(RichText::new("Nothing selected. Select an effect, an instrument, a track, a bus or an automation curve in Studio.").color(pal().warn));
                    return;
                }
                for t in &snap.selection {
                    let k = t.key();
                    let on = n == 1 || self.form.include.contains(&k);
                    ui.horizontal(|ui| {
                        if n > 1 {
                            let mut tick = on;
                            if ui.checkbox(&mut tick, "").on_hover_text("Apply to this one too").changed() {
                                if tick { self.form.include.insert(k.clone()) } else { self.form.include.remove(&k) };
                            }
                        }
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                let colour = if on { pal().strong } else { pal().dim };
                                // A shared effect's kind and use count are in its pill.
                                let name = if t.unique { t.title() } else { t.label.clone() };
                                ui.label(RichText::new(name).strong().color(colour));
                                if !t.unique {
                                    let uses = t.uses.map(|u| format!(" · {u} use{}", if u == 1 { "" } else { "s" })).unwrap_or_default();
                                    pill(ui, &format!("Shared effect{uses}"), pal().warn)
                                        .on_hover_text("A shared effect (an effect preset): every event and bus that uses it gets the curve.");
                                }
                                if let Some(c) = &t.selected_curve {
                                    pill(ui, &format!("curve: {c}"), pal().dim).on_hover_text("An automation curve is selected in Studio: its property and parameter are picked below.");
                                }
                            });
                            if t.unique && !t.place.is_empty() {
                                ui.label(RichText::new(&t.place).small().color(pal().dim));
                            }
                            // Its automation: pull a curve into the graph to edit and
                            // write back, or keep a copy in a library.
                            for info in &t.properties {
                                for c in &info.curves {
                                    let name = if info.label.is_empty() { info.name.clone() } else { info.label.clone() };
                                    ui.horizontal(|ui| {
                                        studio_thumb(ui, &c.points, 34.0, 16.0);
                                        ui.add(egui::Label::new(RichText::new(format!("{name} on {} · {} points", c.parameter_name, c.points.len())).small().color(pal().dim)).truncate());
                                    });
                                    ui.horizontal(|ui| {
                                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                        ui.add_space(38.0);
                                        let open = self.studio_edit.as_ref().is_some_and(|e| {
                                            e.target_key == t.key() && e.ctx.property_name == info.name && e.ctx.param_id == c.parameter_id
                                        });
                                        if ui
                                            .add_enabled(!open, egui::Button::new(if open { "In the graph" } else { "Pull into graph" }).small())
                                            .on_hover_text("Opens this curve from Studio in the graph, in Studio's units. Edit it, then Apply to Studio writes it back.")
                                            .clicked()
                                        {
                                            pulled = Some((t.clone(), info.name.clone(), c.parameter_id.clone(), false));
                                        }
                                        if ui.add(egui::Button::new("Save to library…").small()).on_hover_text("Keeps a copy of this curve (as 0 to 1) in one of your libraries").clicked() {
                                            pulled = Some((t.clone(), info.name.clone(), c.parameter_id.clone(), true));
                                        }
                                    });
                                }
                            }
                            if let Some(sp) = &t.spatial {
                                ui.add_space(2.0);
                                if attenuation_view(ui, sp) {
                                    save_attenuation = Some(sp.clone());
                                }
                            }
                        });
                    });
                }
                if ticked.is_empty() {
                    ui.label(RichText::new("Tick at least one to apply a curve.").color(pal().warn).small());
                } else if ticked.len() > 1 && common.is_none_or(|c| c.properties.is_empty()) {
                    ui.label(RichText::new("The ticked ones have no property in common. Untick the odd one out.").color(pal().warn).small());
                }
            },
        );
        if let Some(sp) = save_attenuation {
            let description = format!(
                "A spatializer's built-in {} attenuation (min {}, max {}), in dB. Apply to a Gain with the value range -80 to +10 dB, on Distance from 0 to {}.",
                sp.rolloff, trim_num(sp.min), trim_num(sp.max), trim_num(sp.max)
            );
            self.open_name("Save curve to library", &format!("{} attenuation", sp.rolloff), NameAction::SaveCapture { points: attenuation_curve(&sp), description });
        }
        match pulled {
            Some((t, prop, param, false)) => self.begin_studio_edit(&t, snap, &prop, &param),
            Some((t, prop, param, true)) => self.capture_to_library(&t, snap, &prop, &param),
            None => {}
        }
    }

    fn apply_form(&mut self, ui: &mut egui::Ui, snap: &Snapshot, linked: bool) {
        // Apply goes to the ticked selection (all of it, unless unticked); the
        // first ticked one supplies ranges, existing curves and the overlay.
        let one = snap.selection.len() == 1;
        let ticked: Vec<Target> = snap.selection.iter().filter(|t| one || self.form.include.contains(&t.key())).cloned().collect();
        let main_key = ticked.iter().map(Target::key).find(|k| self.form.target.as_ref() == Some(k)).or_else(|| ticked.first().map(Target::key));
        if main_key != self.form.target {
            match &main_key {
                Some(k) => self.set_target(snap, k),
                None => self.form.target = None,
            }
        }
        let common = common_target(snap, &ticked);
        self.selection_card(ui, snap, &ticked, common.as_ref());
        if let Some(target) = &common
            && !target.properties.is_empty()
        {
            self.apply_controls(ui, snap, target, &ticked, linked);
        }

        if self.studio_presets {
            card(ui, "FMOD 2.04 curve presets", |ui| {
                ui.label(
                    RichText::new("This project can keep curve presets. Right-click a curve, folder or library in the list > Send to Studio's curve presets, or bring the project's presets into a library:")
                        .color(pal().dim)
                        .small(),
                );
                let dests: Vec<(PathBuf, String)> = self.shelf.libraries.iter().filter(|l| !l.read_only()).map(|l| (l.dir.clone(), l.name().to_string())).collect();
                if dests.is_empty() {
                    ui.label(RichText::new("Make a library of your own first (New library).").color(pal().warn).small());
                } else {
                    self.preset_dest = self.preset_dest.min(dests.len() - 1);
                    ui.horizontal_wrapped(|ui| {
                        if dests.len() > 1 {
                            egui::ComboBox::from_id_salt("preset_dest").selected_text(dests[self.preset_dest].1.clone()).show_ui(ui, |ui| {
                                for (i, (_, n)) in dests.iter().enumerate() {
                                    ui.selectable_value(&mut self.preset_dest, i, n);
                                }
                            });
                        }
                        let lib = dests[self.preset_dest].0.clone();
                        if ui
                            .button(format!("Import Studio's presets into {}", dests[self.preset_dest].1))
                            .on_hover_text("Copies every curve preset in the project into a 'Studio presets' folder, keeping Studio's folders. Importing again updates them.")
                            .clicked()
                            && let Some(b) = self.bridge.as_mut()
                        {
                            b.request("presets", serde_json::json!({}), Pending::Presets { lib });
                            self.set_status("Reading Studio's curve presets…", pal().dim);
                        }
                    });
                }
            });
        }

    }

    fn apply_controls(&mut self, ui: &mut egui::Ui, snap: &Snapshot, target: &Target, ticked: &[Target], linked: bool) {
        let curve_name = self.current().map(|(_, c)| c.data.name.clone());
        let help = format!(
            "Curves are stored 0 to 1 and stretched over these ranges.\n\nPosition: the part of the parameter the curve spans; narrow it to cover only part.\nValues: the property's values that 0 and 1 map to (from {}).\n\nStudio doesn't tell scripts which knob you clicked, so choose the property here (for example Min Distance or Min Extent).",
            self.form.value_source
        );
        card_help(ui, "Apply curve to", &help, |ui| {
            egui::Grid::new("apply_form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend); // short row labels never split mid-word
                let props: Vec<String> = target.properties.iter().map(|p| p.name.clone()).collect();
                if self.form.property.as_ref().is_none_or(|p| !props.contains(p)) {
                    self.form.property = props.first().cloned();
                }
                let prop = self.form.property.clone().unwrap_or_default();
                let prop_label = |p: &str| match target.property(p) {
                    Some(i) if i.automated => format!("{}   · automated", i.display()),
                    Some(i) => i.display(),
                    None => p.to_string(),
                };
                ui.label("Property");
                egui::ComboBox::from_id_salt("prop")
                    .width((ui.available_width() - 8.0).clamp(140.0, 270.0))
                    .truncate()
                    .height(420.0)
                    .selected_text(if prop.is_empty() { "(none)".into() } else { prop_label(&prop) })
                    .show_ui(ui, |ui| {
                        for (automated, title) in [(true, "Automated"), (false, "Can be automated")] {
                            let group: Vec<&studio::PropertyInfo> = target.properties.iter().filter(|p| p.automated == automated).collect();
                            if group.is_empty() {
                                continue;
                            }
                            ui.label(RichText::new(title).small().strong().color(pal().dim));
                            for info in group {
                                ui.horizontal(|ui| {
                                    match info.curves.first() {
                                        Some(existing) => studio_thumb(ui, &existing.points, 30.0, 14.0),
                                        None => ui.allocate_exact_size(egui::vec2(30.0, 14.0), Sense::hover()).1,
                                    };
                                    if ui.selectable_label(info.name == prop, info.display()).clicked() {
                                        self.form.property = Some(info.name.clone());
                                        self.form.param = None;
                                    }
                                });
                            }
                        }
                    });
                ui.end_row();

                // Parameters: ones with a curve on this property first.
                let mut choices: Vec<(String, String)> = Vec::new();
                if let Some(info) = target.property(&prop) {
                    for c in &info.curves {
                        choices.push((c.parameter_id.clone(), c.parameter_name.clone()));
                    }
                }
                for r in &target.parameters {
                    if choices.iter().any(|(id, _)| *id == r.id) {
                        continue;
                    }
                    if let Some(p) = snap.param(&r.id) {
                        let note = if target.event_id.is_some() && !r.in_event && !p.timeline { "  (added to the event)" } else { "" };
                        choices.push((r.id.clone(), format!("{}{note}", p.name)));
                    }
                }
                if self.form.param.as_ref().is_none_or(|id| !choices.iter().any(|(c, _)| c == id)) {
                    self.form.param = choices.first().map(|c| c.0.clone());
                }
                let param = self.form.param.clone().unwrap_or_default();
                let param_label = |(id, name): &(String, String)| {
                    let range = snap
                        .param(id)
                        .map(|p| if p.timeline && p.estimated { "  (empty timeline)".to_string() } else { format!("  {}..{}", trim_num(p.min), trim_num(p.max)) })
                        .unwrap_or_default();
                    match target.curve(&prop, id) {
                        Some(c) => format!("{name}{range}  · has a {}-point curve", c.points.len()),
                        None => format!("{name}{range}"),
                    }
                };
                ui.label("Parameter");
                egui::ComboBox::from_id_salt("param")
                    .width((ui.available_width() - 8.0).clamp(140.0, 270.0))
                    .truncate()
                    .selected_text(choices.iter().find(|c| c.0 == param).map(param_label).unwrap_or_else(|| "(none available)".into()))
                    .show_ui(ui, |ui| {
                        for c in &choices {
                            ui.horizontal(|ui| {
                                match target.curve(&prop, &c.0) {
                                    Some(existing) => studio_thumb(ui, &existing.points, 30.0, 14.0),
                                    None => ui.allocate_exact_size(egui::vec2(30.0, 14.0), Sense::hover()).1,
                                };
                                if ui.selectable_label(c.0 == param, param_label(c)).clicked() {
                                    self.form.param = Some(c.0.clone());
                                }
                            });
                        }
                    });
                ui.end_row();

                let key = (target.key(), prop.clone(), param.clone());
                if key != self.form.filled_for {
                    let sug = studio::suggest(target, &prop, &param, snap);
                    self.form.range = sug.range;
                    self.form.value_source = sug.value_source;
                    self.form.value_known = sug.value_known;
                    self.form.position_estimated = sug.position_estimated;
                    self.form.filled_for = key;
                }
                let pos_unit = if snap.param(&param).is_some_and(|p| p.timeline) { " s".to_string() } else { String::new() };
                let val_unit = target.property(&prop).map(|i| short_unit(&i.unit)).unwrap_or_default();
                let r = &mut self.form.range;
                ui.label("Position range").on_hover_text("The part of the parameter the curve spans. Narrow it to cover only part of the parameter.");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut r.plo).speed(0.01).max_decimals(4).suffix(pos_unit.clone()));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut r.phi).speed(0.01).max_decimals(4).suffix(pos_unit));
                });
                ui.end_row();
                ui.label("Value range").on_hover_text("The property's values the curve's 0 and 1 map to.");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut r.vlo).speed(0.1).max_decimals(4).suffix(val_unit.clone()));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut r.vhi).speed(0.1).max_decimals(4).suffix(val_unit));
                });
                ui.end_row();
            });
            if self.form.position_estimated {
                ui.label(
                    RichText::new("This event's timeline is empty, so its length is a guess (10 s). Set the position range to the length you want the curve to span, in seconds.")
                        .color(pal().warn)
                        .small(),
                );
            }
            if !self.form.value_known {
                ui.label(RichText::new(format!("The property's range isn't known: the values come from {}. Check them.", self.form.value_source)).color(pal().warn).small());
            }
            ui.separator();
            let prop = self.form.property.clone().unwrap_or_default();
            let param = self.form.param.clone().unwrap_or_default();
            // What happens to curves already there, across everything ticked.
            let plan = apply_plan(snap, target, &ticked.iter().map(Target::key).collect(), &prop, &param);
            let replacing: Vec<usize> = plan.iter().filter_map(|(t, r)| r.as_ref().ok().and_then(|p| t.curve(&prop, p)).map(|c| c.points.len())).collect();
            let note = match (replacing.len(), plan.len()) {
                (0, _) => "Adds a new curve.".to_string(),
                (1, 1) => format!("Replaces the existing {}-point curve.", replacing[0]),
                (r, n) if r == n => format!("Replaces the existing curve on all {n}."),
                (r, n) => format!("Replaces the existing curve on {r} of {n}; adds one to the rest."),
            };
            ui.label(RichText::new(note).color(if replacing.is_empty() { pal().dim } else { pal().warn }).small());
            let r = self.form.range;
            let valid = r.phi > r.plo && r.vhi != r.vlo && !prop.is_empty() && !param.is_empty();
            let busy = self.bridge.as_ref().is_some_and(|b| b.is_waiting(|p| matches!(p, Pending::Apply(_))));
            let points = self.current().map(|(_, c)| c.data.points.clone());
            let prop_label = target.property(&prop).map(|i| if i.label.is_empty() { i.name.clone() } else { i.label.clone() }).unwrap_or(prop.clone());
            let param_label = snap.param(&param).map(|p| p.name.clone()).or_else(|| target.curve(&prop, &param).map(|c| c.parameter_name.clone())).unwrap_or_default();
            let ready: Vec<&(Target, Result<String, String>)> = plan.iter().filter(|(_, r)| r.is_ok()).collect();
            let several = plan.len() > 1;
            let button = match &curve_name {
                Some(n) if several => format!("Apply '{n}' to {prop_label} on {param_label} ({} items)", ready.len()),
                Some(n) => format!("Apply '{n}' to {prop_label} on {param_label}"),
                None => "Select a curve to apply".into(),
            };
            ui.add_space(4.0);
            if primary(ui, linked && valid && points.is_some() && !busy && !ready.is_empty(), button)
                .on_hover_text("Writes this curve into Studio, replacing any curve on this property and parameter")
                .clicked()
                && let (Some(points), Some(name)) = (points, curve_name)
            {
                self.last_apply.clear();
                for (t, param) in ready.iter().filter_map(|(t, r)| r.as_ref().ok().map(|p| (t, p))) {
                    let param_name = snap.param(param).map(|p| p.name.clone()).or_else(|| t.curve(&prop, param).map(|c| c.parameter_name.clone())).unwrap_or_default();
                    let ctx = ApplyCtx {
                        curve_name: name.clone(),
                        target_label: t.title(),
                        property: t.property(&prop).map(|i| i.display()).unwrap_or(prop.clone()),
                        param_name,
                        target_id: t.id.clone(),
                        property_name: prop.clone(),
                        param_id: param.clone(),
                        previous: t.curve(&prop, param).map(|c| c.points.clone()),
                    };
                    self.send_apply(ctx, points.clone(), r);
                }
            }
            // The lists only offer what every ticked item has, so nothing should
            // be left out; say so if something is.
            for (t, r) in &plan {
                if let Err(why) = r {
                    ui.label(RichText::new(format!("Left out: {} {why}", t.title())).small().color(pal().warn));
                }
            }
            if !valid {
                ui.label(RichText::new("Pick a property and a parameter, and give both ranges a non-zero width.").color(pal().warn).small());
            }
            if let Some(last) = self.last_apply.first().cloned() {
                let n = self.last_apply.len();
                let what = if n == 1 { format!("{} · {}", last.target_label, last.property) } else { format!("{} targets · {}", n, last.property) };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Last apply: '{}' to {what}", last.curve_name)).small().color(pal().dim));
                    if ui.button("Undo last apply").on_hover_text("Puts that curve back as it was before (or removes it, if the apply created it)").clicked() {
                        self.undo_last_apply();
                    }
                });
            }
        });

    }

    /// Saves a curve that's in Studio into a library (asks for a name),
    /// mapped to 0-1 over the property's and parameter's ranges.
    fn capture_to_library(&mut self, target: &Target, snap: &Snapshot, prop: &str, param: &str) {
        let (prop, param) = (prop.to_string(), param.to_string());
        let sug = studio::suggest(target, &prop, &param, snap);
        if let Some(c) = target.curve(&prop, &param) {
            let range = if snap.param(&param).is_some_and(|p| p.timeline) {
                // Timeline curves: span exactly the curve's points.
                Range { plo: 0.0, phi: c.points.last().map_or(1.0, |q| q.position.max(1e-6)), ..sug.range }
            } else {
                sug.range
            };
            let mut pts = studio::normalise(&c.points, &range);
            for p in &mut pts {
                p.x = p.x.clamp(0.0, 1.0);
                p.y = p.y.clamp(0.0, 1.0);
            }
            let label = target.property(&prop).map(|i| i.display()).unwrap_or(prop.clone());
            let description = format!(
                "From {} · {label} on {}. Values mapped from {} to {} ({}).",
                target.title(), c.parameter_name, trim_num(range.vlo), trim_num(range.vhi), sug.value_source
            );
            let base = target.label.trim_matches('\'').to_string();
            self.open_name("Save curve to library", &format!("{base} {prop}"), NameAction::SaveCapture { points: pts, description });
        }
    }

    // ── Dialogs and keys ────────────────────────────────────────────────────

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut d) = self.name_dialog.take() {
            let mut keep = true;
            let (mut commit, mut keep_default) = (false, false);
            let dests = self.shelf.destinations();
            let modal = egui::Modal::new(egui::Id::new("name_dialog")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new(&d.title).heading());
                if !d.note.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&d.note).small()).wrap());
                }
                let out = egui::TextEdit::singleline(&mut d.value).desired_width(320.0).show(ui);
                if !d.focused {
                    // Select the whole name, so typing replaces it.
                    out.response.request_focus();
                    let mut state = out.state.clone();
                    let len = d.value.chars().count();
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len))));
                    state.store(ui.ctx(), out.response.id);
                    d.focused = true;
                }
                if out.response.lost_focus() && ui.ctx().input(|i| i.key_pressed(Key::Enter)) {
                    commit = true;
                }
                if matches!(d.action, NameAction::NewCurve) {
                    let t = templates();
                    egui::ComboBox::from_label("Start from").selected_text(t.get(d.template).map_or("", |x| x.0)).show_ui(ui, |ui| {
                        for (i, (name, _)) in t.iter().enumerate() {
                            ui.selectable_value(&mut d.template, i, *name);
                        }
                    });
                }
                if matches!(d.action, NameAction::NewCurve | NameAction::SaveCapture { .. }) {
                    egui::ComboBox::from_label("Save in")
                        .selected_text(dests.get(d.dest).map_or("(no library of yours)".to_string(), |x| x.2.clone()))
                        .show_ui(ui, |ui| {
                            for (i, (_, _, label)) in dests.iter().enumerate() {
                                ui.selectable_value(&mut d.dest, i, label);
                            }
                        });
                }
                if !d.error.is_empty() {
                    ui.label(RichText::new(&d.error).color(pal().bad).small());
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let ok = if matches!(d.action, NameAction::NameDefault { .. }) { "Rename" } else { "OK" };
                    if primary(ui, true, ok).clicked() {
                        commit = true;
                    }
                    if matches!(d.action, NameAction::NameDefault { .. }) && ui.button("Keep 'My Curves'").clicked() {
                        commit = true;
                        keep_default = true;
                    }
                    if ui.button("Cancel").on_hover_text("Esc").clicked() {
                        keep = false;
                    }
                });
            });
            if modal.should_close() {
                keep = false;
            }
            if commit {
                match self.commit_name(&d, keep_default) {
                    Ok(_) => keep = false,
                    Err(e) => d.error = e,
                }
            }
            if keep && self.name_dialog.is_none() {
                self.name_dialog = Some(d);
            }
        }
        if let Some((what, text)) = self.confirm.clone() {
            let enter = ctx.input(|i| i.key_pressed(Key::Enter));
            let modal = egui::Modal::new(egui::Id::new("confirm_delete")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new("Delete?").heading());
                ui.label(&text);
                ui.label(RichText::new("This can't be undone from here.").color(pal().warn).small());
                ui.horizontal(|ui| {
                    if danger(ui, "Delete").on_hover_text("Enter").clicked() || enter {
                        let res = match &what {
                            DeleteWhat::Curve(p) => library::delete_curve(&self.root, p),
                            DeleteWhat::Folder(l, f) => library::delete_folder(l, f),
                            DeleteWhat::Library(d) => library::delete_library(d),
                        };
                        self.confirm = None;
                        match res {
                            Ok(()) => {
                                let gone = |p: &PathBuf| match &what {
                                    DeleteWhat::Curve(c) => p == c,
                                    DeleteWhat::Folder(l, f) => p.starts_with(f.iter().fold(l.clone(), |d, x| d.join(x))),
                                    DeleteWhat::Library(d) => p.starts_with(d),
                                };
                                self.unsaved.retain(|p| !gone(p));
                                self.rescan();
                                self.set_status("Deleted", pal().dim);
                            }
                            Err(e) => self.set_status(e, pal().bad),
                        }
                    }
                    if ui.button("Cancel").on_hover_text("Esc").clicked() {
                        self.confirm = None;
                    }
                });
            });
            if modal.should_close() {
                self.confirm = None;
            }
        }
        if self.quit_prompt {
            let modal = egui::Modal::new(egui::Id::new("quit_prompt")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new("Unsaved changes").heading());
                ui.label(format!("{} curve(s) have unsaved changes.", self.unsaved.len()));
                ui.horizontal(|ui| {
                    if primary(ui, true, "Save and close").clicked() {
                        self.run(Action::SaveAll);
                        self.quit_prompt = false;
                        if self.unsaved.is_empty() && self.name_dialog.is_none() {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    if ui.button("Close without saving").clicked() {
                        self.quit_prompt = false;
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Cancel").on_hover_text("Esc").clicked() {
                        self.quit_prompt = false;
                    }
                });
            });
            if modal.should_close() {
                self.quit_prompt = false;
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.name_dialog.is_some() || self.confirm.is_some() {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            self.show_guide = !self.show_guide;
        }
        let save = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
        if ctx.input_mut(|i| i.consume_shortcut(&save)) {
            self.run(Action::SaveAll);
        }
        // Point shortcuts only act on the canvas (pointer over it, or focused),
        // so Delete or the arrows elsewhere never change a curve. Tab is left
        // to egui's normal keyboard navigation; [ and ] step between points.
        // Undo/redo work anywhere except while typing in a text field.
        let typing = ctx.memory(|m| m.focused().is_some()) && !self.canvas.active;
        if !typing {
            let undo = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
            let redo = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
            if ctx.input_mut(|i| i.consume_shortcut(&redo)) {
                self.step_history(false);
            } else if ctx.input_mut(|i| i.consume_shortcut(&undo)) {
                self.step_history(true);
            }
        }
        if typing || !self.canvas.active {
            return;
        }
        let all = KeyboardShortcut::new(Modifiers::COMMAND, Key::A);
        let Some(path) = self.selected.clone() else { return };
        let Some(before) = self.shelf.find(&path).map(|(_, c)| c.data.points.clone()) else { return };
        let n = before.len();
        if ctx.input_mut(|i| i.consume_shortcut(&all)) {
            self.canvas.selection = (0..n).collect();
        }
        let Some(sel) = self.point.filter(|s| *s < n) else { return };
        let (del, left, right, up, down, prev, next, shift) = ctx.input(|i| {
            (
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::OpenBracket),
                i.key_pressed(Key::CloseBracket),
                i.modifiers.shift,
            )
        });
        if (prev || next) && n > 0 {
            self.point = Some(if prev { (sel + n - 1) % n } else { (sel + 1) % n });
            self.canvas.select_only(self.point);
        }
        if !self.current_editable() {
            return;
        }
        let group: BTreeSet<usize> = if self.canvas.selection.is_empty() { [sel].into() } else { self.canvas.selection.clone() };
        if del && n > 2 {
            let mut pts = before.clone();
            delete_selected(&mut pts, &group);
            self.push_undo(path.clone(), before);
            self.point = Some(sel.min(pts.len() - 1));
            self.canvas.select_only(self.point);
            self.set_points(&path, pts);
            return;
        }
        let step = if shift { 0.001 } else { 0.01 };
        let dx = if right { step } else if left { -step } else { 0.0 };
        let dy = if up { step } else if down { -step } else { 0.0 };
        if dx != 0.0 || dy != 0.0 {
            let mut pts = before.clone();
            let start: Vec<(usize, f64, f64)> = group.iter().filter(|&&i| i < n).map(|&i| (i, pts[i].x, pts[i].y)).collect();
            move_group(&mut pts, &start, dx, dy);
            self.push_undo(path.clone(), before);
            self.set_points(&path, pts);
        }
    }

    fn dev_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        // In bridge mode, wait for Studio's first snapshot so it's in the picture.
        if self.bridge.is_some() && self.snap.is_none() {
            ctx.request_repaint();
            return;
        }
        self.frames += 1;
        ctx.request_repaint();
        if self.frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Err(e) = crate::png::write_rgba(&path, image.size[0] as u32, image.size[1] as u32, &rgba) {
                eprintln!("screenshot failed: {e}");
            }
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let pointer_down = ctx.input(|i| i.pointer.any_down());

        // Closing with unsaved edits asks first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.unsaved.is_empty() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_prompt = true;
        }

        // Pick up libraries added or changed outside the app.
        if self.last_watch.elapsed() >= WATCH_EVERY {
            self.last_watch = Instant::now();
            if !pointer_down && library::stamp(&self.root) != self.last_stamp {
                self.rescan();
                self.set_status("Libraries changed on disk and were reloaded", pal().dim);
            }
        }
        // Wake up for the watcher and the Studio poll; otherwise only on input.
        ctx.request_repaint_after(if self.bridge.is_some() { SELECTION_EVERY } else { WATCH_EVERY });
        self.shortcuts(&ctx);
        self.link_tick();

        let mut actions = Vec::new();
        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                let n = self.unsaved.len();
                if n > 0 {
                    if primary(ui, true, format!("Save {n} change{}", if n == 1 { "" } else { "s" })).on_hover_text(for_platform("Save edited curves to their files (Cmd+S)")).clicked() {
                        actions.push(Action::SaveAll);
                    }
                } else {
                    ui.label(RichText::new("All saved").color(pal().dim));
                }
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).clicked() {
                    self.step_history(true);
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).clicked() {
                    self.step_history(false);
                }
                // Only does something once you've zoomed or panned, so it's greyed out until then.
                let zoomed = self.canvas.is_zoomed();
                if ui
                    .add_enabled(zoomed, egui::Button::new("Reset zoom"))
                    .on_hover_text("Zoom and pan back out to the whole curve (0 to 1 on both axes)")
                    .on_disabled_hover_text(for_platform(
                        "Showing the whole curve. Pinch or Cmd+scroll to zoom; two-finger scroll, Space+drag or Alt+drag to pan",
                    ))
                    .clicked()
                {
                    self.canvas.fit();
                }
                ui.checkbox(&mut self.auto_fit, "Auto")
                    .on_hover_text("Zoom back out to the whole curve 2 seconds after the pointer leaves the graph");
                ui.separator();
                ui.checkbox(&mut self.show_studio_curve, "Show Studio's curve").on_hover_text(
                    "While the Studio tab has a target that already has a curve, draw it (dashed, with its points) behind yours",
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new("Guide").strong()).on_hover_text("How to use the Curve Library (F1)").clicked() {
                        self.show_guide = !self.show_guide;
                    }
                    let mut look = self.look;
                    egui::ComboBox::from_id_salt("look")
                        .selected_text(look.name())
                        .show_ui(ui, |ui| {
                            for l in Look::ALL {
                                ui.selectable_value(&mut look, l, l.name());
                            }
                        })
                        .response
                        .on_hover_text("The app's look: DAT Audio (like the Network Audio Grabber) or Classic");
                    if look != self.look {
                        self.look = look;
                        theme::apply(&ctx, look);
                    }
                });
            });
        });
        // Hints for whatever is under the pointer.
        let hover = ctx.input(|i| i.pointer.hover_pos());
        let hint = if self.canvas.active {
            "Drag points · drag a box to select · drag a diamond to bend · double-click to add/remove · right-click for shapes · \
             Shift fine · Cmd snap · arrows nudge · [ ] previous/next point · Cmd+A all · pan: scroll, Space/Alt-drag · zoom: pinch or Cmd+scroll"
        } else if hover.is_some_and(|p| self.tree_rect.contains(p)) {
            "Click a curve to open it · drag to reorder or move it · drag a Factory curve into your library to copy it · right-click for more"
        } else {
            "Press F1 for the guide · Cmd+S saves · Cmd+Z undoes"
        };
        // The bottom bar: the latest message for a few seconds (errors longer),
        // then hints for whatever is under the pointer. Messages used to float
        // over the graph and hid its axis title.
        let message = self.toast.clone().filter(|(_, color, at)| at.elapsed().as_secs_f32() < if *color == pal().bad { 8.0 } else { 4.0 });
        if message.is_none() {
            self.toast = None;
        }
        egui::Panel::bottom("status").show(ui, |ui| match &message {
            Some((text, color, _)) => {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                    ui.painter().rect_filled(rect, CornerRadius::same(2), *color);
                    ui.add(egui::Label::new(RichText::new(text).color(*color)).wrap());
                });
                ctx.request_repaint_after(Duration::from_millis(250));
            }
            None => {
                ui.add(egui::Label::new(RichText::new(for_platform(hint)).color(pal().dim).small()).truncate());
            }
        });
        // Side panels stay within limits so the graph always keeps at least
        // MIN_GRAPH_WIDTH, and long text wraps or truncates instead of
        // widening a panel (a long Studio label used to push the inspector
        // wider and hide part of the graph).
        let win_w = ctx.content_rect().width();
        let left_max = (win_w * 0.28).max(LEFT_MIN);
        let right_max = (win_w - left_max - MIN_GRAPH_WIDTH).clamp(RIGHT_MIN, win_w * 0.45);
        let side_frame = egui::Frame::side_top_panel(ui.style()).fill(pal().side);
        let left = egui::Panel::left("libraries").frame(side_frame).resizable(true).default_size(260.0).size_range(LEFT_MIN..=left_max).show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            self.library_panel(ui, &mut actions)
        });
        self.tree_rect = left.response.rect;
        egui::Panel::right("studio").frame(side_frame).resizable(true).default_size(460.0).size_range(RIGHT_MIN..=right_max).show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.heading("FMOD Studio");
            self.studio_panel(ui);
        });
        self.canvas.overlay = self.studio_overlay();
        if self.auto_fit {
            self.canvas.auto_fit(&ctx, Duration::from_secs(2));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            if self.current().is_some() {
                egui::Panel::bottom("points_strip").frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6))).show(ui, |ui| self.points_strip(ui));
            }
            self.centre(ui, &mut actions)
        });
        for a in actions {
            self.run(a);
        }
        self.dialogs(&ctx);
        if self.show_guide {
            egui::Window::new("Curve Library guide")
                .open(&mut self.show_guide)
                .default_size([560.0, 640.0])
                .resizable(true)
                .collapsible(false)
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| guide_ui(ui, &for_platform(GUIDE)));
                });
        }

        // The title only when it changes: sending it every frame makes the
        // window system wake the app again, which kept 0.4.0 busy (~36% CPU).
        let title = match self.current() {
            Some((l, c)) => format!("{}Curve Library — {} / {}", if self.unsaved.is_empty() { "" } else { "• " }, l.name(), c.data.name),
            None => "Curve Library".to_string(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        self.dev_screenshot(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(p) = &self.selected {
            storage.set_string(STORAGE_SELECTED, p.display().to_string());
        }
        storage.set_string(STORAGE_FAVOURITES, serde_json::to_string(&self.favourites).unwrap_or_default());
        storage.set_string(STORAGE_GUIDE_SEEN, "1".into());
        storage.set_string(STORAGE_SHOW_STUDIO_CURVE, if self.show_studio_curve { "1" } else { "0" }.into());
        storage.set_string(STORAGE_LOOK, self.look.key().into());
        storage.set_string(STORAGE_AUTO_FIT, if self.auto_fit { "1" } else { "0" }.into());
    }
}

/// Draws the guide's small Markdown subset: `#`/`##` headings, `-` bullets,
/// `1.` steps (indented `-` under them), and paragraphs. `**` and backticks
/// are dropped.
fn guide_ui(ui: &mut egui::Ui, text: &str) {
    let clean = |s: &str| s.replace("**", "").replace('`', "");
    for line in text.lines() {
        let t = line.trim_end();
        let indent = (t.len() - t.trim_start().len()) as f32 * 6.0;
        let body = t.trim_start();
        if let Some(h) = body.strip_prefix("# ") {
            ui.label(RichText::new(clean(h)).heading().color(pal().accent));
        } else if let Some(h) = body.strip_prefix("## ") {
            ui.add_space(6.0);
            ui.label(RichText::new(clean(h)).strong().size(16.0));
        } else if body.is_empty() {
            ui.add_space(4.0);
        } else {
            let (marker, rest) = if let Some(r) = body.strip_prefix("- ") {
                ("•".to_string(), r)
            } else if let Some(dot) = body.find(". ").filter(|&i| i <= 2 && body[..i].chars().all(|c| c.is_ascii_digit())) {
                (format!("{}.", &body[..dot]), &body[dot + 2..])
            } else {
                (String::new(), body)
            };
            ui.horizontal_wrapped(|ui| {
                ui.add_space(indent);
                if !marker.is_empty() {
                    ui.label(RichText::new(marker).color(pal().dim));
                }
                ui.add(egui::Label::new(clean(rest)).wrap());
            });
        }
    }
}

fn remap_action(a: &mut Action, old: &Path, new: &Path) {
    let fix = |p: &mut PathBuf| {
        if let Ok(rest) = p.strip_prefix(old) {
            *p = new.join(rest);
        }
    };
    match a {
        Action::CreateCurve { lib, .. } | Action::CopyTo { lib, .. } => fix(lib),
        Action::Drop { src, lib, before, .. } => {
            fix(src);
            fix(lib);
            if let Some(b) = before {
                fix(b);
            }
        }
        Action::MoveTo { path, lib, .. } => {
            fix(path);
            fix(lib);
        }
        Action::Duplicate(p) | Action::Select(p) => fix(p),
        _ => {}
    }
}

// ── Library tree (drawn from a borrow of the shelf) ─────────────────────────

struct Tree<'a> {
    selected: Option<&'a Path>,
    /// Linked to a Studio with 2.04 curve presets: offer "Send to Studio's curve presets".
    presets: bool,
    unsaved: &'a BTreeSet<PathBuf>,
    dests: &'a [(PathBuf, Vec<String>, String)],
    reveal: bool,
    favourites: &'a BTreeSet<String>,
    /// Fold every folder this frame.
    collapse: bool,
}

impl Tree<'_> {
    fn dot(&self, p: &Path) -> &'static str {
        if self.unsaved.contains(p) { "  •" } else { "" }
    }

    fn folder(&self, ui: &mut egui::Ui, lib: &Library, folder: &[String], actions: &mut Vec<Action>) {
        for sub in lib.subfolders(folder) {
            let name = sub.last().cloned().unwrap_or_default();
            let holds = self.selected.is_some_and(|s| s.starts_with(lib.folder_dir(sub)));
            let r = egui::CollapsingHeader::new(name)
                .id_salt(("folder", &lib.dir, sub.join("/")))
                .default_open(!lib.read_only()) // Factory starts folded so your libraries stay in view
                .open(if self.reveal && holds { Some(true) } else if self.collapse { Some(false) } else { None })
                .show(ui, |ui| self.folder(ui, lib, sub, actions));
            self.drop_target(&r.header_response, lib, sub, None, actions);
            r.header_response.context_menu(|ui| {
                if lib.read_only() {
                    ui.label(RichText::new("Read-only").small());
                } else {
                    if ui.button("New curve here…").clicked() {
                        actions.push(Action::NewCurve { lib: lib.dir.clone(), folder: sub.clone() });
                        ui.close();
                    }
                    if ui.button("New subfolder…").clicked() {
                        actions.push(Action::NewFolder { lib: lib.dir.clone(), parent: sub.clone() });
                        ui.close();
                    }
                    if ui.button("Rename folder…").clicked() {
                        actions.push(Action::RenameFolder { lib: lib.dir.clone(), folder: sub.clone() });
                        ui.close();
                    }
                }
                if ui.button(reveal_label()).clicked() {
                    actions.push(Action::Reveal(lib.folder_dir(sub)));
                    ui.close();
                }
                if self.presets && ui.button("Send folder to Studio's curve presets").on_hover_text(PRESET_HELP).clicked() {
                    let all = lib.curves.iter().filter(|c| c.folder.starts_with(sub)).map(|c| c.path.clone()).collect();
                    actions.push(Action::SendPresets(all));
                    ui.close();
                }
                if !lib.read_only() {
                    ui.separator();
                    if ui.button("Delete folder…").clicked() {
                        actions.push(Action::Delete(DeleteWhat::Folder(lib.dir.clone(), sub.clone())));
                        ui.close();
                    }
                }
            });
        }
        if folder.is_empty() && lib.curves.is_empty() && !lib.read_only() {
            ui.add(
                egui::Label::new(RichText::new("Empty. Right-click the library > New curve, or drag a Factory curve here.").small().color(pal().dim)).wrap(),
            );
        }
        for c in lib.curves_in(folder) {
            let is_sel = self.selected == Some(c.path.as_path());
            let (r, star) = curve_row(
                ui,
                c,
                egui::Button::selectable(is_sel, format!("{}{}", c.data.name, self.dot(&c.path))).sense(Sense::click_and_drag()),
                self.favourites.contains(&c.data.id),
            );
            if star {
                actions.push(Action::ToggleFavourite(c.data.id.clone()));
            }
            r.dnd_set_drag_payload(DragCurve(c.path.clone()));
            self.drop_target(&r, lib, &c.folder, Some(&c.path), actions);
            if is_sel && self.reveal {
                r.scroll_to_me(Some(egui::Align::Center));
            }
            if r.clicked() {
                actions.push(Action::Select(c.path.clone()));
            }
            self.curve_menu(&r, lib, c, actions);
        }
    }

    /// Makes `r` a drop target for dragged curves: on a curve row, the drop
    /// goes before that curve; on a folder or library header, into it (last).
    /// Read-only libraries accept nothing.
    fn drop_target(&self, r: &egui::Response, lib: &Library, folder: &[String], before: Option<&Path>, actions: &mut Vec<Action>) {
        if lib.read_only() {
            return;
        }
        if r.dnd_hover_payload::<DragCurve>().is_some() {
            let painter = r.ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dnd-line")));
            let rect = r.rect;
            if before.is_some() {
                painter.hline(rect.x_range(), rect.top(), egui::Stroke::new(2.0, pal().accent));
            } else {
                painter.rect_stroke(rect, CornerRadius::same(4), egui::Stroke::new(2.0, pal().accent), egui::StrokeKind::Inside);
            }
        }
        if let Some(p) = r.dnd_release_payload::<DragCurve>() {
            actions.push(Action::Drop { src: p.0.clone(), lib: lib.dir.clone(), folder: folder.to_vec(), before: before.map(Path::to_path_buf) });
        }
    }

    fn curve_menu(&self, r: &egui::Response, lib: &Library, c: &library::Curve, actions: &mut Vec<Action>) {
        r.context_menu(|ui| {
            let fav = self.favourites.contains(&c.data.id);
            if ui.button(if fav { "Remove from favourites" } else { "Add to favourites" }).clicked() {
                actions.push(Action::ToggleFavourite(c.data.id.clone()));
                ui.close();
            }
            if self.presets && ui.button("Send to Studio's curve presets").on_hover_text(PRESET_HELP).clicked() {
                actions.push(Action::SendPresets(vec![c.path.clone()]));
                ui.close();
            }
            ui.separator();
            if !lib.read_only() {
                if ui.button("Rename…").clicked() {
                    actions.push(Action::RenameCurve(c.path.clone()));
                    ui.close();
                }
                if ui.button("Duplicate").clicked() {
                    actions.push(Action::Duplicate(c.path.clone()));
                    ui.close();
                }
                // Reordering without dragging.
                let siblings: Vec<PathBuf> = lib.curves_in(&c.folder).iter().map(|x| x.path.clone()).collect();
                let at = siblings.iter().position(|p| *p == c.path).unwrap_or(0);
                if ui.add_enabled(at > 0, egui::Button::new("Move up")).clicked() {
                    actions.push(Action::Drop { src: c.path.clone(), lib: lib.dir.clone(), folder: c.folder.clone(), before: siblings.get(at - 1).cloned() });
                    ui.close();
                }
                if ui.add_enabled(at + 1 < siblings.len(), egui::Button::new("Move down")).clicked() {
                    actions.push(Action::Drop { src: c.path.clone(), lib: lib.dir.clone(), folder: c.folder.clone(), before: siblings.get(at + 2).cloned() });
                    ui.close();
                }
                ui.menu_button("Move to", |ui| {
                    for (l, f, label) in self.dests {
                        if ui.button(label).clicked() {
                            actions.push(Action::MoveTo { path: c.path.clone(), lib: l.clone(), folder: f.clone() });
                            ui.close();
                        }
                    }
                });
            }
            ui.menu_button("Copy to", |ui| {
                if self.dests.is_empty() {
                    ui.label(RichText::new("Make a library of your own first").small());
                }
                for (l, f, label) in self.dests {
                    if ui.button(label).clicked() {
                        actions.push(Action::CopyTo { path: c.path.clone(), lib: l.clone(), folder: f.clone() });
                        ui.close();
                    }
                }
            });
            if ui.button(reveal_label()).clicked() {
                actions.push(Action::Reveal(c.path.clone()));
                ui.close();
            }
            if !lib.read_only() {
                ui.separator();
                if ui.button("Delete curve…").clicked() {
                    actions.push(Action::Delete(DeleteWhat::Curve(c.path.clone())));
                    ui.close();
                }
            }
        });
    }
}

/// The first ticked target, narrowed to the properties and parameters every
/// ticked one has (an event's own timeline counts as another's timeline), so
/// whatever the lists offer applies to all of them.
fn common_target(snap: &Snapshot, ticked: &[Target]) -> Option<Target> {
    let (first, rest) = ticked.split_first()?;
    let mut t = first.clone();
    t.properties.retain(|p| rest.iter().all(|o| o.property(&p.name).is_some()));
    let is_timeline = |id: &str| snap.param(id).is_some_and(|p| p.timeline);
    t.parameters.retain(|r| {
        rest.iter().all(|o| if is_timeline(&r.id) { o.parameters.iter().any(|q| is_timeline(&q.id)) } else { o.parameters.iter().any(|q| q.id == r.id) })
    });
    Some(t)
}

/// Whether two applies wrote to the same curve.
fn same_place(a: &ApplyCtx, b: &ApplyCtx) -> bool {
    a.target_id == b.target_id && a.property_name == b.property_name && a.param_id == b.param_id
}

/// Where an apply goes: the main target, then every other ticked selection,
/// each with the parameter it would use (an event's own timeline stands in
/// for another event's) or why it's skipped.
fn apply_plan(snap: &Snapshot, main: &Target, include: &BTreeSet<String>, prop: &str, param: &str) -> Vec<(Target, Result<String, String>)> {
    let timeline = snap.param(param).is_some_and(|p| p.timeline);
    let mut out = vec![(main.clone(), Ok(param.to_string()))];
    for t in &snap.selection {
        if t.key() == main.key() || !include.contains(&t.key()) {
            continue;
        }
        let verdict = match t.property(prop) {
            None => Err(format!("has no {}", main.property(prop).map_or(prop.to_string(), |i| if i.label.is_empty() { i.name.clone() } else { i.label.clone() }))),
            Some(_) if timeline => t
                .parameters
                .iter()
                .find(|r| snap.param(&r.id).is_some_and(|p| p.timeline))
                .map(|r| r.id.clone())
                .ok_or_else(|| "has no timeline".to_string()),
            Some(_) if t.parameters.iter().any(|r| r.id == param) => Ok(param.to_string()),
            Some(_) => Err(format!("can't use {}", snap.param(param).map_or("this parameter", |p| p.name.as_str()))),
        };
        out.push((t.clone(), verdict));
    }
    out
}

/// A curve's row in the list: a small drawing of the curve, then its name.
/// Hovering shows a larger one with the description.
/// Returns the row's response and whether its favourite star was clicked.
fn curve_row(ui: &mut egui::Ui, c: &library::Curve, button: egui::Button, favourite: bool) -> (egui::Response, bool) {
    let (r, star) = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let star = star_button(ui, favourite, 6.5, &format!("Favourite {}", c.data.name))
                .on_hover_text(if favourite { "Favourite: click to remove" } else { "Add to favourites (kept on this machine only)" })
                .clicked();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 14.0), Sense::hover());
            draw_preview(ui.painter(), rect, &c.data.points, pal().dim, 1.2);
            (ui.add(button), star)
        })
        .inner;
    let r = r.on_hover_ui(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(180.0, 90.0), Sense::hover());
        ui.painter().rect_filled(rect, 3.0, pal().plot);
        draw_preview(ui.painter(), rect.shrink(6.0), &c.data.points, pal().accent, 2.0);
        if !c.data.description.is_empty() {
            ui.set_max_width(180.0);
            ui.label(RichText::new(&c.data.description).small());
        }
    });
    (r, star)
}

/// A clickable favourite star: amber and filled when on, a faint outline when off.
/// `label` names it for accessibility (and the tests).
fn star_button(ui: &mut egui::Ui, on: bool, radius: f32, label: &str) -> egui::Response {
    let size = egui::vec2(radius * 2.0 + 6.0, (radius * 2.0 + 6.0).max(ui.spacing().interact_size.y * 0.8));
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    let colour = if on { pal().star } else if r.hovered() { pal().text } else { pal().dim.gamma_multiply(0.75) };
    theme::star(ui.painter(), rect.center(), radius, on, colour);
    let label = label.to_string();
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, &label));
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small drawing of a curve that's in Studio, scaled to its own extent
/// (its shape, not its values); hover for a larger one.
fn studio_thumb(ui: &mut egui::Ui, raw: &[studio::RawPoint], w: f32, h: f32) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(w, h), Sense::hover());
    let pts = shape_of(raw);
    ui.painter().rect_filled(rect, 3.0, pal().plot);
    draw_preview(ui.painter(), rect.shrink(2.0), &pts, pal().curve, 1.3);
    r.on_hover_ui(|ui| {
        let (big, _) = ui.allocate_exact_size(egui::vec2(180.0, 90.0), Sense::hover());
        ui.painter().rect_filled(big, 3.0, pal().plot);
        draw_preview(ui.painter(), big.shrink(6.0), &pts, pal().curve, 2.0);
        ui.label(RichText::new("The curve in Studio now (its shape).").small());
    })
}

/// A spatializer's built-in distance attenuation: its mode and range, a
/// drawing (dB over distance, min and max marked), and what to do instead,
/// since FMOD 2.02-2.04 can't give it a custom curve. Returns true when
/// "Save as a curve…" was pressed.
fn attenuation_view(ui: &mut egui::Ui, sp: &studio::Spatial) -> bool {
    let from = match sp.source.as_str() {
        "event" => " (the event's)",
        "each event" => " (each event's)",
        _ => "",
    };
    ui.label(RichText::new(format!("Distance attenuation: {} · {} to {}{from}", sp.rolloff, trim_num(sp.min), trim_num(sp.max))).small().strong());
    let span = (sp.max * 1.15).max(1e-3);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(320.0), 64.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, pal().plot);
    let plot = rect.shrink2(egui::vec2(6.0, 8.0));
    for (d, label) in [(sp.min, "min"), (sp.max, "max")] {
        let x = plot.left() + (d / span) as f32 * plot.width();
        painter.add(egui::Shape::dashed_line(&[egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())], egui::Stroke::new(1.0, pal().grid_major), 3.0, 3.0));
        painter.text(egui::pos2(x + 2.0, plot.top() - 6.0), egui::Align2::LEFT_TOP, label, egui::FontId::proportional(9.5), pal().label);
    }
    let pts: Vec<Point> = (0..=96)
        .map(|i| {
            let d = span * i as f64 / 96.0;
            let db = (20.0 * sp.gain(d).max(1e-9).log10()).max(-80.0);
            Point { x: i as f64 / 96.0, y: (db + 80.0) / 80.0, shape: 0.0, s: false }
        })
        .collect();
    draw_preview(painter, plot, &pts, pal().curve, 1.6);
    for (y, label) in [(plot.top(), "0 dB"), (plot.bottom(), "-80 dB")] {
        painter.text(egui::pos2(plot.right(), y), egui::Align2::RIGHT_CENTER, label, egui::FontId::proportional(9.5), pal().label);
    }
    resp.on_hover_text(format!("Gain in dB (0 at the top, -80 at the bottom) over distance 0 to {}.", trim_num(span)));
    if sp.is_off() {
        ui.label(RichText::new("Off: the spatializer doesn't attenuate, so a Gain automated on Distance can shape it instead.").small().color(pal().dim));
        return false;
    }
    ui.label(
        RichText::new(format!(
            "This curve is built into the spatializer: FMOD (2.02 to 2.04) can't replace it with your own. To use a custom curve, set Distance Attenuation to Off and add a Gain after the spatializer, automated on Distance from 0 to {}.",
            trim_num(sp.max)
        ))
        .small()
        .color(pal().dim),
    );
    ui.add(egui::Button::new("Save as a curve…").small())
        .on_hover_text("Keeps this attenuation in a library as a dB-shaped curve, ready for a Gain (-80 to +10 dB) on Distance")
        .clicked()
}

/// A spatializer's attenuation as a library curve: dB mapped onto -80..+10
/// (the Factory's convention for Gain), over distance 0..max.
fn attenuation_curve(sp: &studio::Spatial) -> Vec<Point> {
    (0..=16)
        .map(|i| {
            let t = i as f64 / 16.0;
            let db = (20.0 * sp.gain(sp.max * t).max(1e-9).log10()).max(-80.0);
            Point { x: t, y: ((db + 80.0) / 90.0).clamp(0.0, 1.0), shape: 0.0, s: false }
        })
        .collect()
}

/// A Studio curve's points rescaled to 0-1 on both axes.
fn shape_of(raw: &[studio::RawPoint]) -> Vec<Point> {
    let span = |f: fn(&studio::RawPoint) -> f64| {
        let lo = raw.iter().map(f).fold(f64::INFINITY, f64::min);
        let hi = raw.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
        (lo, hi)
    };
    let ((x0, x1), (y0, y1)) = (span(|q| q.position), span(|q| q.value));
    let norm = |v: f64, lo: f64, hi: f64| if hi > lo { (v - lo) / (hi - lo) } else { 0.5 };
    raw.iter().map(|q| Point { x: norm(q.position, x0, x1), y: norm(q.value, y0, y1), shape: q.shape, s: q.s }).collect()
}

/// The curve (0-1 on both axes) drawn into `rect`.
fn draw_preview(painter: &egui::Painter, rect: egui::Rect, pts: &[Point], color: Color32, width: f32) {
    if pts.is_empty() {
        return;
    }
    const STEPS: usize = 48;
    let line: Vec<egui::Pos2> = (0..=STEPS)
        .map(|i| {
            let x = i as f64 / STEPS as f64;
            let y = curve::value_at(pts, x).clamp(0.0, 1.0);
            egui::pos2(rect.left() + x as f32 * rect.width(), rect.bottom() - y as f32 * rect.height())
        })
        .collect();
    painter.add(egui::Shape::line(line, egui::Stroke::new(width, color)));
}

const PRESET_HELP: &str = "FMOD 2.04 keeps curve presets in the project. They go under Curve Library / <library> / <folders> in Studio's preset list; one with the same name there is replaced. Save the project in Studio to keep them.";

/// Text written for macOS, worded for this platform: Ctrl for Cmd, Explorer for
/// Finder. (egui's `Modifiers::COMMAND` is already Ctrl off macOS.)
fn for_platform(text: &str) -> std::borrow::Cow<'_, str> {
    if cfg!(target_os = "macos") {
        return text.into();
    }
    text.replace("Cmd", "Ctrl")
        .replace("Show in Finder", reveal_label())
        .replace("whatever macOS's appearance", "whatever the system's appearance")
        .replace("two-finger scroll, ", "scroll, ")
        .into()
}

fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Show in Finder"
    } else if cfg!(windows) {
        "Show in Explorer"
    } else {
        "Show in file manager"
    }
}

/// Opens the file manager at a folder, or with a file selected.
fn reveal(p: &Path) {
    let r = if cfg!(target_os = "macos") {
        if p.is_file() { std::process::Command::new("open").arg("-R").arg(p).spawn() } else { std::process::Command::new("open").arg(p).spawn() }
    } else if cfg!(windows) {
        if p.is_file() {
            std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn()
        } else {
            std::process::Command::new("explorer").arg(p).spawn()
        }
    } else {
        std::process::Command::new("xdg-open").arg(if p.is_file() { p.parent().unwrap_or(p) } else { p }).spawn()
    };
    if let Err(e) = r {
        eprintln!("couldn't open {}: {e}", p.display());
    }
}

fn trim_num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Whole-window tests: the real app, driven through its buttons.
#[cfg(test)]
mod app_tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn root_with_factory(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("acl-app-{tag}-{}-{}", std::process::id(), library::new_id()));
        let f = root.join(library::FACTORY);
        std::fs::create_dir_all(f.join("Fades")).unwrap();
        std::fs::write(f.join(library::META_FILE), r#"{"format":"dat-curve-library","version":1,"name":"Factory","readOnly":true}"#).unwrap();
        let c = CurveData::new("Fade in", templates()[0].1.clone(), "");
        std::fs::write(f.join("Fades").join("Fade in.curve.json"), serde_json::to_string_pretty(&c).unwrap()).unwrap();
        root
    }

    fn app(root: &Path) -> Harness<'static, EditorApp> {
        let r = root.to_path_buf();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1400.0, 860.0))
            .with_step_dt(1.0 / 60.0)
            .build_eframe(move |cc| EditorApp::new(cc, r, None, false));
        h.run_steps(4);
        h
    }

    #[test]
    fn wording_follows_the_platform() {
        let t = for_platform("Cmd+S saves · Show in Finder · two-finger scroll, pans");
        if cfg!(target_os = "macos") {
            assert_eq!(t, "Cmd+S saves · Show in Finder · two-finger scroll, pans");
        } else {
            assert_eq!(t, format!("Ctrl+S saves · {} · scroll, pans", reveal_label()));
        }
    }

    #[test]
    fn copying_from_factory_names_the_library_then_reveals_the_copy() {
        let root = root_with_factory("copy");
        let mut h = app(&root);
        assert!(h.state().current().is_some_and(|(l, _)| l.read_only()), "starts on the Factory curve");
        h.get_by_label("Copy to 'My Curves' to edit").click();
        h.run_steps(4);
        assert!(h.state().name_dialog.as_ref().is_some_and(|d| d.title == "Name your library"), "asks for a name first");
        h.get_by_label("Keep 'My Curves'").click();
        h.run_steps(4);
        let st = h.state();
        let sel = st.selected.clone().unwrap();
        assert!(sel.starts_with(root.join(library::DEFAULT_USER_LIBRARY)), "the copy is selected: {sel:?}");
        assert!(!st.shelf.library(&root.join(library::DEFAULT_USER_LIBRARY)).unwrap().needs_naming(), "asked once");
        // A second copy doesn't ask again.
        h.state_mut().select(root.join("Factory/Fades/Fade in.curve.json"));
        h.run_steps(2);
        h.get_by_label("Copy to 'My Curves' to edit").click();
        h.run_steps(4);
        assert!(h.state().name_dialog.is_none());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn dragging_reorders_moves_and_copies() {
        let root = root_with_factory("dnd");
        let mut h = app(&root);
        let mine = root.join(library::DEFAULT_USER_LIBRARY);
        library::keep_library_name(&mine).unwrap();
        let a = library::new_curve(&mine, &[], &CurveData::new("Alpha", templates()[0].1.clone(), "")).unwrap();
        let b = library::new_curve(&mine, &[], &CurveData::new("Bravo", templates()[0].1.clone(), "")).unwrap();
        let sub = library::create_folder(&mine, &[], "Sub").unwrap();
        h.state_mut().rescan();
        // Reorder: Bravo before Alpha.
        h.state_mut().run(Action::Drop { src: b.clone(), lib: mine.clone(), folder: vec![], before: Some(a.clone()) });
        let names: Vec<String> = h.state().shelf.library(&mine).unwrap().curves_in(&[]).iter().map(|c| c.data.name.clone()).collect();
        assert_eq!(names, ["Bravo", "Alpha"]);
        // Move Alpha into Sub.
        h.state_mut().run(Action::Drop { src: a.clone(), lib: mine.clone(), folder: sub.clone(), before: None });
        assert!(!a.exists() && mine.join("Sub").join("Alpha.curve.json").exists());
        // Factory → mine copies.
        let f = root.join("Factory/Fades/Fade in.curve.json");
        h.state_mut().run(Action::Drop { src: f.clone(), lib: mine.clone(), folder: vec![], before: None });
        assert!(f.exists() && mine.join("Fade in.curve.json").exists(), "a Factory drag copies");
        assert!(h.state().selected.as_ref().is_some_and(|p| p.starts_with(&mine)), "the copy is selected");
        // Nothing can be dropped into Factory.
        h.state_mut().run(Action::Drop { src: b.clone(), lib: root.join("Factory"), folder: vec![], before: None });
        assert!(b.exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn favourites_toggle_and_survive_a_move() {
        let root = root_with_factory("fav");
        let mut h = app(&root);
        let f = root.join("Factory/Fades/Fade in.curve.json");
        let id = h.state().shelf.find(&f).unwrap().1.data.id.clone();
        h.get_by_label("Favourite this curve").click();
        h.run_steps(3);
        assert!(h.state().favourites.contains(&id));
        assert!(h.query_by_label_contains("Favourites (1)").is_some(), "listed in the Favourites folder at the top");
        h.state_mut().collapse_all = true;
        h.run_steps(3);
        assert!(h.query_by_label_contains("Favourites (1)").is_some(), "still there with every folder closed");
        // Kept by id, so it's still a favourite after a rescan.
        h.state_mut().rescan();
        assert!(h.state().favourites.contains(&id));
        h.get_by_label("Favourite this curve").click();
        h.run_steps(3);
        assert!(h.state().favourites.is_empty(), "toggles off");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn long_text_never_hides_the_graph() {
        let root = root_with_factory("layout");
        let mine = root.join(library::DEFAULT_USER_LIBRARY);
        let long = "A very long curve name that goes on and on to see whether the header pushes the graph out of view";
        let r = root.clone();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1060.0, 640.0)) // the smallest window (main.rs)
            .with_step_dt(1.0 / 60.0)
            .build_eframe(move |cc| EditorApp::new(cc, r, None, false));
        h.run_steps(2);
        library::create_library(&root, "x").ok();
        let p = library::new_curve(&mine, &[], &CurveData::new(long, templates()[0].1.clone(), &long.repeat(3))).unwrap();
        h.state_mut().rescan();
        h.state_mut().select(p);
        // A Studio selection with very long labels, shown in the Studio tab.
        let t: Target = serde_json::from_value(serde_json::json!({
            "id": "{x}", "kind": "insert", "kindLabel": "Effect insert", "label": long, "where": long, "unique": true,
            "properties": [], "parameters": []
        }))
        .unwrap();
        h.state_mut().snap = Some(Snapshot { selection: vec![t.clone(), t], shared_effects: vec![], parameters: vec![] });
        h.run_steps(6);
        let plot = h.state().canvas.plot_rect;
        assert!(plot.width() >= MIN_GRAPH_WIDTH - 100.0, "the graph keeps its room: {plot:?}");
        assert!(plot.right() <= 1060.0 - RIGHT_MIN + 1.0, "and isn't under the inspector: {plot:?}");
        std::fs::remove_dir_all(root).ok();
    }

    fn curve_target() -> Target {
        serde_json::from_value(serde_json::json!({
            "id": "{fx}", "kind": "shared", "kindLabel": "Shared effect", "label": "'FMOD Gain'",
            "properties": [ { "name": "gain", "label": "Gain", "automated": true, "lo": -80, "hi": 10, "unit": "dB",
              "curves": [ { "parameterId": "{dist}", "parameterName": "Distance", "pmin": 0, "pmax": 20,
                "points": [ { "position": 2, "value": 0 }, { "position": 15, "value": -80 } ] } ] } ],
            "parameters": [ { "id": "{dist}" } ]
        }))
        .unwrap()
    }

    #[test]
    fn delete_key_off_the_canvas_changes_nothing() {
        let root = root_with_factory("keys");
        let mut h = app(&root);
        let mine = root.join(library::DEFAULT_USER_LIBRARY);
        let p = library::new_curve(&mine, &[], &CurveData::new("Three", vec![
            Point { x: 0.0, y: 0.0, shape: 0.0, s: false }, Point { x: 0.5, y: 1.0, shape: 0.0, s: false }, Point { x: 1.0, y: 0.0, shape: 0.0, s: false },
        ], "")).unwrap();
        h.state_mut().rescan();
        h.state_mut().select(p.clone());
        h.state_mut().point = Some(1);
        // Pointer over the library list, not the canvas.
        h.event(egui::Event::PointerMoved(egui::pos2(60.0, 300.0)));
        h.run_steps(2);
        h.key_press(Key::Backspace);
        h.run_steps(2);
        assert_eq!(h.state().current().unwrap().1.data.points.len(), 3, "Backspace in the list must not delete points");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn undo_last_apply_sends_the_previous_curve_or_removes_it() {
        let root = root_with_factory("undo");
        let mut h = app(&root);
        let (bridge, out) = Bridge::captured();
        h.state_mut().bridge = Some(bridge);
        let t = curve_target();
        let ctx = |previous| ApplyCtx {
            curve_name: "Bell".into(), target_label: t.title(), property: "Gain".into(), param_name: "Distance".into(),
            target_id: "{fx}".into(), property_name: "gain".into(), param_id: "{dist}".into(), previous,
        };
        h.state_mut().last_apply = vec![ctx(Some(t.curve("gain", "{dist}").unwrap().points.clone()))];
        h.state_mut().undo_last_apply();
        let sent = String::from_utf8(out.lock().unwrap().clone()).unwrap();
        assert!(sent.contains("\"cmd\":\"apply\"") && sent.contains("\"x\":15.0") && sent.contains("\"y\":-80.0"), "re-applies the old points as-is: {sent}");
        out.lock().unwrap().clear();
        h.state_mut().last_apply = vec![ctx(None)];
        h.state_mut().undo_last_apply();
        let sent = String::from_utf8(out.lock().unwrap().clone()).unwrap();
        assert!(sent.contains("\"cmd\":\"removeCurve\""), "a curve the apply created is removed: {sent}");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn studio_target_puts_units_and_a_ghost_on_the_canvas() {
        let root = root_with_factory("overlay");
        let mut h = app(&root);
        let (bridge, _) = Bridge::captured();
        assert!(h.state().show_studio_curve, "on by default");
        let st = h.state_mut();
        st.bridge = Some(bridge);
        st.snap = Some(Snapshot {
            selection: vec![curve_target()],
            shared_effects: vec![],
            parameters: vec![serde_json::from_value(serde_json::json!({ "id": "{dist}", "name": "Distance", "min": 0, "max": 20 })).unwrap()],
        });
        st.form.target = Some("{fx}".into());
        st.form.property = Some("gain".into());
        st.form.param = Some("{dist}".into());
        st.form.range = Range { plo: 0.0, phi: 20.0, vlo: -80.0, vhi: 10.0 };
        let o = st.studio_overlay();
        let y = o.y.unwrap();
        assert_eq!((y.unit.as_str(), y.title.as_str()), ("dB", "Gain"));
        assert_eq!(y.show(80.0 / 90.0), "0 dB");
        assert_eq!(o.x.unwrap().title, "Distance");
        assert_eq!(o.ghost.unwrap().len(), 2, "the curve already in Studio is shown");
        h.state_mut().show_studio_curve = false;
        assert!(h.state().studio_overlay().ghost.is_none(), "and hidden when the toggle is off");
        assert!(h.state().studio_overlay().y.is_some(), "the units stay");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn editing_a_studio_curve_in_place_writes_it_back_where_it_came_from() {
        let root = root_with_factory("studio_edit");
        let mut h = app(&root);
        let (bridge, out) = Bridge::captured();
        let before = h.state().selected.clone();
        let t = curve_target();
        let snap = Snapshot {
            selection: vec![t.clone()],
            shared_effects: vec![],
            parameters: vec![serde_json::from_value(serde_json::json!({ "id": "{dist}", "name": "Distance", "min": 0, "max": 20 })).unwrap()],
        };
        let st = h.state_mut();
        st.bridge = Some(bridge);
        st.snap = Some(snap.clone());
        st.begin_studio_edit(&t, &snap, "gain", "{dist}");
        assert!(st.selected.as_deref().is_some_and(Shelf::is_scratch), "the Studio curve is on screen");
        let pts = st.current().unwrap().1.data.points.clone();
        assert!((pts[0].x - 0.1).abs() < 1e-9 && (pts[1].y - 0.0).abs() < 1e-9, "mapped over Distance 0-20 and -80..10 dB: {pts:?}");
        let o = st.studio_overlay();
        assert_eq!(o.y.unwrap().unit, "dB", "drawn in Studio's units, whatever tab is open");
        h.run_steps(2);
        h.get_by_label("Flip").click();
        h.run_steps(2);
        assert!(h.state().unsaved.is_empty(), "a Studio edit is never a library edit");
        h.state_mut().apply_studio_edit();
        let sent = String::from_utf8(out.lock().unwrap().clone()).unwrap();
        assert!(sent.contains("\"targetId\":\"{fx}\"") && sent.contains("\"parameterId\":\"{dist}\""), "back to the same place: {sent}");
        assert!(sent.contains("\"vlo\":-80.0") && sent.contains("\"phi\":20.0"), "with the ranges it was opened with: {sent}");
        assert!(sent.contains("\"y\":1.0"), "flipped: the end is now at the top: {sent}");
        h.state_mut().end_studio_edit();
        assert_eq!(h.state().selected, before, "Done goes back to the library curve");
        assert!(h.state().shelf.scratch.is_none());
        // Opening a library curve also closes an edit (no "back to it").
        h.state_mut().begin_studio_edit(&t, &snap, "gain", "{dist}");
        h.state_mut().select(root.join("Factory/Fades/Fade in.curve.json"));
        assert!(h.state().studio_edit.is_none() && h.state().shelf.scratch.is_none());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn several_selected_things_each_get_the_curve_or_say_why_not() {
        let gain = |id: &str, label: &str, params: serde_json::Value| -> Target {
            serde_json::from_value(serde_json::json!({
                "id": id, "kind": "insert", "label": label,
                "properties": [ { "name": "gain", "label": "Gain", "lo": -80, "hi": 10, "unit": "dB" } ], "parameters": params,
            }))
            .unwrap()
        };
        let a = gain("{a}", "'Gain A'", serde_json::json!([ { "id": "{dist}" }, { "id": "{tl1}" } ]));
        let b = gain("{b}", "'Gain B'", serde_json::json!([ { "id": "{tl2}" } ]));
        let c: Target = serde_json::from_value(serde_json::json!({
            "id": "{c}", "kind": "sound", "label": "'Footstep'", "properties": [ { "name": "volume", "label": "Volume" } ], "parameters": [ { "id": "{dist}" } ],
        }))
        .unwrap();
        let p = |id: &str, name: &str, timeline: bool| -> studio::Param {
            serde_json::from_value(serde_json::json!({ "id": id, "name": name, "min": 0, "max": 10, "timeline": timeline })).unwrap()
        };
        let snap = Snapshot {
            selection: vec![a.clone(), b.clone(), c.clone()],
            shared_effects: vec![],
            parameters: vec![p("{dist}", "Distance", false), p("{tl1}", "Timeline", true), p("{tl2}", "Timeline", true)],
        };
        let all: BTreeSet<String> = snap.selection.iter().map(Target::key).collect();
        let plan = apply_plan(&snap, &a, &all, "gain", "{dist}");
        let verdicts: Vec<_> = plan.iter().map(|(t, r)| (t.id.as_str(), r.clone())).collect();
        assert_eq!(verdicts[0], ("{a}", Ok("{dist}".to_string())));
        assert!(matches!(&verdicts[1], ("{b}", Err(why)) if why.contains("Distance")), "B can't use Distance: {verdicts:?}");
        assert!(matches!(&verdicts[2], ("{c}", Err(why)) if why.contains("Gain")), "the sound has no Gain: {verdicts:?}");
        let on_timeline = apply_plan(&snap, &a, &all, "gain", "{tl1}");
        assert_eq!(on_timeline[1].1, Ok("{tl2}".to_string()), "each event's own timeline");
        // The lists offer only what every ticked one has.
        let both = common_target(&snap, &[a.clone(), b.clone()]).unwrap();
        assert_eq!(both.properties.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["gain"]);
        assert_eq!(both.parameters.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["{tl1}"], "B has no Distance; both have a timeline");
        assert!(common_target(&snap, &[a.clone(), c.clone()]).unwrap().properties.is_empty(), "an instrument and a gain effect share nothing");
        let only_a: BTreeSet<String> = [a.key()].into();
        assert_eq!(apply_plan(&snap, &a, &only_a, "gain", "{dist}").len(), 1, "unticked ones are left out");
    }

    #[test]
    fn sending_to_studio_presets_groups_curves_by_their_folder() {
        let root = root_with_factory("send_presets");
        let mut h = app(&root);
        let (bridge, out) = Bridge::captured();
        let mine = root.join(library::DEFAULT_USER_LIBRARY);
        let line = vec![Point { x: 0.0, y: 0.0, shape: 0.0, s: false }, Point { x: 1.0, y: 1.0, shape: 0.0, s: false }];
        let a = library::new_curve(&mine, &["Cars".into()], &CurveData::new("Rev", line.clone(), "")).unwrap();
        let b = library::new_curve(&mine, &["Cars".into()], &CurveData::new("Idle", line.clone(), "")).unwrap();
        let c = library::new_curve(&mine, &[], &CurveData::new("Top", line, "")).unwrap();
        let st = h.state_mut();
        st.bridge = Some(bridge);
        st.rescan();
        st.send_presets(&[a, b, c]);
        let sent = String::from_utf8(out.lock().unwrap().clone()).unwrap();
        let lines: Vec<&str> = sent.lines().filter(|l| l.contains("writePresets")).collect();
        assert_eq!(lines.len(), 2, "one request per folder: {sent}");
        assert!(lines[0].contains("[\"Curve Library\",\"My Curves\",\"Cars\"]") && lines[0].contains("\"Rev\"") && lines[0].contains("\"Idle\""), "{}", lines[0]);
        assert!(lines[1].contains("[\"Curve Library\",\"My Curves\"]") && lines[1].contains("\"Top\""), "{}", lines[1]);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn stays_dark_when_the_system_is_light() {
        // 0.8.0 followed macOS's light mode: dark text on the dark panels.
        let root = root_with_factory("light");
        let h = app(&root);
        assert_eq!(h.ctx.options(|o| o.theme_preference), egui::ThemePreference::Dark);
        let light = h.ctx.style_of(egui::Theme::Light);
        assert_eq!(light.visuals.override_text_color, Some(pal().text), "even the light style draws light text");
        assert!(light.visuals.dark_mode);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn opening_another_curve_shows_all_of_it() {
        let root = root_with_factory("fit");
        let mut h = app(&root);
        h.state_mut().canvas.view = [0.2, 0.4, 0.2, 0.4];
        let other = root.join("Factory/Fades/Fade out.curve.json");
        h.state_mut().select(other);
        assert!(!h.state().canvas.is_zoomed());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn guide_opens_from_its_button() {
        let root = root_with_factory("guide");
        let mut h = app(&root);
        h.get_by_label("Guide").click();
        h.run_steps(3);
        assert!(h.state().show_guide);
        assert!(h.query_by_label_contains("Applying to Studio").is_some(), "the guide's sections are shown");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn renaming_the_default_library_and_explicit_save() {
        let root = root_with_factory("save");
        let mut h = app(&root);
        h.get_by_label("Copy to 'My Curves' to edit").click();
        h.run_steps(4);
        h.state_mut().name_dialog.as_mut().unwrap().value = "Test Folder".into();
        h.get_by_label("Rename").click();
        h.run_steps(4);
        let sel = h.state().selected.clone().unwrap();
        assert!(sel.starts_with(root.join("Test Folder")), "renamed, and the copy landed in it: {sel:?}");
        // Edits stay in memory until Save.
        let on_disk = library::read_curve(&sel).unwrap().points;
        let mut pts = on_disk.clone();
        pts[0].y = 0.4;
        h.state_mut().set_points(&sel, pts);
        h.run_steps(2);
        assert_eq!(library::read_curve(&sel).unwrap().points, on_disk, "not written yet");
        assert!(h.state().unsaved.contains(&sel));
        h.key_press_modifiers(Modifiers::COMMAND, Key::S);
        h.run_steps(2);
        assert_eq!(library::read_curve(&sel).unwrap().points[0].y, 0.4, "Cmd+S saved it");
        assert!(h.state().unsaved.is_empty());
        // Revert drops unsaved edits.
        let mut pts = library::read_curve(&sel).unwrap().points;
        pts[0].y = 0.9;
        h.state_mut().set_points(&sel, pts);
        h.run_steps(2);
        h.get_by_label("Revert").click();
        h.run_steps(2);
        assert_eq!(h.state().current().unwrap().1.data.points[0].y, 0.4);
        std::fs::remove_dir_all(root).ok();
    }
}
