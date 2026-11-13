//! Curve libraries on disk. They live with the tool, per machine:
//!
//! ```text
//! <FMOD per-user folder>/Tools/AutomationCurveLibrary/Libraries/
//!     Factory/                     read-only, ships with the tool
//!         library.json             {"format": "dat-curve-library", "readOnly": true, ...}
//!         Fades/Fade in, gentle.curve.json
//!     My Curves/                   a user library: copy the folder to share it
//!         library.json
//!         Vehicles/Engine/Idle swell.curve.json
//! ```
//!
//! One folder per library, one file per curve, and the user's folders are real
//! folders, so a library can be copied into a repo as it is and merges well.
//! Fields this app doesn't know are kept when a curve is saved. A file that
//! can't be read is reported, skipped, and never written over.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const LIB_FORMAT: &str = "dat-curve-library";
pub const CURVE_FORMAT: &str = "dat-automation-curve";
pub const META_FILE: &str = "library.json";
pub const CURVE_EXT: &str = ".curve.json";
pub const DEFAULT_USER_LIBRARY: &str = "My Curves";
pub const FACTORY: &str = "Factory";
const AUTO_CREATED: &str = "autoCreated";
const NAME_KEPT: &str = "nameKept";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub shape: f64,
    #[serde(default)]
    pub s: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CurveData {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub points: Vec<Point>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub modified: String,
    /// Position in its folder when the user has ordered it by dragging;
    /// curves without one follow, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl CurveData {
    pub fn new(name: &str, points: Vec<Point>, description: &str) -> CurveData {
        let now = now_iso();
        let mut points = points;
        sort_points(&mut points);
        CurveData {
            format: CURVE_FORMAT.into(),
            version: 1,
            id: new_id(),
            name: name.into(),
            description: description.into(),
            points,
            created: now.clone(),
            modified: now,
            order: None,
            extra: Map::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMeta {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct Curve {
    pub path: PathBuf,
    /// Folder path inside the library, e.g. ["Vehicles", "Engine"]; empty at the top.
    pub folder: Vec<String>,
    pub data: CurveData,
}

#[derive(Clone, Debug)]
pub struct Library {
    pub dir: PathBuf,
    pub meta: LibraryMeta,
    /// Every folder, including empty ones, sorted.
    pub folders: Vec<Vec<String>>,
    /// Sorted by folder, then name.
    pub curves: Vec<Curve>,
}

impl Library {
    pub fn read_only(&self) -> bool {
        self.meta.read_only
    }
    pub fn name(&self) -> &str {
        &self.meta.name
    }
    /// The auto-created "My Curves" that the user hasn't named yet: they're
    /// asked to name it before the first save into it, so shared libraries
    /// aren't all called "My Curves".
    pub fn needs_naming(&self) -> bool {
        let flag = |k: &str| self.meta.extra.get(k) == Some(&Value::Bool(true));
        !self.read_only() && (flag(AUTO_CREATED) || (self.meta.name == DEFAULT_USER_LIBRARY && !flag(NAME_KEPT)))
    }
    pub fn folder_dir(&self, folder: &[String]) -> PathBuf {
        folder.iter().fold(self.dir.clone(), |d, f| d.join(f))
    }
    /// Direct subfolders of `folder`.
    pub fn subfolders(&self, folder: &[String]) -> Vec<&Vec<String>> {
        self.folders.iter().filter(|f| f.len() == folder.len() + 1 && f.starts_with(folder)).collect()
    }
    pub fn curves_in(&self, folder: &[String]) -> Vec<&Curve> {
        self.curves.iter().filter(|c| c.folder == folder).collect()
    }
}

/// Not a real folder: the scratch slot's made-up location.
const SCRATCH_DIR: &str = "<studio>";

/// Every library under the libraries folder.
#[derive(Clone, Debug, Default)]
pub struct Shelf {
    pub libraries: Vec<Library>,
    /// Files or folders that couldn't be read (shown to the user, never overwritten).
    pub problems: Vec<String>,
    /// A curve being edited straight from Studio: found like any other curve
    /// (so the canvas, Points tab and undo work on it) but in no library, so
    /// it's never listed, saved to disk or offered as a destination.
    pub scratch: Option<Library>,
}

impl Shelf {
    pub fn scan(root: &Path) -> Shelf {
        let mut shelf = Shelf::default();
        let Ok(entries) = std::fs::read_dir(root) else {
            shelf.problems.push(format!("Can't read the libraries folder {}", root.display()));
            return shelf;
        };
        let mut dirs: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir() && !hidden(p)).collect();
        dirs.sort_by_key(|d| {
            let n = file_name(d);
            (n != FACTORY, n.to_lowercase())
        });
        for dir in dirs {
            match read_library(&dir) {
                Ok((lib, problems)) => {
                    shelf.libraries.push(lib);
                    shelf.problems.extend(problems);
                }
                Err(e) => shelf.problems.push(e),
            }
        }
        shelf
    }

    pub fn find(&self, path: &Path) -> Option<(&Library, &Curve)> {
        self.libraries.iter().chain(self.scratch.iter()).find_map(|l| l.curves.iter().find(|c| c.path == path).map(|c| (l, c)))
    }

    pub fn find_mut(&mut self, path: &Path) -> Option<&mut Curve> {
        self.libraries.iter_mut().chain(self.scratch.iter_mut()).find_map(|l| l.curves.iter_mut().find(|c| c.path == path))
    }

    /// Puts `data` in the scratch slot (see `scratch`) and returns its path.
    pub fn set_scratch(&mut self, data: CurveData) -> PathBuf {
        let dir = PathBuf::from(SCRATCH_DIR);
        let path = Shelf::scratch_path();
        let meta = LibraryMeta {
            format: LIB_FORMAT.into(),
            version: 1,
            name: "FMOD Studio".into(),
            read_only: false,
            description: String::new(),
            extra: Map::new(),
        };
        self.scratch = Some(Library { dir, meta, folders: Vec::new(), curves: vec![Curve { path: path.clone(), folder: Vec::new(), data }] });
        path
    }

    pub fn scratch_path() -> PathBuf {
        PathBuf::from(SCRATCH_DIR).join("edit")
    }

    pub fn is_scratch(path: &Path) -> bool {
        path.starts_with(SCRATCH_DIR)
    }

    pub fn library(&self, dir: &Path) -> Option<&Library> {
        self.libraries.iter().find(|l| l.dir == dir)
    }

    /// Every place a curve can be written to: (library dir, folder, label).
    pub fn destinations(&self) -> Vec<(PathBuf, Vec<String>, String)> {
        let mut out = Vec::new();
        for l in self.libraries.iter().filter(|l| !l.read_only()) {
            out.push((l.dir.clone(), Vec::new(), l.name().to_string()));
            for f in &l.folders {
                out.push((l.dir.clone(), f.clone(), format!("{} / {}", l.name(), f.join(" / "))));
            }
        }
        out
    }

    pub fn first_writable(&self) -> Option<&Library> {
        self.libraries.iter().find(|l| !l.read_only())
    }
}

fn hidden(p: &Path) -> bool {
    file_name(p).starts_with('.')
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn read_meta(dir: &Path) -> Result<LibraryMeta, String> {
    let path = dir.join(META_FILE);
    if !path.exists() {
        // A folder without library.json (for example, copied in by hand) is a
        // writable library named after the folder.
        return Ok(LibraryMeta {
            format: LIB_FORMAT.into(),
            version: 1,
            name: file_name(dir),
            read_only: false,
            description: String::new(),
            extra: Map::new(),
        });
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
    let meta: LibraryMeta = serde_json::from_str(&text).map_err(|e| format!("{} isn't a valid library.json: {e}", path.display()))?;
    if meta.format != LIB_FORMAT {
        return Err(format!("{}: format is '{}', expected '{LIB_FORMAT}'", path.display(), meta.format));
    }
    Ok(meta)
}

fn read_library(dir: &Path) -> Result<(Library, Vec<String>), String> {
    let meta = read_meta(dir)?;
    let mut lib = Library { dir: dir.to_path_buf(), meta, folders: Vec::new(), curves: Vec::new() };
    let mut problems = Vec::new();
    walk(dir, &mut Vec::new(), &mut lib, &mut problems);
    lib.folders.sort_by_key(|f| f.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>());
    lib.curves.sort_by_key(|c| {
        (c.folder.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>(), c.data.order.unwrap_or(i64::MAX), c.data.name.to_lowercase())
    });
    Ok((lib, problems))
}

fn walk(dir: &Path, folder: &mut Vec<String>, lib: &mut Library, problems: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        if hidden(&p) {
            continue;
        }
        let name = file_name(&p);
        if p.is_dir() {
            folder.push(name);
            lib.folders.push(folder.clone());
            walk(&p, folder, lib, problems);
            folder.pop();
        } else if name.ends_with(CURVE_EXT) {
            match read_curve(&p) {
                Ok(data) => lib.curves.push(Curve { path: p, folder: folder.clone(), data }),
                Err(e) => problems.push(e),
            }
        }
    }
}

pub fn read_curve(path: &Path) -> Result<CurveData, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
    let mut data: CurveData = serde_json::from_str(&text).map_err(|e| format!("{} isn't a valid curve file: {e}", path.display()))?;
    if data.format != CURVE_FORMAT {
        return Err(format!("{}: format is '{}', expected '{CURVE_FORMAT}'", path.display(), data.format));
    }
    sort_points(&mut data.points);
    Ok(data)
}

// ── Writing ─────────────────────────────────────────────────────────────────

fn writable(lib_dir: &Path) -> Result<(), String> {
    let meta = read_meta(lib_dir)?;
    if meta.read_only {
        return Err(format!("'{}' is read-only. Copy the curve to one of your libraries to change it.", meta.name));
    }
    Ok(())
}

/// The library folder that contains `path` (the nearest folder up with a
/// library.json, else the top folder under the libraries root).
fn library_dir_of(path: &Path, root: &Path) -> Option<PathBuf> {
    let mut d = path.parent()?;
    loop {
        if d.join(META_FILE).exists() || d.parent() == Some(root) {
            return Some(d.to_path_buf());
        }
        d = d.parent()?;
        if !d.starts_with(root) {
            return None;
        }
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("Couldn't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Couldn't replace {}: {e}", path.display()))
}

fn json(v: &impl Serialize) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_default();
    s.push('\n');
    s
}

/// A name that is safe as a file or folder name on macOS and Windows.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { '-' } else { c }).collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    let mut s: String = trimmed.chars().take(80).collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if s.is_empty() { "Untitled".into() } else { s }
}

fn unique(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{base}{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{base} {n}{ext}"));
        n += 1;
    }
    p
}

pub fn create_library(root: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("Couldn't create {}: {e}", root.display()))?;
    let dir = unique(root, &sanitize(name), "");
    std::fs::create_dir(&dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    let meta = LibraryMeta {
        format: LIB_FORMAT.into(),
        version: 1,
        name: file_name(&dir),
        read_only: false,
        description: String::new(),
        extra: Map::new(),
    };
    write_atomic(&dir.join(META_FILE), &json(&meta))?;
    Ok(dir)
}

pub fn rename_library(dir: &Path, name: &str) -> Result<PathBuf, String> {
    writable(dir)?;
    let parent = dir.parent().ok_or("no parent folder")?;
    let target = if sanitize(name) == file_name(dir) { dir.to_path_buf() } else { unique(parent, &sanitize(name), "") };
    if target != dir {
        std::fs::rename(dir, &target).map_err(|e| format!("Couldn't rename the library: {e}"))?;
    }
    let mut meta = read_meta(&target)?;
    meta.name = file_name(&target);
    meta.extra.remove(AUTO_CREATED);
    write_atomic(&target.join(META_FILE), &json(&meta))?;
    Ok(target)
}

/// Keeps an auto-created library's name ("My Curves") and stops asking.
pub fn keep_library_name(dir: &Path) -> Result<(), String> {
    writable(dir)?;
    let mut meta = read_meta(dir)?;
    meta.extra.remove(AUTO_CREATED);
    meta.extra.insert(NAME_KEPT.into(), Value::Bool(true));
    write_atomic(&dir.join(META_FILE), &json(&meta))
}

pub fn delete_library(dir: &Path) -> Result<(), String> {
    writable(dir)?;
    std::fs::remove_dir_all(dir).map_err(|e| format!("Couldn't delete the library: {e}"))
}

pub fn create_folder(lib_dir: &Path, parent: &[String], name: &str) -> Result<Vec<String>, String> {
    writable(lib_dir)?;
    let base = parent.iter().fold(lib_dir.to_path_buf(), |d, f| d.join(f));
    let dir = unique(&base, &sanitize(name), "");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create the folder: {e}"))?;
    let mut f = parent.to_vec();
    f.push(file_name(&dir));
    Ok(f)
}

pub fn rename_folder(lib_dir: &Path, folder: &[String], name: &str) -> Result<Vec<String>, String> {
    writable(lib_dir)?;
    let dir = folder.iter().fold(lib_dir.to_path_buf(), |d, f| d.join(f));
    let parent = dir.parent().ok_or("no parent folder")?;
    let target = unique(parent, &sanitize(name), "");
    std::fs::rename(&dir, &target).map_err(|e| format!("Couldn't rename the folder: {e}"))?;
    let mut f = folder[..folder.len().saturating_sub(1)].to_vec();
    f.push(file_name(&target));
    Ok(f)
}

pub fn delete_folder(lib_dir: &Path, folder: &[String]) -> Result<(), String> {
    writable(lib_dir)?;
    if folder.is_empty() {
        return Err("That's the library itself".into());
    }
    let dir = folder.iter().fold(lib_dir.to_path_buf(), |d, f| d.join(f));
    std::fs::remove_dir_all(&dir).map_err(|e| format!("Couldn't delete the folder: {e}"))
}

pub fn new_curve(lib_dir: &Path, folder: &[String], data: &CurveData) -> Result<PathBuf, String> {
    writable(lib_dir)?;
    let dir = folder.iter().fold(lib_dir.to_path_buf(), |d, f| d.join(f));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create the folder: {e}"))?;
    let path = unique(&dir, &sanitize(&data.name), CURVE_EXT);
    let mut data = data.clone();
    data.name = file_name(&path).trim_end_matches(CURVE_EXT).to_string();
    write_atomic(&path, &json(&data))?;
    Ok(path)
}

/// One of FMOD 2.04's project curve presets, as Studio lists it.
#[derive(Deserialize, Clone, Debug)]
pub struct StudioPreset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub folder: Vec<String>,
    pub points: Vec<Point>,
}

