//! The node-graph view, laid out in signal flow: sources on the left, where
//! they go on the right (a child event, then the events that use it; an
//! asset, then its events; a bus, then its output). Drag to pan; pinch or
//! Cmd/Ctrl+scroll to zoom; scroll to pan. Click a box or a link's dot to
//! select it; double-click a box to open it in Studio; right-click for more.

pub use crate::model::Graph;
use crate::kit::{self, MenuAct};
use crate::model::{Edge, Node};
use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, epaint::CubicBezierShape, pos2, vec2};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// Toward destinations (events that use it, outputs).
    Up,
    Both,
    /// Toward sources (events it uses, inputs).
    Down,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Pick {
    Node(String),
    Edge(String),
}

#[derive(Default)]
pub struct Out {
    pub clicked: Option<Pick>,
    pub open: Option<(String, String)>,
    /// The background was clicked: clear the selection.
    pub clear: bool,
}

/// Pan, zoom, and which crowded columns are expanded.
#[derive(Clone, Debug)]
pub struct View {
    pub offset: Vec2,
    pub zoom: f32,
    pub expanded: HashSet<usize>,
    /// Where each box was last frame (graph space), to hold the view still
    /// when the layout shifts under it.
    pub last: HashMap<String, Pos2>,
}

impl Default for View {
    fn default() -> Self {
        View { offset: vec2(20.0, 40.0), zoom: 1.0, expanded: HashSet::new(), last: HashMap::new() }
    }
}

impl View {
    /// When the layout moves `anchor` (unfolding a path, folding a column),
    /// shift the camera by the same amount so it stays put on screen. Then
    /// remember this frame's positions.
    pub fn keep_still(&mut self, l: &Layout, anchor: Option<&str>) {
        if let Some(a) = anchor {
            let now = l.boxes.iter().find(|(n, _)| n.id == a).map(|(_, r)| r.min);
            if let (Some(before), Some(now)) = (self.last.get(a), now) {
                self.offset += (*before - now) * self.zoom;
            }
        }
        self.last = l.boxes.iter().map(|(n, r)| (n.id.clone(), r.min)).collect();
    }
}

const BOX_W: f32 = 230.0;
const BOX_H: f32 = 86.0;
const COL_W: f32 = 380.0;
const ROW_H: f32 = 104.0;
/// More boxes than this in a column collapse into "+N more".
pub const MAX_PER_COLUMN: usize = 10;
/// Graph-space room above the boxes for the column headings.
const TOP: f32 = 36.0;

pub const ORANGE: Color32 = kit::ORANGE;
const DIM: Color32 = kit::DIM;
/// How much of the content must stay in view when panning, in points.
const PAN_MARGIN: f32 = 120.0;
const EDGE: Color32 = Color32::from_rgb(110, 118, 130);

fn node_fill(n: &Node) -> Color32 {
    if n.further {
        return Color32::from_rgb(30, 32, 37);
    }
    match n.kind.as_str() {
        "asset" => Color32::from_rgb(30, 46, 52),
        "bus" => Color32::from_rgb(40, 36, 54),
        "return" => Color32::from_rgb(30, 48, 42),
        "vca" => Color32::from_rgb(50, 46, 30),
        "preset" => Color32::from_rgb(50, 38, 38),
        _ => Color32::from_rgb(36, 39, 45),
    }
}

/// Link colour per kind.
pub fn edge_colour(kind: &str) -> Color32 {
    match kind {
        "asset" => Color32::from_rgb(90, 160, 170),
        "route" => Color32::from_rgb(150, 130, 200),
        "send" => Color32::from_rgb(110, 190, 140),
        "effect" => Color32::from_rgb(200, 130, 120),
        "vca" => Color32::from_rgb(200, 180, 90),
        "snapshot" => Color32::from_rgb(210, 120, 160),
        _ => EDGE,
    }
}

fn visible(n: &Node, side: Side) -> bool {
    match side {
        Side::Up => n.depth <= 0,
        Side::Down => n.depth >= 0,
        Side::Both => true,
    }
}

/// The link that connects a box toward the selection, if any.
pub fn link_toward_focus<'a>(g: &'a Graph, n: &Node) -> Option<&'a Edge> {
    if n.is_focus {
        return None;
    }
    let depth = |id: &str| g.node(id).map_or(0, |x| x.depth);
    if n.depth < 0 {
        g.edges.iter().find(|e| e.parent_id == n.id && depth(&e.child_id) > n.depth)
    } else {
        g.edges.iter().find(|e| e.child_id == n.id && depth(&e.parent_id) < n.depth)
    }
}

/// Column headings per graph mode, by distance from the selection's column.
fn column_label(mode: &str, d: i32) -> String {
    if d == 0 {
        return "SELECTED".into();
    }
    let (left, right) = match mode {
        "bus" | "return" | "send" => ("INPUTS", "OUTPUTS"),
        "asset" | "preset" => ("", "USED IN"),
        "vca" => ("CONTROLS", ""),
        _ => ("PARENT OF", "REFERENCED IN"),
    };
    let name = if d < 0 { left } else { right };
    if name.is_empty() { String::new() } else { format!("{name} · {}", d.abs()) }
}

pub struct Layout<'a> {
    pub boxes: Vec<(&'a Node, Rect)>,
    /// Crowded columns: (column, box, how many hidden, expanded).
    pub more: Vec<(usize, Rect, usize, bool)>,
    /// Boxes folded into a column's "+N more": node id → column.
    pub folded: HashMap<String, usize>,
    pub headings: Vec<(f32, String)>,
    pub size: Vec2,
}

/// Box positions in graph space: a column per flow rank (sources left), rows
/// ordered to keep links short; crowded columns collapse unless expanded.
/// `pinned` boxes (the lit path of a selected box) are always shown, so a
/// highlighted path never runs through a hidden box.
pub fn layout<'a>(g: &'a Graph, side: Side, expanded: &HashSet<usize>, pinned: &HashSet<String>) -> Layout<'a> {
    let nodes: Vec<&Node> = g.nodes.iter().filter(|n| visible(n, side)).collect();
    let idx: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let links: Vec<(usize, usize)> = g
        .edges
        .iter()
        .filter_map(|e| Some((*idx.get(e.child_id.as_str())?, *idx.get(e.parent_id.as_str())?)))
        .filter(|(a, b)| a != b)
        .collect();
    let n = nodes.len();
    if n == 0 {
        return Layout { boxes: Vec::new(), more: Vec::new(), folded: HashMap::new(), headings: Vec::new(), size: vec2(BOX_W, BOX_H) };
    }
    // Longest path from the sources; cycles stop after n passes.
    let mut rank = vec![0i32; n];
    for _ in 0..n {
        let mut changed = false;
        for &(c, p) in &links {
            if rank[p] < rank[c] + 1 && rank[c] + 1 < n as i32 {
                rank[p] = rank[c] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Pull sources right, next to what they feed.
    for i in 0..n {
        if links.iter().any(|&(_, p)| p == i) {
            continue;
        }
        if let Some(m) = links.iter().filter(|&&(c, _)| c == i).map(|&(_, p)| rank[p]).min() {
            rank[i] = (m - 1).max(0);
        }
    }
    let cols = rank.iter().copied().max().unwrap_or(0) as usize + 1;
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); cols];
    for i in 0..n {
        columns[rank[i] as usize].push(i);
    }
    // Barycentre sweeps: order each column by its neighbours' rows.
    let mut row = vec![0f32; n];
    for col in &columns {
        for (r, &i) in col.iter().enumerate() {
            row[i] = r as f32;
        }
    }
    for sweep in 0..4 {
        let order: Vec<usize> = if sweep % 2 == 0 { (1..cols).collect() } else { (0..cols.saturating_sub(1)).rev().collect() };
        for c in order {
            let mut keyed: Vec<(f32, usize)> = columns[c]
                .iter()
                .map(|&i| {
                    let nb: Vec<f32> = links
                        .iter()
                        .filter_map(|&(a, b)| {
                            if b == i && rank[a] as usize != c {
                                Some(row[a])
                            } else if a == i && rank[b] as usize != c {
                                Some(row[b])
                            } else {
                                None
                            }
                        })
                        .collect();
                    (if nb.is_empty() { row[i] } else { nb.iter().sum::<f32>() / nb.len() as f32 }, i)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            columns[c] = keyed.into_iter().map(|k| k.1).collect();
            for (r, &i) in columns[c].iter().enumerate() {
                row[i] = r as f32;
            }
        }
    }
    // Crowded columns: keep the selection, the pinned path and the first others, then "+N more".
    let mut shown: Vec<Vec<usize>> = Vec::new();
    let mut hidden = vec![0usize; cols];
    let mut folded: HashMap<String, usize> = HashMap::new();
    for (c, col) in columns.iter().enumerate() {
        if col.len() <= MAX_PER_COLUMN || expanded.contains(&c) {
            shown.push(col.clone());
            continue;
        }
        let must = |i: usize| nodes[i].is_focus || pinned.contains(&nodes[i].id);
        let mut keep: Vec<usize> = col.iter().copied().filter(|&i| must(i)).collect();
        for &i in col {
            if keep.len() >= MAX_PER_COLUMN - 1 {
                break;
            }
            if !must(i) {
                keep.push(i);
            }
        }
        keep.sort_by(|a, b| row[*a].total_cmp(&row[*b]));
        hidden[c] = col.len() - keep.len();
        for &i in col {
            if !keep.contains(&i) {
                folded.insert(nodes[i].id.clone(), c);
            }
        }
        shown.push(keep);
    }
    let crowded: Vec<bool> = (0..cols).map(|c| columns[c].len() > MAX_PER_COLUMN).collect();
    let rows_in = |c: usize| shown[c].len() + usize::from(crowded[c]);
    let tallest = (0..cols).map(rows_in).max().unwrap_or(1) as f32;
    let focus_cols: Vec<i32> = (0..cols).filter(|&c| shown[c].iter().any(|&i| nodes[i].is_focus)).map(|c| c as i32).collect();
    let mut out = Layout { boxes: Vec::new(), more: Vec::new(), folded, headings: Vec::new(), size: vec2((cols - 1) as f32 * COL_W + BOX_W, TOP + tallest * ROW_H) };
    for c in 0..cols {
        let top = TOP + (tallest - rows_in(c) as f32) * ROW_H / 2.0;
        let x = c as f32 * COL_W;
        for (r, &i) in shown[c].iter().enumerate() {
            out.boxes.push((nodes[i], Rect::from_min_size(pos2(x, top + r as f32 * ROW_H), vec2(BOX_W, BOX_H))));
        }
        if crowded[c] {
            let r = shown[c].len() as f32;
            out.more.push((c, Rect::from_min_size(pos2(x, top + r * ROW_H), vec2(BOX_W, 30.0)), hidden[c], expanded.contains(&c)));
        }
        // Signed distance to the nearest selection column.
        let d = focus_cols.iter().map(|&f| c as i32 - f).min_by_key(|d| d.abs()).unwrap_or(0);
        // The further chain is one group: only its first column is headed.
        let further = !shown[c].is_empty() && shown[c].iter().all(|&i| nodes[i].further);
        let already = out.headings.iter().any(|h| h.1 == "FURTHER CHAIN");
        let label = match (further, already) {
            (true, true) => String::new(),
            (true, false) => "FURTHER CHAIN".into(),
            _ => column_label(g.mode(), d),
        };
        out.headings.push((x, label));
    }
    out
}

/// The graph plus what lies past its top-level events: each one's route
/// through the mixer to the Master Bus, and the snapshots it starts. These
/// "further chain" boxes are marked `further` and drawn dimmer.
pub fn with_further_chain(g: &Graph) -> Graph {
    let mut out = g.clone();
    if !matches!(g.mode(), "event" | "asset" | "preset") {
        return out;
    }
    let mut have: HashSet<String> = out.nodes.iter().map(|n| n.id.clone()).collect();
    let mut links: HashSet<String> = out.edges.iter().map(|e| e.id.clone()).collect();
    for n in g.nodes.iter().filter(|n| n.is_event() && n.used_by == 0 && !n.snapshot) {
        if let Some(route) = &n.route {
            let (mut prev, mut depth) = (n.id.clone(), n.depth);
            // route[0] is the event's own input strip.
            for s in route.iter().skip(1) {
                depth -= 1;
                if have.insert(s.id.clone()) {
                    let master = s.entity == "MixerMaster";
                    out.nodes.push(Node {
                        id: s.id.clone(),
                        kind: if s.entity.contains("Return") { "return".into() } else { "bus".into() },
                        label: if master { "Master Bus".into() } else { s.kind.clone() },
                        name: if s.name.is_empty() { s.kind.clone() } else { s.name.clone() },
                        depth,
                        color: s.color.clone(),
                        strip: Some(s.clone()),
                        further: true,
                        ..Default::default()
                    });
                }
                let id = format!("{prev}>{}", s.id);
                if links.insert(id.clone()) {
                    out.edges.push(Edge { id, kind: "route".into(), child_id: prev.clone(), parent_id: s.id.clone(), further: true, ..Default::default() });
                }
                prev = s.id.clone();
            }
        }
        for sn in &n.snapshots {
            let Some(r) = &sn.snapshot else { continue };
            if have.insert(r.id.clone()) {
                out.nodes.push(Node { id: r.id.clone(), kind: "event".into(), snapshot: true, name: r.name.clone(), path: r.path.clone(), color: r.color.clone(), depth: n.depth - 1, used_by: 1, further: true, ..Default::default() });
            }
            let id = format!("{}>{}", sn.id, r.id);
            if links.insert(id.clone()) {
                out.edges.push(Edge { id, kind: "snapshot".into(), child_id: n.id.clone(), parent_id: r.id.clone(), further: true, ..Default::default() });
            }
        }
    }
    out
}

/// Everything on a path through `pick`: what feeds it and where it goes.
pub fn lit(g: &Graph, pick: &Pick) -> HashSet<String> {
    let mut on = HashSet::new();
    let walk = |start: &str, up: bool, on: &mut HashSet<String>| {
        let mut stack = vec![start.to_string()];
        while let Some(id) = stack.pop() {
            if !on.insert(id.clone()) && id != start {
                continue;
            }
            for e in &g.edges {
                let next = if up && e.child_id == id { &e.parent_id } else if !up && e.parent_id == id { &e.child_id } else { continue };
                if !on.contains(next) {
                    stack.push(next.clone());
                }
            }
        }
    };
    match pick {
        Pick::Node(id) => {
            walk(id, true, &mut on);
            walk(id, false, &mut on);
        }
        Pick::Edge(eid) => {
            if let Some(e) = g.edges.iter().find(|e| &e.id == eid) {
                walk(&e.parent_id, true, &mut on);
                walk(&e.child_id, false, &mut on);
            }
        }
    }
    on
}

/// A few words on what's on a link, for its label.
pub fn edge_badges(e: &Edge) -> Vec<String> {
    let mut b = Vec::new();
    match e.kind.as_str() {
        "send" => {
            if let Some(s) = &e.send {
                b.push(format!("send {}", s.level.map(crate::gain::fmt_db).unwrap_or_default()));
                if s.bypass {
                    b.push("bypassed".into());
                }
            }
            return b;
        }
        "effect" => {
            if e.effect.as_ref().is_some_and(|f| f.bypass) {
                b.push("bypassed".into());
            }
            return b;
        }
        "route" | "vca" | "snapshot" => return b,
        _ => {}
    }
    for c in e.containers.iter().rev() {
        if let Some(k) = &c.container {
            if k != "Scatterer playlist" {
                b.push(k.clone());
            }
        }
    }
    let i = &e.instrument;
    let conds = i.conditions.len() + e.containers.iter().map(|c| c.conditions.len()).sum::<usize>();
    if conds > 0 {
        b.push(format!("{conds} cond"));
    }
    let autos = i.automation.len() + e.track.as_ref().and_then(|t| t.strip.as_ref()).map_or(0, |s| s.automation.len());
    if autos > 0 {
        b.push("auto".into());
    }
    if !i.modulation.is_empty() {
        b.push("mod".into());
    }
    if !i.params.is_empty() {
        b.push(format!("{} param", i.params.len()));
    }
    if let Some(v) = i.volume {
        if v != 0.0 {
            b.push(crate::gain::fmt_db(v));
        }
    }
    b
}

/// The badges that fit on a graph link.
fn short_badges(e: &Edge) -> String {
    let b = edge_badges(e);
    if b.len() > 3 { format!("{} +{}", b[..3].join(" · "), b.len() - 3) } else { b.join(" · ") }
}

/// Pan/zoom to show the selected boxes (and everything, when it fits).
pub fn fit(g: &Graph, side: Side, view: Vec2, expanded: &HashSet<usize>) -> View {
    let l = layout(g, side, expanded, &HashSet::new());
    let zoom = ((view.x - 40.0) / l.size.x).min((view.y - 40.0) / l.size.y).clamp(0.5, 1.0);
    let focus = l.boxes.iter().filter(|(n, _)| n.is_focus).map(|(_, r)| *r).reduce(|a, b| a.union(b));
    let fits = l.size.x * zoom <= view.x - 40.0 && l.size.y * zoom <= view.y - 40.0;
    let centre = match focus {
        Some(r) if !fits => r.center(),
        _ => (l.size / 2.0).to_pos2(),
    };
    View { offset: view / 2.0 - centre.to_vec2() * zoom, zoom, expanded: expanded.clone(), last: HashMap::new() }
}

pub fn show(ui: &mut Ui, g: &Graph, side: Side, selected: Option<&Pick>, view: &mut View) -> Out {
    let mut out = Out::default();
    let area = ui.available_rect_before_wrap();
    let bg = ui.allocate_rect(area, Sense::click_and_drag());
    if bg.clicked() {
        out.clear = true;
    }
    // Pan and zoom.
    if bg.dragged() {
        view.offset += bg.drag_delta();
    }
    if ui.rect_contains_pointer(area) {
        let (zoom_delta, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        if zoom_delta != 1.0 {
            let p = ui.input(|i| i.pointer.hover_pos()).unwrap_or(area.center()) - area.min;
            let z = (view.zoom * zoom_delta).clamp(0.3, 2.5);
            view.offset = p - (p - view.offset) * (z / view.zoom);
            view.zoom = z;
        } else if scroll != Vec2::ZERO {
            view.offset += scroll;
        }
    }
    // A selected box or link lights its paths; the rest dims. Lit boxes are
    // never folded away, so the whole path is drawn.
    let lit = selected.map(|p| lit(g, p));
    let l = layout(g, side, &view.expanded, lit.as_ref().unwrap_or(&HashSet::new()));
    // Hold the clicked box (else the selection) still when the layout shifts.
    let anchor = match selected {
        Some(Pick::Node(id)) => Some(id.clone()),
        Some(Pick::Edge(eid)) => g.edges.iter().find(|e| &e.id == eid).map(|e| e.parent_id.clone()),
        None => None,
    }
    .or_else(|| g.foci().first().map(|n| n.id.clone()));
    view.keep_still(&l, anchor.as_deref());
    // Keep at least PAN_MARGIN of the content in view.
    let (w, h) = (l.size.x * view.zoom, l.size.y * view.zoom);
    let m = PAN_MARGIN.min(area.width() / 2.0).min(area.height() / 2.0);
    view.offset.x = view.offset.x.clamp((m - w).min(area.width() - m), area.width() - m);
    view.offset.y = view.offset.y.clamp((m - h).min(area.height() - m), area.height() - m);
    let z = view.zoom;
    let dim = |id: &str| lit.as_ref().is_some_and(|l| !l.contains(id));
    let (origin, offset) = (area.min, view.offset);
    let to_screen = move |p: Pos2| origin + offset + p.to_vec2() * z;
    let on_screen = move |r: Rect| Rect::from_min_max(to_screen(r.min), to_screen(r.max));
    let at = |id: &str| l.boxes.iter().find(|(n, _)| n.id == id).map(|(_, r)| on_screen(*r));
    let painter = ui.painter_at(area);
    let small = FontId::proportional((10.5 * z).max(7.0));
    let highlight = g.highlight.as_deref();

    // Dot grid.
    let step = 24.0 * z;
    if step >= 8.0 {
        let start = area.min + vec2(offset.x.rem_euclid(step), offset.y.rem_euclid(step));
        let mut y = start.y;
        while y < area.max.y {
            let mut x = start.x;
            while x < area.max.x {
                painter.circle_filled(pos2(x, y), 0.9, Color32::from_gray(48));
                x += step;
            }
            y += step;
        }
    }
    // Column headings, pinned to the top.
    for (x, label) in &l.headings {
        if label.is_empty() {
            continue;
        }
        let sx = to_screen(pos2(*x, 0.0)).x;
        let col = if label == "SELECTED" { Color32::WHITE } else { DIM };
        painter.text(pos2(sx, area.top() + 10.0), Align2::LEFT_TOP, label, FontId::monospace(11.0), col);
    }

    // The further chain as one group: a shaded panel behind all its boxes.
    if let Some(group) = l.boxes.iter().filter(|(n, _)| n.further).map(|(_, r)| on_screen(*r)).reduce(|a, b| a.union(b)) {
        let panel = Rect::from_min_max(group.min - vec2(14.0, 30.0) * z, group.max + vec2(14.0, 14.0) * z);
        painter.rect(panel, 10.0 * z, Color32::from_rgba_unmultiplied(255, 255, 255, 6), Stroke::new(1.0, Color32::from_gray(52)), StrokeKind::Inside);
        painter.text(panel.left_top() + vec2(10.0, 6.0) * z, Align2::LEFT_TOP, "mixer route and snapshots, past the top-level events", FontId::proportional((10.0 * z).max(7.0)), DIM);
    }
    // Links first, under the boxes.
    for e in &g.edges {
        let (Some(c), Some(p)) = (at(&e.child_id), at(&e.parent_id)) else { continue };
        let (a, b) = if c.center().x <= p.center().x { (c.right_center(), p.left_center()) } else { (c.left_center(), p.right_center()) };
        let bend = ((b.x - a.x).abs() * 0.45).max(40.0 * z) * if a.x <= b.x { 1.0 } else { -1.0 };
        let pts = [a, pos2(a.x + bend, a.y), pos2(b.x - bend, b.y), b];
        let is_sel = selected == Some(&Pick::Edge(e.id.clone())) || highlight == Some(e.id.as_str());
        let faded = dim(&e.child_id) || dim(&e.parent_id);
        let col = if is_sel {
            ORANGE
        } else if faded {
            edge_colour(&e.kind).gamma_multiply(0.18)
        } else if lit.is_some() {
            Color32::WHITE
        } else {
            edge_colour(&e.kind)
        };
        painter.add(CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(if is_sel { 2.5 } else { 1.5 }, col)));
        painter.circle_filled(a, 3.5 * z, col);
        painter.circle_filled(b, 3.5 * z, col);
        let mid = bezier_mid(pts);
        if !area.contains(mid) {
            continue;
        }
        let resp = ui.interact(Rect::from_center_size(mid, vec2(14.0, 14.0)), Id::new(("edge", &e.id)), Sense::click());
        painter.circle_filled(mid, if resp.hovered() || is_sel { 6.0 } else { 4.5 }, col);
        let badges = short_badges(e);
        if !badges.is_empty() && z >= 0.6 && !faded {
            painter.text(mid + vec2(0.0, -9.0), Align2::CENTER_BOTTOM, badges, small.clone(), DIM);
        }
        if resp.clicked() {
            out.clicked = Some(Pick::Edge(e.id.clone()));
        }
        let (child, parent) = (g.node(&e.child_id), g.node(&e.parent_id));
        resp.context_menu(|ui| {
            if ui.button("Show link details").clicked() {
                out.clicked = Some(Pick::Edge(e.id.clone()));
                ui.close();
            }
            for n in [child, parent].into_iter().flatten() {
                if ui.button(format!("{}: {}", n.name, kit::open_label(n))).clicked() {
                    out.open = Some(n.open_id());
                    ui.close();
                }
            }
        });
        resp.on_hover_text(format!("{}\nClick for details, right-click for more", link_text(g, e)));
    }
    // Links into folded columns: one line per "+N more" box and visible box,
    // labelled with how many hidden items it stands for.
    // (column, visible box, link kinds, hidden names)
    let mut agg: Vec<(usize, String, Vec<String>, Vec<String>)> = Vec::new();
    for e in &g.edges {
        let (fc, fp) = (l.folded.get(&e.child_id), l.folded.get(&e.parent_id));
        let (col, hidden_id, other) = match (fc, fp) {
            (Some(c), None) => (*c, &e.child_id, &e.parent_id),
            (None, Some(c)) => (*c, &e.parent_id, &e.child_id),
            _ => continue,
        };
        if at(other).is_none() {
            continue;
        }
        let name = g.node(hidden_id).map_or_else(|| hidden_id.clone(), |n| n.name.clone());
        match agg.iter_mut().find(|a| a.0 == col && &a.1 == other) {
            Some(a) => {
                if !a.2.contains(&e.kind) {
                    a.2.push(e.kind.clone());
                }
                if !a.3.contains(&name) {
                    a.3.push(name);
                }
            }
            None => agg.push((col, other.clone(), vec![e.kind.clone()], vec![name])),
        }
    }
    for (col, other, kinds, names) in &agg {
        // One kind: its colour. Mixed: neutral.
        let kind = if kinds.len() == 1 { kinds[0].as_str() } else { "" };
        let (Some(m), Some(o)) = (l.more.iter().find(|m| m.0 == *col).map(|m| on_screen(m.1)), at(other)) else { continue };
        let (a, b) = if m.center().x <= o.center().x { (m.right_center(), o.left_center()) } else { (m.left_center(), o.right_center()) };
        let bend = ((b.x - a.x).abs() * 0.45).max(40.0 * z) * if a.x <= b.x { 1.0 } else { -1.0 };
        let pts = [a, pos2(a.x + bend, a.y), pos2(b.x - bend, b.y), b];
        let col_c = if dim(other) { edge_colour(kind).gamma_multiply(0.18) } else { edge_colour(kind).gamma_multiply(0.7) };
        painter.add(CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, Stroke::new(1.5 + (names.len() as f32).sqrt().min(4.0), col_c)));
        let mid = bezier_mid(pts);
        if !area.contains(mid) {
            continue;
        }
        let resp = ui.interact(Rect::from_center_size(mid, vec2(30.0, 16.0)), Id::new(("more_link", col, other)), Sense::click());
        painter.rect_filled(Rect::from_center_size(mid, vec2(28.0, 14.0)), 7.0, Color32::from_rgb(40, 44, 50));
        painter.text(mid, Align2::CENTER_CENTER, format!("×{}", names.len()), small.clone(), if resp.hovered() { Color32::WHITE } else { DIM });
        if resp.clicked() {
            view.expanded.insert(*col);
        }
        let mut list = names.iter().take(12).cloned().collect::<Vec<_>>().join("\n");
        if names.len() > 12 {
            list += &format!("\n…and {} more", names.len() - 12);
        }
        resp.on_hover_text(format!("{} hidden in the folded column link here:\n{list}\nClick to show them all.", names.len()));
    }
    for (n, r) in &l.boxes {
        let r = on_screen(*r);
        if !area.intersects(r) {
            continue;
        }
        let resp = ui.interact(r.intersect(area), Id::new(("node", &n.id)), Sense::click());
        let is_sel = selected == Some(&Pick::Node(n.id.clone()));
        // The border is the item's colour in Studio, and nothing else.
        let studio = kit::studio_colour(&n.color);
        let mut stroke = match studio {
            Some(c) => Stroke::new(2.0, c),
            None => Stroke::new(1.0, Color32::from_gray(if resp.hovered() { 110 } else if n.further { 52 } else { 66 })),
        };
        let faded = dim(&n.id);
        let mut fill = node_fill(n);
        if resp.hovered() {
            fill = fill.gamma_multiply(1.25);
        }
        if faded {
            stroke.color = stroke.color.gamma_multiply(0.3);
            fill = fill.gamma_multiply(0.5);
        }
        painter.rect(r, 7.0 * z, fill, stroke, StrokeKind::Inside);
        // Selection is an accent bar down the left edge: orange for what's
        // selected in Studio, white for the box you're inspecting.
        let accent = if n.is_focus { Some(ORANGE) } else if is_sel { Some(Color32::WHITE) } else { None };
        if let Some(c) = accent {
            let bar = Rect::from_min_max(r.left_top() + vec2(2.0, 2.0) * z, pos2(r.left() + 6.0 * z, r.bottom() - 2.0 * z));
            painter.rect_filled(bar, 2.0 * z, if faded { c.gamma_multiply(0.3) } else { c });
        }
        draw_card(&painter, g, n, r, z, faded);
        let link = link_toward_focus(g, n);
        if resp.double_clicked() {
            out.open = Some(n.open_id());
        } else if resp.clicked() {
            out.clicked = Some(Pick::Node(n.id.clone()));
        }
        match kit::node_menu(&resp, n, link) {
            Some(MenuAct::Open(id, name)) => out.open = Some((id, name)),
            Some(MenuAct::Select(p)) => out.clicked = Some(p),
            None => {}
        }
        let path = if n.path.is_empty() { n.label.clone() } else { n.path.clone() };
        let colour = if kit::studio_colour(&n.color).is_some() { format!("\nStudio colour: {}", n.color) } else { String::new() };
        resp.on_hover_text(format!("{path}{colour}\nDouble-click: {}. Right-click for more.", kit::open_label(n)));
    }
    // "+N more" boxes for crowded columns.
    for (c, r, hidden, open) in &l.more {
        let r = on_screen(*r);
        if !area.intersects(r) {
            continue;
        }
        let resp = ui.interact(r.intersect(area), Id::new(("more", *c)), Sense::click());
        painter.rect(r, 6.0 * z, Color32::from_rgb(32, 35, 40), Stroke::new(1.0, if resp.hovered() { Color32::WHITE } else { Color32::from_gray(66) }), StrokeKind::Inside);
        let text = if *open { "Show fewer".to_string() } else { format!("+{hidden} more in this column") };
        painter.text(r.left_center() + vec2(10.0 * z, 0.0), Align2::LEFT_CENTER, text, FontId::proportional(12.0 * z), DIM);
        if resp.clicked() {
            if *open {
                view.expanded.remove(c);
            } else {
                view.expanded.insert(*c);
            }
        }
    }
    // Fit button, bottom left.
    let fit_rect = Rect::from_min_size(pos2(area.left() + 8.0, area.bottom() - 30.0), vec2(44.0, 22.0));
    if ui.put(fit_rect, egui::Button::new("Fit").small()).on_hover_text("Show the selection again.\nDrag to pan; pinch or Cmd+scroll to zoom.").clicked() {
        *view = fit(g, side, area.size(), &view.expanded);
    }
    out
}

/// The text inside a box: kind, name, folder, how it links toward the
/// selection, and a line of facts.
fn draw_card(painter: &egui::Painter, g: &Graph, n: &Node, r: Rect, z: f32, faded: bool) {
    let (tag, tag_col) = kit::kind_tag(n);
    let fade = |c: Color32| if faded { c.gamma_multiply(0.35) } else { c };
    let x = r.left() + 13.0 * z;
    let mut y = r.top() + 7.0 * z;
    let head = if n.is_focus { format!("{tag} · SELECTED") } else { tag.to_string() };
    painter.text(pos2(x, y), Align2::LEFT_TOP, head, FontId::monospace(9.5 * z), fade(if n.is_focus { ORANGE } else { tag_col }));
    // Bank status: a pill at the top right.
    if let Some((bank, col)) = kit::bank_fact(n) {
        let galley = painter.layout_no_wrap(clip(&bank, 18), FontId::proportional(9.5 * z), fade(col));
        let size = galley.size() + vec2(10.0, 2.0) * z;
        let pill = Rect::from_min_size(pos2(r.right() - 7.0 * z - size.x, r.top() + 5.0 * z), size);
        painter.rect(pill, 7.0 * z, fade(col.gamma_multiply(0.18)), Stroke::new(1.0, fade(col.gamma_multiply(0.7))), StrokeKind::Inside);
        painter.galley(pill.min + vec2(5.0, 1.0) * z, galley, fade(col));
    }
    y += 14.0 * z;
    painter.text(pos2(x, y), Align2::LEFT_TOP, clip(&n.name, 28), FontId::proportional(14.0 * z), fade(Color32::WHITE));
    y += 19.0 * z;
    if z < 0.6 {
        return;
    }
    let f = kit::folder(&n.path);
    let mut lines: Vec<String> = Vec::new();
    if !f.is_empty() {
        lines.push(f);
    }
    if let Some(e) = link_toward_focus(g, n) {
        lines.push(kit::link_summary(e));
    }
    for line in lines.into_iter().take(2) {
        if line.is_empty() {
            continue;
        }
        painter.text(pos2(x, y), Align2::LEFT_TOP, clip(&line, 36), FontId::proportional(10.5 * z), fade(DIM));
        y += 14.0 * z;
    }
    let facts = kit::node_facts(g, n).join(" · ");
    if !facts.is_empty() {
        painter.text(pos2(x, y), Align2::LEFT_TOP, clip(&facts, 36), FontId::proportional(10.5 * z), fade(DIM));
    }
}

pub fn link_text(g: &Graph, e: &Edge) -> String {
    let (c, p) = (name(g, &e.child_id), name(g, &e.parent_id));
    match e.kind.as_str() {
        "asset" => format!("{p} plays {c}"),
        "route" => format!("{c} routes into {p}"),
        "send" => format!("{} sends to {p}", e.place.clone().unwrap_or_else(|| c.to_string())),
        "effect" => format!("{c} is on {}", e.place.clone().unwrap_or_else(|| p.to_string())),
        "vca" => format!("{p} controls {c}"),
        "snapshot" => format!("{c} starts snapshot {p}"),
        _ => format!("{p} uses {c}"),
    }
}

fn name<'a>(g: &'a Graph, id: &str) -> &'a str {
    g.node(id).map_or("?", |n| n.name.as_str())
}