/// The key that ties an imported curve to its Studio preset, so importing
/// again updates it instead of making a copy.
pub const STUDIO_PRESET_KEY: &str = "studioPresetId";

/// Imports Studio presets into `base` (a folder path) of a library, keeping
/// their folders. A curve imported earlier from the same preset is updated in
/// place. Returns (created, updated).
pub fn import_presets(lib_dir: &Path, base: &[String], presets: &[StudioPreset]) -> Result<(usize, usize), String> {
    writable(lib_dir)?;
    let (mut created, mut updated) = (0, 0);
    for pr in presets {
        let mut folder = base.to_vec();
        folder.extend(pr.folder.iter().map(|f| sanitize(f)));
        // Studio stores floats: 0.6000000238418579 → 0.6.
        let mut points: Vec<Point> = pr
            .points
            .iter()
            .map(|q| Point { x: round6(q.x), y: round6(q.y), shape: round6(q.shape), s: q.s })
            .collect();
        sort_points(&mut points);
        let dir = folder.iter().fold(lib_dir.to_path_buf(), |d, f| d.join(f));
        let earlier = std::fs::read_dir(&dir).ok().and_then(|entries| {
            entries.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(CURVE_EXT)).find_map(|p| {
                read_curve(&p).ok().filter(|d| d.extra.get(STUDIO_PRESET_KEY).and_then(Value::as_str) == Some(pr.id.as_str())).map(|d| (p, d))
            })
        });
        match earlier {
            Some((path, mut data)) => {
                if data.points != points {
                    data.points = points;
                    data.modified = now_iso();
                    write_atomic(&path, &json(&data))?;
                    updated += 1;
                }
            }
            None => {
                let mut data = CurveData::new(if pr.name.is_empty() { "Preset" } else { &pr.name }, points, "Imported from FMOD Studio's curve presets.");
                data.extra.insert(STUDIO_PRESET_KEY.into(), Value::String(pr.id.clone()));
                new_curve(lib_dir, &folder, &data)?;
                created += 1;
            }
        }
    }
    Ok((created, updated))
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// Saves a curve's points (and anything else in `data`) in place.
pub fn save_curve(root: &Path, path: &Path, data: &CurveData) -> Result<(), String> {
    let lib = library_dir_of(path, root).ok_or("the curve isn't inside the libraries folder")?;
    writable(&lib)?;
    write_atomic(path, &json(data))
}

pub fn rename_curve(root: &Path, path: &Path, name: &str) -> Result<PathBuf, String> {
    let lib = library_dir_of(path, root).ok_or("the curve isn't inside the libraries folder")?;
    writable(&lib)?;
    let mut data = read_curve(path)?;
    let dir = path.parent().ok_or("no folder")?;
    let target = if sanitize(name) == data.name { path.to_path_buf() } else { unique(dir, &sanitize(name), CURVE_EXT) };
    data.name = file_name(&target).trim_end_matches(CURVE_EXT).to_string();
    data.modified = now_iso();
    write_atomic(&target, &json(&data))?;
    if target != path {
        std::fs::remove_file(path).map_err(|e| format!("Couldn't remove the old file: {e}"))?;
    }
    Ok(target)
}

/// Copies a curve (from any library, Factory included) into a writable one,
/// as a new curve with its own id.
pub fn copy_curve(path: &Path, lib_dir: &Path, folder: &[String]) -> Result<PathBuf, String> {
    let src = read_curve(path)?;
    let mut data = CurveData::new(&src.name, src.points.clone(), &src.description);
    data.extra = src.extra;
    new_curve(lib_dir, folder, &data)
}

pub fn move_curve(root: &Path, path: &Path, lib_dir: &Path, folder: &[String]) -> Result<PathBuf, String> {
    let from = library_dir_of(path, root).ok_or("the curve isn't inside the libraries folder")?;
    writable(&from)?;
    writable(lib_dir)?;
    let data = read_curve(path)?;
    let dir = folder.iter().fold(lib_dir.to_path_buf(), |d, f| d.join(f));
    if path.parent() == Some(dir.as_path()) {
        return Ok(path.to_path_buf());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create the folder: {e}"))?;
    let target = unique(&dir, &sanitize(&data.name), CURVE_EXT);
    std::fs::rename(path, &target).map_err(|e| format!("Couldn't move the curve: {e}"))?;
    Ok(target)
}

/// Writes `order` into each curve file, in the given sequence (a folder's
/// curves after a drag). Only files whose order changes are rewritten.
pub fn set_order(root: &Path, paths: &[PathBuf]) -> Result<(), String> {
    for (i, p) in paths.iter().enumerate() {
        let lib = library_dir_of(p, root).ok_or("the curve isn't inside the libraries folder")?;
        writable(&lib)?;
        let mut data = read_curve(p)?;
        let want = Some(i as i64 * 10);
        if data.order != want {
            data.order = want;
            write_atomic(p, &json(&data))?;
        }
    }
    Ok(())
}

pub fn delete_curve(root: &Path, path: &Path) -> Result<(), String> {
    let lib = library_dir_of(path, root).ok_or("the curve isn't inside the libraries folder")?;
    writable(&lib)?;
    std::fs::remove_file(path).map_err(|e| format!("Couldn't delete the curve: {e}"))
}

/// Makes sure there's somewhere to save: creates "My Curves" when there's no
/// writable library yet.
pub fn ensure_user_library(root: &Path) -> Result<Option<PathBuf>, String> {
    let shelf = Shelf::scan(root);
    if shelf.first_writable().is_some() {
        return Ok(None);
    }
    let dir = create_library(root, DEFAULT_USER_LIBRARY)?;
    let mut meta = read_meta(&dir)?;
    meta.extra.insert(AUTO_CREATED.into(), Value::Bool(true));
    write_atomic(&dir.join(META_FILE), &json(&meta))?;
    Ok(Some(dir))
}

/// A fingerprint of everything under the libraries folder (paths, sizes,
/// modification times), to notice changes made outside the app.
pub fn stamp(root: &Path) -> u64 {
    fn visit(dir: &Path, h: &mut std::collections::hash_map::DefaultHasher) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            p.hash(h);
            if let Ok(m) = std::fs::metadata(&p) {
                m.len().hash(h);
                if let Ok(t) = m.modified() {
                    t.hash(h);
                }
                if m.is_dir() {
                    visit(&p, h);
                }
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    visit(root, &mut h);
    h.finish()
}

pub fn sort_points(points: &mut [Point]) {
    points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
}

/// Starter shapes for New curve.
pub fn templates() -> Vec<(&'static str, Vec<Point>)> {
    let p = |x, y, shape, s| Point { x, y, shape, s };
    vec![
        ("Linear rise", vec![p(0.0, 0.0, 0.0, false), p(1.0, 1.0, 0.0, false)]),
        ("Linear fall", vec![p(0.0, 1.0, 0.0, false), p(1.0, 0.0, 0.0, false)]),
        ("Ease in (starts slow)", vec![p(0.0, 0.0, 0.6, false), p(1.0, 1.0, 0.0, false)]),
        ("Ease out (starts fast)", vec![p(0.0, 0.0, -0.6, false), p(1.0, 1.0, 0.0, false)]),
        ("S-curve", vec![p(0.0, 0.0, 0.6, true), p(1.0, 1.0, 0.0, false)]),
        ("Flat", vec![p(0.0, 0.5, 0.0, false), p(1.0, 0.5, 0.0, false)]),
    ]
}

/// "c_" + base-36 time + a counter.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ms = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed) + (std::process::id() as u64) * 1000;
    format!("c_{}{}", base36(ms), base36(n))
}

fn base36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(D[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// UTC time as `YYYY-MM-DDTHH:MM:SS.mmmZ`, like JavaScript's `toISOString`.
pub fn now_iso() -> String {
    let d = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60,
        d.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("acl-lib-{tag}-{}-{}", std::process::id(), new_id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn importing_studio_presets_keeps_folders_and_updates_on_reimport() {
        let root = temp_root("presets");
        let lib = create_library(&root, "Mine").unwrap();
        let pr = |y: f64| StudioPreset {
            id: "{p1}".into(),
            name: "Bell".into(),
            folder: vec!["Curve Library".into(), "Shapes".into()],
            points: vec![Point { x: 0.0, y: 0.0, shape: 0.6000000238418579, s: true }, Point { x: 1.0, y, shape: 0.0, s: false }],
        };
        let base = vec!["Studio presets".to_string()];
        assert_eq!(import_presets(&lib, &base, &[pr(1.0)]).unwrap(), (1, 0));
        let path = lib.join("Studio presets").join("Curve Library").join("Shapes").join(format!("Bell{CURVE_EXT}"));
        let data = read_curve(&path).unwrap();
        assert_eq!(data.points[0].shape, 0.6, "float noise rounded off");
        assert_eq!(import_presets(&lib, &base, &[pr(1.0)]).unwrap(), (0, 0), "unchanged: nothing to do");
        assert_eq!(import_presets(&lib, &base, &[pr(0.5)]).unwrap(), (0, 1), "changed in Studio: updated, not copied");
        assert_eq!(read_curve(&path).unwrap().points[1].y, 0.5);
        std::fs::remove_dir_all(root).ok();
    }

    fn factory(root: &Path) {
        let dir = root.join(FACTORY);
        std::fs::create_dir_all(dir.join("Fades")).unwrap();
        std::fs::write(
            dir.join(META_FILE),
            r#"{"format":"dat-curve-library","version":1,"name":"Factory","readOnly":true}"#,
        )
        .unwrap();
        let c = CurveData::new("Fade in", templates()[0].1.clone(), "test");
        std::fs::write(dir.join("Fades").join("Fade in.curve.json"), json(&c)).unwrap();
    }

    #[test]
    fn scan_orders_factory_first_and_reads_folders() {
        let root = temp_root("scan");
        factory(&root);
        let mine = create_library(&root, "Alpha").unwrap();
        create_folder(&mine, &[], "Vehicles").unwrap();
        let sub = create_folder(&mine, &["Vehicles".into()], "Engine").unwrap();
        new_curve(&mine, &sub, &CurveData::new("Idle swell", templates()[2].1.clone(), "")).unwrap();
        std::fs::write(mine.join("broken.curve.json"), "{ nope").unwrap();
        let shelf = Shelf::scan(&root);
        assert_eq!(shelf.libraries.len(), 2);
        assert_eq!(shelf.libraries[0].name(), "Factory");
        assert!(shelf.libraries[0].read_only());
        let a = &shelf.libraries[1];
        assert_eq!(a.folders, vec![vec!["Vehicles".to_string()], vec!["Vehicles".to_string(), "Engine".to_string()]]);
        assert_eq!(a.curves[0].folder, sub);
        assert_eq!(a.subfolders(&[]).len(), 1);
        assert_eq!(shelf.problems.len(), 1, "the broken file is reported: {:?}", shelf.problems);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn factory_is_read_only_but_copyable() {
        let root = temp_root("ro");
        factory(&root);
        let mine = create_library(&root, "Mine").unwrap();
        let shelf = Shelf::scan(&root);
        let fade = shelf.libraries[0].curves[0].path.clone();
        let data = read_curve(&fade).unwrap();
        assert!(save_curve(&root, &fade, &data).unwrap_err().contains("read-only"));
        assert!(rename_curve(&root, &fade, "x").is_err());
        assert!(delete_curve(&root, &fade).is_err());
        assert!(move_curve(&root, &fade, &mine, &[]).is_err());
        assert!(create_folder(&root.join(FACTORY), &[], "x").is_err());
        assert!(delete_library(&root.join(FACTORY)).is_err());
        let copy = copy_curve(&fade, &mine, &[]).unwrap();
        let c = read_curve(&copy).unwrap();
        assert_eq!(c.points, data.points);
        assert_ne!(c.id, data.id, "a copy is a new curve");
        assert!(fade.exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn curve_lifecycle_and_unique_names() {
        let root = temp_root("life");
        let mine = create_library(&root, "Mine").unwrap();
        let a = new_curve(&mine, &[], &CurveData::new("Swell", templates()[0].1.clone(), "")).unwrap();
        let b = new_curve(&mine, &[], &CurveData::new("Swell", templates()[1].1.clone(), "")).unwrap();
        assert_eq!(file_name(&b), "Swell 2.curve.json");
        assert_eq!(read_curve(&b).unwrap().name, "Swell 2");
        let renamed = rename_curve(&root, &a, "Big: swell?").unwrap();
        assert_eq!(read_curve(&renamed).unwrap().name, "Big- swell-");
        assert!(!a.exists());
        let f = create_folder(&mine, &[], "Sub").unwrap();
        let moved = move_curve(&root, &renamed, &mine, &f).unwrap();
        assert!(moved.starts_with(mine.join("Sub")));
        let mut data = read_curve(&moved).unwrap();
        data.points[0].y = 0.25;
        save_curve(&root, &moved, &data).unwrap();
        assert_eq!(read_curve(&moved).unwrap().points[0].y, 0.25);
        let f2 = rename_folder(&mine, &f, "Renamed").unwrap();
        assert_eq!(f2, vec!["Renamed".to_string()]);
        delete_folder(&mine, &f2).unwrap();
        let lib2 = rename_library(&mine, "Ours").unwrap();
        assert_eq!(Shelf::scan(&root).libraries[0].name(), "Ours");
        delete_curve(&root, &lib2.join("Swell 2.curve.json")).unwrap();
        delete_library(&lib2).unwrap();
        assert!(Shelf::scan(&root).libraries.is_empty());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unknown_fields_survive_a_save() {
        let root = temp_root("extra");
        let mine = create_library(&root, "Mine").unwrap();
        let p = mine.join("X.curve.json");
        std::fs::write(
            &p,
            r#"{"format":"dat-automation-curve","version":1,"id":"c1","name":"X","points":[{"x":0,"y":0},{"x":1,"y":1}],"futureField":7}"#,
        )
        .unwrap();
        let data = read_curve(&p).unwrap();
        save_curve(&root, &p, &data).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["futureField"], 7);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn default_user_library_and_stamp() {
        let root = temp_root("default");
        factory(&root);
        let made = ensure_user_library(&root).unwrap();
        assert_eq!(made.as_deref().map(file_name), Some(DEFAULT_USER_LIBRARY.to_string()));
        let shelf = Shelf::scan(&root);
        assert!(shelf.libraries[1].needs_naming(), "the auto-created library asks to be named");
        keep_library_name(made.as_deref().unwrap()).unwrap();
        assert!(!Shelf::scan(&root).libraries[1].needs_naming());
        // A "My Curves" made before the flag existed (0.4.0) also asks.
        let old = create_library(&root, "Older").unwrap();
        let mut meta = read_meta(&old).unwrap();
        meta.name = DEFAULT_USER_LIBRARY.into();
        write_atomic(&old.join(META_FILE), &json(&meta)).unwrap();
        assert!(Shelf::scan(&root).libraries.iter().any(|l| l.dir == old && l.needs_naming()));
        assert!(ensure_user_library(&root).unwrap().is_none(), "only once");
        let s1 = stamp(&root);
        std::thread::sleep(std::time::Duration::from_millis(20));
        new_curve(&root.join(DEFAULT_USER_LIBRARY), &[], &CurveData::new("Y", templates()[0].1.clone(), "")).unwrap();
        assert_ne!(stamp(&root), s1);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn sanitize_names() {
        assert_eq!(sanitize("  ../a/b\\c:d  "), "-a-b-c-d");
        assert_eq!(sanitize(""), "Untitled");
        assert_eq!(sanitize("x."), "x");
    }

    #[test]
    fn iso_time_shape() {
        let t = now_iso();
        assert_eq!(t.len(), 24);
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T');
    }

    #[test]
    fn custom_order_sorts_first_and_persists() {
        let root = temp_root("order");
        let mine = create_library(&root, "Mine").unwrap();
        let a = new_curve(&mine, &[], &CurveData::new("Alpha", templates()[0].1.clone(), "")).unwrap();
        let b = new_curve(&mine, &[], &CurveData::new("Bravo", templates()[0].1.clone(), "")).unwrap();
        let c = new_curve(&mine, &[], &CurveData::new("Charlie", templates()[0].1.clone(), "")).unwrap();
        let names = |root: &Path| Shelf::scan(root).libraries[0].curves.iter().map(|c| c.data.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&root), ["Alpha", "Bravo", "Charlie"]);
        set_order(&root, &[c.clone(), a.clone(), b.clone()]).unwrap();
        assert_eq!(names(&root), ["Charlie", "Alpha", "Bravo"]);
        factory_like_read_only(&root);
        std::fs::remove_dir_all(root).ok();
    }

    fn factory_like_read_only(root: &Path) {
        factory(root);
        let f = Shelf::scan(root).libraries[0].curves[0].path.clone();
        assert!(set_order(root, &[f]).is_err(), "Factory can't be reordered");
    }
}