pub fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

fn bezier_mid(p: [Pos2; 4]) -> Pos2 {
    let t = 0.5f32;
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    pos2(p.iter().zip(w).map(|(q, k)| q.x * k).sum(), p.iter().zip(w).map(|(q, k)| q.y * k).sum())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::fixtures::*;

    fn xs(g: &Graph, side: Side) -> HashMap<String, f32> {
        layout(g, side, &HashSet::new(), &HashSet::new()).boxes.iter().map(|(n, r)| (n.name.clone(), r.min.x)).collect()
    }

    #[test]
    fn signal_flows_left_to_right() {
        // C uses B uses A: A plays into B, B into C.
        let g = focus(EVENT_TOP).graph.unwrap();
        let x = xs(&g, Side::Both);
        assert!(x["A_leaf"] < x["B_mid"] && x["B_mid"] < x["C_top"], "{x:?}");
        let up = focus(EVENT_LEAF).graph.unwrap();
        assert_eq!(layout(&up, Side::Down, &HashSet::new(), &HashSet::new()).boxes.len(), 1, "a leaf has nothing below it");
        let x = xs(&up, Side::Up);
        assert!(x["A_leaf"] < x["B_mid"] && x["B_mid"] < x["C_top"] && x["A_leaf"] < x["E_scatter"], "{x:?}");
    }

    #[test]
    fn assets_and_buses_flow_too() {
        let a = focus(ASSET).graph.unwrap();
        let x = xs(&a, Side::Both);
        assert!(x["tone_short.wav"] < x["A_leaf"] && x["A_leaf"] < x["C_top"], "{x:?}");
        let b = focus(BUS).graph.unwrap();
        let x = xs(&b, Side::Both);
        assert!(x["A_leaf"] < x["Foley"] && x["Foley"] < x["SFX"] && x["SFX"] < x["Master Bus"], "{x:?}");
    }

    #[test]
    fn headings_name_each_column() {
        let g = focus(EVENT_LEAF).graph.unwrap();
        let l = layout(&g, Side::Both, &HashSet::new(), &HashSet::new());
        let h: Vec<&str> = l.headings.iter().map(|h| h.1.as_str()).collect();
        assert_eq!(h, vec!["SELECTED", "REFERENCED IN · 1", "REFERENCED IN · 2"]);
        let b = focus(BUS).graph.unwrap();
        let h: Vec<String> = layout(&b, Side::Both, &HashSet::new(), &HashSet::new()).headings.into_iter().map(|h| h.1).collect();
        assert_eq!(h, vec!["INPUTS · 2", "INPUTS · 1", "SELECTED", "OUTPUTS · 1"]);
    }

    #[test]
    fn crowded_columns_collapse_and_expand() {
        let mut g = focus(EVENT_LEAF).graph.unwrap();
        // 15 more users of A.
        let a = g.focus_id.clone();
        for i in 0..15 {
            let id = format!("x{i}");
            g.nodes.push(Node { id: id.clone(), name: id.clone(), kind: "event".into(), depth: -1, used_by: 0, ..Default::default() });
            g.edges.push(Edge { id: format!("e{i}"), kind: "event".into(), child_id: a.clone(), parent_id: id, ..Default::default() });
        }
        let l = layout(&g, Side::Both, &HashSet::new(), &HashSet::new());
        let (col, _, hidden, open) = l.more[0];
        assert_eq!((col, open), (1, false));
        assert_eq!(l.folded.len(), hidden, "every hidden box is known, for the summary links");
        assert!(l.folded.values().all(|&c| c == 1));
        assert_eq!(l.boxes.iter().filter(|(_, r)| r.min.x == COL_W).count() + hidden, 18);
        let l2 = layout(&g, Side::Both, &HashSet::from([1]), &HashSet::new());
        assert_eq!(l2.boxes.len(), g.nodes.len());
        assert!(l2.more[0].3);
    }

    #[test]
    fn further_chain_continues_to_the_master_bus() {
        let g = with_further_chain(&focus(EVENT_LEAF).graph.unwrap());
        let names: Vec<&str> = g.nodes.iter().filter(|n| n.further).map(|n| n.name.as_str()).collect();
        // C (top-level) routes to the Master Bus; D and E too; C starts the Duck snapshot.
        assert!(names.contains(&"Master Bus") && names.contains(&"Duck"), "{names:?}");
        let x = xs(&g, Side::Both);
        assert!(x["C_top"] < x["Master Bus"] && x["C_top"] < x["Duck"], "{x:?}");
        let l = layout(&g, Side::Both, &HashSet::new(), &HashSet::new());
        assert_eq!(l.headings.iter().filter(|h| h.1 == "FURTHER CHAIN").count(), 1, "{:?}", l.headings);
        // Mixer graphs aren't extended.
        let b = focus(BUS).graph.unwrap();
        assert_eq!(with_further_chain(&b).nodes.len(), b.nodes.len());
    }

    #[test]
    fn a_selected_box_lights_only_its_paths() {
        let g = focus(EVENT_LEAF).graph.unwrap();
        let id = |name: &str| g.nodes.iter().find(|n| n.name == name).unwrap().id.clone();
        let on = lit(&g, &Pick::Node(id("B_mid")));
        assert!(on.contains(&id("A_leaf")) && on.contains(&id("C_top")));
        assert!(!on.contains(&id("D_multi")) && !on.contains(&id("E_scatter")));
    }

    #[test]
    fn a_lit_path_is_never_folded_away() {
        // A is used by B, D, E and 15 more; x14 (last, so folded) is used by "top".
        let mut g = focus(EVENT_LEAF).graph.unwrap();
        let a = g.focus_id.clone();
        for i in 0..15 {
            let id = format!("x{i}");
            g.nodes.push(Node { id: id.clone(), name: id.clone(), kind: "event".into(), depth: -1, used_by: 1, ..Default::default() });
            g.edges.push(Edge { id: format!("e{i}"), kind: "event".into(), child_id: a.clone(), parent_id: id, ..Default::default() });
        }
        g.nodes.push(Node { id: "top".into(), name: "top".into(), kind: "event".into(), depth: -2, ..Default::default() });
        g.edges.push(Edge { id: "up".into(), kind: "event".into(), child_id: "x14".into(), parent_id: "top".into(), ..Default::default() });
        let (none, unpinned): (HashSet<usize>, HashSet<String>) = (HashSet::new(), HashSet::new());
        let folded = layout(&g, Side::Both, &none, &unpinned);
        assert!(!folded.boxes.iter().any(|(n, _)| n.id == "x14"), "x14 starts folded");
        let on = lit(&g, &Pick::Node("top".into()));
        let l = layout(&g, Side::Both, &none, &on);
        assert!(l.boxes.iter().any(|(n, _)| n.id == "x14"), "selecting 'top' shows the hop through x14");
        assert!(l.more.iter().any(|m| m.2 > 0), "the rest of the column stays folded");
    }

    #[test]
    fn the_view_holds_still_when_columns_unfold() {
        // As in a_lit_path_is_never_folded_away: selecting "top" unfolds x14.
        let mut g = focus(EVENT_LEAF).graph.unwrap();
        let a = g.focus_id.clone();
        for i in 0..15 {
            let id = format!("x{i}");
            g.nodes.push(Node { id: id.clone(), name: id.clone(), kind: "event".into(), depth: -1, used_by: 1, ..Default::default() });
            g.edges.push(Edge { id: format!("e{i}"), kind: "event".into(), child_id: a.clone(), parent_id: id, ..Default::default() });
        }
        g.nodes.push(Node { id: "top".into(), name: "top".into(), kind: "event".into(), depth: -2, ..Default::default() });
        g.edges.push(Edge { id: "up".into(), kind: "event".into(), child_id: "x14".into(), parent_id: "top".into(), ..Default::default() });
        let none: HashSet<usize> = HashSet::new();
        // Unfolding the crowded column makes it the tallest, so every column re-centres.
        let unfolded: HashSet<usize> = HashSet::from([1]);
        let before = layout(&g, Side::Both, &none, &HashSet::new());
        let after = layout(&g, Side::Both, &unfolded, &lit(&g, &Pick::Node("top".into())));
        // A box that the unfolding really moves.
        let pos = |l: &Layout, id: &str| l.boxes.iter().find(|(n, _)| n.id == id).map(|(_, r)| r.min);
        let moved = before.boxes.iter().map(|(n, _)| n.id.clone()).find(|id| pos(&after, id).is_some_and(|p| p != pos(&before, id).unwrap())).expect("something moves");
        let mut v = View { zoom: 0.8, ..Default::default() };
        let screen = |v: &View, l: &Layout| v.offset + pos(l, &moved).unwrap().to_vec2() * v.zoom;
        v.keep_still(&before, Some(&moved));
        let at_before = screen(&v, &before);
        v.keep_still(&after, Some(&moved));
        let at_after = screen(&v, &after);
        assert!((at_before - at_after).length() < 0.01, "{moved}: {at_before:?} vs {at_after:?}");
    }

    #[test]
    fn fit_centres_the_selection_when_it_doesnt_all_fit() {
        let g = focus(EVENT_LEAF).graph.unwrap();
        let v = fit(&g, Side::Both, vec2(300.0, 200.0), &HashSet::new());
        let l = layout(&g, Side::Both, &HashSet::new(), &HashSet::new());
        let r = l.boxes.iter().find(|(n, _)| n.is_focus).unwrap().1;
        let c = v.offset + r.center().to_vec2() * v.zoom;
        assert!((c.x - 150.0).abs() < 1.0 && (c.y - 100.0).abs() < 1.0, "{c:?}");
    }
}
