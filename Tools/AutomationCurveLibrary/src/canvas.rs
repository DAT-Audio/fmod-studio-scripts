//! The curve canvas: draws a curve and edits it with the mouse or trackpad.
//!
//! - Drag a point to move it. It stays between its neighbours and inside 0-1.
//! - Drag on empty space to draw a selection box; Shift adds to the selection.
//!   Shift-click toggles a point. Dragging any selected point moves them all:
//!   points that reach an edge (or an unselected neighbour) stop there while
//!   the rest keep going, so a selection squeezes against the edges.
//! - Drag a segment's diamond handle up or down to bend that segment.
//! - Double-click empty space to add a point; double-click a point to delete it.
//! - Right-click for a menu, including segment shapes (linear, curve, S-curve,
//!   and compound shapes built from extra points).
//! - Shift while dragging moves at a tenth of the speed. Cmd/Ctrl snaps to 0.05.
//! - Scroll, middle-drag, Space+drag or Alt+drag to pan; pinch or Cmd/Ctrl+scroll to
//!   zoom; Alt+double-click resets the view.
//!
//! Hit-testing uses where the button went down (`press_origin`), not where
//! the pointer is when egui decides a drag has started (by then it has moved
//! past egui's drag threshold; in 0.1.0 a quick grab often missed the point
//! and panned the view instead).

use crate::curve::{self, SegmentShape};
use crate::theme::{self, pal};
use crate::library::{Point, sort_points};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Key, Mesh, Pos2, Rect, Response, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2,
};
use std::collections::BTreeSet;


const HIT_POINT: f32 = 12.0;
const HIT_HANDLE: f32 = 11.0;
const SNAP: f64 = 0.05;

/// One axis mapped to real units (the Studio target's ranges), for labels only.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Axis {
    pub lo: f64,
    pub hi: f64,
    pub unit: String,
    pub title: String,
}

impl Axis {
    pub fn show(&self, v: f64) -> String {
        let x = self.lo + v * (self.hi - self.lo);
        let span = (self.hi - self.lo).abs();
        let s = if span >= 1000.0 { format!("{x:.0}") } else if span >= 10.0 { format!("{x:.1}") } else { format!("{x:.2}") };
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        if self.unit.is_empty() { s } else { format!("{s} {}", self.unit) }
    }
}

/// Extra context the app sets before drawing: real-unit axes and the curve
/// Studio already has on the target (a faint dashed "ghost").
#[derive(Clone, Debug, Default)]
pub struct Overlay {
    pub x: Option<Axis>,
    pub y: Option<Axis>,
    pub ghost: Option<Vec<Point>>,
}

/// How the value axis is labelled. Points are always stored 0-1; this only
/// changes what the labels and tooltips say.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayRange {
    pub lo: f64,
    pub hi: f64,
    pub unit: String,
}

impl DisplayRange {
    pub fn normalised() -> Self {
        DisplayRange { lo: 0.0, hi: 1.0, unit: String::new() }
    }
    pub fn is_normalised(&self) -> bool {
        self.lo == 0.0 && self.hi == 1.0 && self.unit.is_empty()
    }
    pub fn show(&self, y: f64) -> String {
        if self.is_normalised() {
            return format!("{y:.4}");
        }
        let v = self.lo + y * (self.hi - self.lo);
        let unit = if self.unit.is_empty() { String::new() } else { format!(" {}", self.unit) };
        format!("{v:.2}{unit}")
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    None,
    /// Moving the selected points: (index, start x, start y), from `pointer`.
    Points { start: Vec<(usize, f64, f64)>, pointer: Pos2 },
    Bend { segment: usize },
    Marquee { from: Pos2, to: Pos2, add: bool },
    Pan,
}

/// View and interaction state that lives across frames.
pub struct Canvas {
    /// Visible data range: x0, x1, y0, y1.
    pub view: [f64; 4],
    drag: Drag,
    /// The plot area last frame (for tests and screen/data conversion).
    pub plot_rect: Rect,
    /// Selected points (the primary one is the caller's `selected`).
    pub selection: BTreeSet<usize>,
    /// Real-unit axes and a ghost curve (set by the app each frame).
    pub overlay: Overlay,
    /// The pointer is over the canvas, or it has keyboard focus: point
    /// shortcuts (Delete, arrows) only act then.
    pub active: bool,
    /// When the pointer last touched the graph (for the automatic reset).
    last_touch: Option<std::time::Instant>,
}

/// What the canvas did this frame, for the app to act on.
#[derive(Default)]
pub struct CanvasOutput {
    /// An edit is about to start (take an undo snapshot first).
    pub begin_edit: bool,
    /// Points changed this frame.
    pub changed: bool,
    /// A drag or a one-shot edit finished.
    pub finished: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas {
            view: Self::FIT,
            drag: Drag::None,
            plot_rect: Rect::NOTHING,
            selection: BTreeSet::new(),
            overlay: Overlay::default(),
            active: false,
            last_touch: None,
        }
    }
}

impl Canvas {
    pub const FIT: [f64; 4] = [-0.03, 1.03, -0.05, 1.05];

    pub fn fit(&mut self) {
        self.view = Self::FIT;
    }

    /// Zoomed or panned away from the whole curve.
    pub fn is_zoomed(&self) -> bool {
        self.view != Self::FIT
    }

    /// Eases the view back to the whole curve once the pointer has been off
    /// the graph for `after`: a peek at a detail doesn't need a Reset zoom,
    /// and it never moves while you're working on the graph.
    pub fn auto_fit(&mut self, ctx: &egui::Context, after: std::time::Duration) {
        if !self.is_zoomed() || self.active {
            return;
        }
        let Some(t) = self.last_touch else { return };
        let left = after.saturating_sub(t.elapsed());
        if !left.is_zero() {
            ctx.request_repaint_after(left);
            return;
        }
        let mut done = true;
        for (v, f) in self.view.iter_mut().zip(Self::FIT) {
            *v += (f - *v) * 0.25;
            if (*v - f).abs() > 1e-3 {
                done = false;
            }
        }
        if done {
            self.view = Self::FIT;
        } else {
            ctx.request_repaint();
        }
    }

    /// Selects one point (or none), dropping any multi-selection.
    pub fn select_only(&mut self, i: Option<usize>) {
        self.selection = i.into_iter().collect();
    }

    pub fn to_screen(&self, r: Rect, x: f64, y: f64) -> Pos2 {
        let [x0, x1, y0, y1] = self.view;
        Pos2::new(
            r.left() + ((x - x0) / (x1 - x0)) as f32 * r.width(),
            r.bottom() - ((y - y0) / (y1 - y0)) as f32 * r.height(),
        )
    }

    fn to_data(&self, r: Rect, p: Pos2) -> (f64, f64) {
        let [x0, x1, y0, y1] = self.view;
        (
            x0 + ((p.x - r.left()) / r.width()) as f64 * (x1 - x0),
            y0 + ((r.bottom() - p.y) / r.height()) as f64 * (y1 - y0),
        )
    }

    /// Bend handles on the curve: a segment's midpoint, or for an S-curve its
    /// quarter point (an S always passes through the middle; its strength
    /// shows at the quarter).
    fn handles(&self, r: Rect, pts: &[Point]) -> Vec<(usize, Pos2)> {
        let mut out = Vec::new();
        for i in 0..pts.len().saturating_sub(1) {
            let (a, b) = (&pts[i], &pts[i + 1]);
            if b.x - a.x < 1e-6 || (b.y - a.y).abs() < 1e-6 {
                continue;
            }
            let hx = a.x + (b.x - a.x) * if a.s { 0.25 } else { 0.5 };
            out.push((i, self.to_screen(r, hx, curve::value_at(pts, hx))));
        }
        out
    }

    fn hit_point(&self, r: Rect, pts: &[Point], p: Pos2) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, q) in pts.iter().enumerate() {
            let d = self.to_screen(r, q.x, q.y).distance(p);
            if d <= HIT_POINT && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    fn hit_handle(&self, r: Rect, pts: &[Point], p: Pos2) -> Option<usize> {
        self.handles(r, pts).into_iter().find(|(_, h)| h.distance(p) <= HIT_HANDLE).map(|(i, _)| i)
    }

    /// The segment under a screen position (by x), if any.
    fn segment_at(&self, r: Rect, pts: &[Point], p: Pos2) -> Option<usize> {
        if let Some(i) = self.hit_handle(r, pts, p) {
            return Some(i);
        }
        let (x, _) = self.to_data(r, p);
        (0..pts.len().saturating_sub(1)).find(|&i| x >= pts[i].x && x <= pts[i + 1].x)
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        pts: &mut Vec<Point>,
        selected: &mut Option<usize>,
        display: &DisplayRange,
        editable: bool,
    ) -> CanvasOutput {
        let mut out = CanvasOutput::default();
        self.selection.retain(|&i| i < pts.len());
        if let Some(s) = *selected
            && s < pts.len()
            && self.selection.is_empty()
        {
            self.selection.insert(s);
        }
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        self.active = response.hovered() || response.has_focus() || response.dragged();
        if response.hovered() || response.dragged() {
            self.last_touch = Some(std::time::Instant::now());
        }
        let outer = response.rect;
        // Room for axis labels and titles on the left, top and bottom.
        let r = Rect::from_min_max(outer.min + Vec2::new(62.0, 24.0), outer.max - Vec2::new(16.0, 44.0));
        if r.width() < 40.0 || r.height() < 40.0 {
            return out;
        }
        self.plot_rect = r;

        self.handle_view_input(ui, &response, r);
        if editable {
            self.handle_edit_input(ui, &response, r, pts, selected, &mut out);
        } else {
            self.handle_view_only_input(ui, &response, r, pts, selected);
        }

        // ── Drawing ─────────────────────────────────────────────────────────
        painter.rect_filled(outer, CornerRadius::ZERO, pal().bg);
        let clip = painter.with_clip_rect(r);
        clip.rect_filled(r, CornerRadius::ZERO, pal().plot);
        self.draw_grid(&painter, &clip, r, display);

        if let Some(ghost) = self.overlay.ghost.clone().filter(|g| g.len() >= 2) {
            self.draw_ghost(&clip, r, &ghost);
        }
        if !pts.is_empty() {
            self.draw_curve(&clip, r, pts);
            for (i, h) in self.handles(r, pts) {
                let active = matches!(self.drag, Drag::Bend { segment } if segment == i);
                let s = if active { 7.5 } else { 6.0 };
                let diamond = vec![h + Vec2::new(0.0, -s), h + Vec2::new(s, 0.0), h + Vec2::new(0.0, s), h + Vec2::new(-s, 0.0)];
                clip.add(Shape::convex_polygon(diamond, if active { pal().point_selected } else { pal().plot }, Stroke::new(1.5, pal().curve)));
            }
            for (i, q) in pts.iter().enumerate() {
                let c = self.to_screen(r, q.x, q.y);
                if Some(i) == *selected {
                    // The point the Points tab and the keys act on.
                    if pal().square_points {
                        theme::point(&painter, c, 6.5, pal().point, Stroke::new(2.0, pal().point_selected));
                    } else {
                        theme::point(&painter, c, 7.0, pal().plot, Stroke::new(2.5, pal().point_selected));
                    }
                } else if self.selection.contains(&i) {
                    theme::point(&painter, c, 5.5, pal().point_selected, Stroke::NONE);
                } else {
                    theme::point(&painter, c, 4.5, pal().point, Stroke::NONE);
                }
            }
        }
        if let Drag::Marquee { from, to, .. } = self.drag {
            let rect = Rect::from_two_pos(from, to);
            clip.rect_filled(rect, CornerRadius::ZERO, pal().marquee);
            clip.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0, pal().point_selected), StrokeKind::Inside);
        }
        painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, pal().border), StrokeKind::Outside);

        // Selection count, top-left inside the plot.
        if self.selection.len() > 1 {
            painter.text(r.left_top() + Vec2::new(8.0, 6.0), Align2::LEFT_TOP, format!("{} points selected", self.selection.len()), FontId::proportional(12.5), pal().point_selected);
        }
        if self.overlay.ghost.is_some() {
            painter.text(r.right_top() + Vec2::new(-8.0, 6.0), Align2::RIGHT_TOP, "- - -  curve in Studio now", FontId::proportional(11.5), pal().ghost);
        }

        // Hover readout, beside the pointer (flipped near the edges).
        if let Some(hp) = response.hover_pos()
            && r.contains(hp)
        {
            let (x, y) = self.to_data(r, hp);
            let tip = match self.hit_point(r, pts, hp) {
                Some(i) => format!("Point {}:  {} , {}", i + 1, self.fmt_x(pts[i].x), self.fmt_y(pts[i].y, display)),
                None if self.hit_handle(r, pts, hp).is_some() => "Drag to bend this segment".to_string(),
                None => format!("{},  {}", self.fmt_x(x), self.fmt_y(y, display)),
            };
            let galley = painter.layout_no_wrap(tip, FontId::monospace(12.0), Color32::from_rgb(0xe8, 0xe8, 0xec));
            let size = galley.size() + Vec2::new(10.0, 6.0);
            let mut at = hp + Vec2::new(16.0, 16.0);
            if at.x + size.x > r.right() {
                at.x = hp.x - 16.0 - size.x;
            }
            if at.y + size.y > r.bottom() {
                at.y = hp.y - 16.0 - size.y;
            }
            let bg = Rect::from_min_size(at, size);
            painter.rect_filled(bg, CornerRadius::same(4), Color32::from_rgba_unmultiplied(0x10, 0x10, 0x12, 225));
            painter.galley(at + Vec2::new(5.0, 3.0), galley, pal().label);
        }
        out
    }

    fn handle_view_input(&mut self, ui: &Ui, response: &Response, r: Rect) {
        let Some(hp) = response.hover_pos() else { return };
        let (zoom, scroll) = ui.ctx().input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        let [x0, x1, y0, y1] = self.view;
        if (zoom - 1.0).abs() > 1e-4 {
            let (cx, cy) = self.to_data(r, hp);
            let f = 1.0 / zoom as f64;
            let w = ((x1 - x0) * f).clamp(0.01, 4.0);
            let h = ((y1 - y0) * f).clamp(0.01, 4.0);
            let tx = (cx - x0) / (x1 - x0);
            let ty = (cy - y0) / (y1 - y0);
            self.view = [cx - tx * w, cx - tx * w + w, cy - ty * h, cy - ty * h + h];
        } else if scroll != Vec2::ZERO {
            let dx = -(scroll.x / r.width()) as f64 * (x1 - x0);
            let dy = (scroll.y / r.height()) as f64 * (y1 - y0);
            self.view = [x0 + dx, x1 + dx, y0 + dy, y1 + dy];
        }
        if response.double_clicked() && ui.ctx().input(|i| i.modifiers.alt) {
            self.fit();
        }
    }

    fn pan_by(&mut self, r: Rect, d: Vec2) {
        let [x0, x1, y0, y1] = self.view;
        let dx = -(d.x / r.width()) as f64 * (x1 - x0);
        let dy = (d.y / r.height()) as f64 * (y1 - y0);
        self.view = [x0 + dx, x1 + dx, y0 + dy, y1 + dy];
    }

    /// Read-only curves (Factory): select points, pan and zoom, nothing else.
    fn handle_view_only_input(&mut self, ui: &mut Ui, response: &Response, r: Rect, pts: &[Point], selected: &mut Option<usize>) {
        let (origin, pressed, space) = ui.ctx().input(|i| (i.pointer.press_origin(), i.pointer.primary_pressed(), i.key_down(Key::Space)));
        if pressed
            && response.hovered()
            && let Some(o) = origin
            && let Some(i) = self.hit_point(r, pts, o)
        {
            *selected = Some(i);
            self.select_only(Some(i));
        }
        if response.drag_started() {
            let alt = ui.ctx().input(|i| i.modifiers.alt);
            self.drag = if response.dragged_by(egui::PointerButton::Middle) || space || alt { Drag::Pan } else { Drag::None };
        }
        if response.dragged() && self.drag == Drag::Pan {
            self.pan_by(r, response.drag_delta());
        }
        if response.drag_stopped() {
            self.drag = Drag::None;
        }
        response.context_menu(|ui| {
            ui.label(RichText::new("Read-only: copy it to one of your libraries to edit.").small());
            if ui.button("Reset zoom").clicked() {
                self.fit();
                ui.close();
            }
        });
    }

    fn handle_edit_input(
        &mut self,
        ui: &mut Ui,
        response: &Response,
        r: Rect,
        pts: &mut Vec<Point>,
        selected: &mut Option<usize>,
        out: &mut CanvasOutput,
    ) {
        let (mods, origin, pressed, space) =
            ui.ctx().input(|i| (i.modifiers, i.pointer.press_origin(), i.pointer.primary_pressed(), i.key_down(Key::Space)));

        // Press: select (Shift toggles), so the point under the button is
        // selected at once and a group stays selected for dragging.
        if pressed
            && response.hovered()
            && let Some(o) = origin
        {
            if let Some(i) = self.hit_point(r, pts, o) {
                if mods.shift {
                    if !self.selection.remove(&i) {
                        self.selection.insert(i);
                    }
                    *selected = Some(i);
                } else if !self.selection.contains(&i) {
                    self.select_only(Some(i));
                    *selected = Some(i);
                } else {
                    *selected = Some(i);
                }
            } else if let Some(seg) = self.hit_handle(r, pts, o) {
                *selected = Some(seg);
                self.select_only(Some(seg));
            }
        }

        // Start of a drag: decide what is being dragged, from where the
        // button went down.
        if response.drag_started() {
            let o = origin.or(response.interact_pointer_pos()).unwrap_or_default();
            if response.dragged_by(egui::PointerButton::Middle) || space || mods.alt {
                self.drag = Drag::Pan;
            } else if let Some(i) = self.hit_point(r, pts, o) {
                if !self.selection.contains(&i) {
                    self.select_only(Some(i));
                    *selected = Some(i);
                }
                let start = self.selection.iter().filter(|&&k| k < pts.len()).map(|&k| (k, pts[k].x, pts[k].y)).collect();
                self.drag = Drag::Points { start, pointer: o };
                out.begin_edit = true;
            } else if let Some(seg) = self.hit_handle(r, pts, o) {
                *selected = Some(seg);
                self.drag = Drag::Bend { segment: seg };
                out.begin_edit = true;
            } else {
                self.drag = Drag::Marquee { from: o, to: o, add: mods.shift };
            }
        }

        // Cursor: a hand over anything draggable.
        let cursor = match self.drag {
            Drag::Points { .. } | Drag::Bend { .. } => Some(CursorIcon::Grabbing),
            Drag::Pan => Some(CursorIcon::Move),
            Drag::Marquee { .. } => Some(CursorIcon::Crosshair),
            Drag::None => response.hover_pos().and_then(|h| {
                (self.hit_point(r, pts, h).is_some() || self.hit_handle(r, pts, h).is_some()).then_some(CursorIcon::Grab)
            }),
        };
        if let Some(c) = cursor {
            ui.ctx().set_cursor_icon(c);
        }

        if response.dragged() {
            let p = response.interact_pointer_pos().unwrap_or_default();
            match self.drag.clone() {
                Drag::Points { start, pointer } => {
                    let (sx, sy) = self.to_data(r, pointer);
                    let (px, py) = self.to_data(r, p);
                    let fine = if mods.shift { 0.1 } else { 1.0 };
                    let (mut dx, mut dy) = ((px - sx) * fine, (py - sy) * fine);
                    if mods.command && let Some(&(_, x0, y0)) = start.first() {
                        dx = ((x0 + dx) / SNAP).round() * SNAP - x0;
                        dy = ((y0 + dy) / SNAP).round() * SNAP - y0;
                    }
                    move_group(pts, &start, dx, dy);
                    out.changed = true;
                }
                Drag::Bend { segment } if segment + 1 < pts.len() => {
                    let seg = segment;
                    let (_, py) = self.to_data(r, p);
                    let (a, b) = (pts[seg].y, pts[seg + 1].y);
                    if (b - a).abs() > 1e-9 {
                        let f = (py - a) / (b - a);
                        pts[seg].shape = if pts[seg].s { curve::s_strength_for_quarter(f) } else { curve::bend_for_midpoint(f) };
                        if mods.command {
                            pts[seg].shape = (pts[seg].shape * 20.0).round() / 20.0;
                        }
                        out.changed = true;
                    }
                }
                Drag::Marquee { from, add, .. } => self.drag = Drag::Marquee { from, to: p, add },
                Drag::Pan => {
                    let d = response.drag_delta();
                    self.pan_by(r, d);
                }
                _ => {}
            }
        }

        if response.drag_stopped() {
            match std::mem::replace(&mut self.drag, Drag::None) {
                Drag::Points { .. } | Drag::Bend { .. } => out.finished = true,
                Drag::Marquee { from, to, add } => {
                    let rect = Rect::from_two_pos(from, to);
                    if !add {
                        self.selection.clear();
                    }
                    for (i, q) in pts.iter().enumerate() {
                        if rect.contains(self.to_screen(r, q.x, q.y)) {
                            self.selection.insert(i);
                        }
                    }
                    *selected = self.selection.iter().next().copied().or(*selected);
                }
                _ => {}
            }
        }

        // A click on empty space clears the selection (Shift keeps it).
        if response.clicked()
            && !mods.shift
            && let Some(p) = response.interact_pointer_pos()
            && self.hit_point(r, pts, p).is_none()
            && self.hit_handle(r, pts, p).is_none()
        {
            self.select_only(*selected);
        }

        if response.double_clicked()
            && !mods.alt
            && let Some(p) = response.interact_pointer_pos()
            && r.contains(p)
        {
            if let Some(i) = self.hit_point(r, pts, p) {
                if pts.len() > 2 {
                    out.begin_edit = true;
                    pts.remove(i);
                    *selected = Some(i.min(pts.len() - 1));
                    self.select_only(*selected);
                    out.changed = true;
                    out.finished = true;
                }
            } else {
                let (x, y) = self.to_data(r, p);
                out.begin_edit = true;
                *selected = Some(insert_point(pts, x, y));
                self.select_only(*selected);
                out.changed = true;
                out.finished = true;
            }
        }

        // Context menu.
        let menu_pos = response.interact_pointer_pos();
        response.context_menu(|ui| {
            if let Some(i) = menu_pos.and_then(|p| self.hit_point(r, pts, p))
                && !self.selection.contains(&i)
            {
                *selected = Some(i);
                self.select_only(Some(i));
            }
            let seg = menu_pos.and_then(|p| self.segment_at(r, pts, p)).or(selected.filter(|&i| i + 1 < pts.len()));
            if ui.button("Add point here").clicked() {
                if let Some(p) = menu_pos {
                    let (x, y) = self.to_data(r, p);
                    out.begin_edit = true;
                    *selected = Some(insert_point(pts, x, y));
                    self.select_only(*selected);
                    out.changed = true;
                    out.finished = true;
                }
                ui.close();
            }
            let n_sel = self.selection.len();
            let del_label = if n_sel > 1 { format!("Delete {n_sel} points") } else { "Delete point".to_string() };
            if ui.add_enabled(n_sel > 0 && pts.len() > 2, egui::Button::new(del_label)).clicked() {
                out.begin_edit = true;
                delete_selected(pts, &self.selection);
                *selected = Some(selected.unwrap_or(0).min(pts.len() - 1));
                self.select_only(*selected);
                out.changed = true;
                out.finished = true;
                ui.close();
            }
            if ui.button("Select all").clicked() {
                self.selection = (0..pts.len()).collect();
                ui.close();
            }
            ui.separator();
            ui.add_enabled_ui(seg.is_some(), |ui| {
                ui.menu_button("Segment shape", |ui| {
                    for (shape, name) in SegmentShape::all() {
                        if shape == SegmentShape::Steps(2) || shape == SegmentShape::Wave(1) {
                            ui.separator();
                        }
                        let label = if shape.is_native() { name.to_string() } else { format!("{name}  (adds points)") };
                        if ui.button(label).clicked() {
                            if let Some(i) = seg {
                                out.begin_edit = true;
                                *pts = curve::reshape_segment(pts, i, shape);
                                *selected = Some(i);
                                self.select_only(Some(i));
                                out.changed = true;
                                out.finished = true;
                            }
                            ui.close();
                        }
                    }
                });
            });
            ui.separator();
            if ui.button("Reset zoom").clicked() {
                self.fit();
                ui.close();
            }
        });
    }

    fn fmt_x(&self, x: f64) -> String {
        match &self.overlay.x {
            Some(a) => a.show(x),
            None => format!("{x:.4}"),
        }
    }

    fn fmt_y(&self, y: f64, display: &DisplayRange) -> String {
        match &self.overlay.y {
            Some(a) => a.show(y),
            None => display.show(y),
        }
    }

    fn draw_ghost(&self, clip: &egui::Painter, r: Rect, ghost: &[Point]) {
        let xs0 = self.view[0].max(0.0);
        let xs1 = self.view[1].min(1.0);
        if xs1 <= xs0 {
            return;
        }
        let n = ((self.to_screen(r, xs1, 0.0).x - self.to_screen(r, xs0, 0.0).x) / 3.0).ceil().max(2.0) as usize;
        let line: Vec<Pos2> = (0..=n)
            .map(|i| {
                let x = xs0 + (xs1 - xs0) * i as f64 / n as f64;
                self.to_screen(r, x, curve::value_at(ghost, x))
            })
            .collect();
        clip.extend(Shape::dashed_line(&line, Stroke::new(1.5, pal().ghost), 6.0, 5.0));
        // Its points: hollow rings, so they read as "already there", not editable.
        for q in ghost {
            if q.x < xs0 - 1e-9 || q.x > xs1 + 1e-9 {
                continue;
            }
            let c = self.to_screen(r, q.x, q.y);
            theme::point(clip, c, 4.5, pal().plot, Stroke::new(1.5, pal().ghost));
        }
    }

    fn grid_step(&self, r: Rect) -> f64 {
        // Smallest of 0.1 / 0.05 / 0.01 / 0.005 that leaves ≥ 28 px between lines.
        let px_per_unit = r.width() as f64 / (self.view[1] - self.view[0]);
        [0.005, 0.01, 0.05, 0.1, 0.25].into_iter().find(|s| s * px_per_unit >= 28.0).unwrap_or(0.25)
    }

    fn draw_grid(&self, painter: &egui::Painter, clip: &egui::Painter, r: Rect, display: &DisplayRange) {
        let step = self.grid_step(r);
        let [x0, x1, y0, y1] = self.view;
        let font = FontId::proportional(11.0);
        let decimals = if step < 0.01 { 3 } else if step < 0.1 { 2 } else { 1 };
        let mut v = (x0.max(0.0) / step).ceil() * step;
        while v <= x1.min(1.0) + 1e-9 {
            let p = self.to_screen(r, v, 0.0);
            let major = ((v * 10.0).round() - v * 10.0).abs() < 1e-6;
            clip.vline(p.x, r.y_range(), Stroke::new(1.0, if major { pal().grid_major } else { pal().grid }));
            let label = match &self.overlay.x {
                Some(a) => a.show(v),
                None => format!("{v:.decimals$}"),
            };
            painter.text(Pos2::new(p.x, r.bottom() + 4.0), Align2::CENTER_TOP, label, font.clone(), pal().label);
            v += step;
        }
        // Axis titles.
        let x_title = self.overlay.x.as_ref().map_or("Position".to_string(), |a| a.title.clone());
        let y_title = self.overlay.y.as_ref().map_or("Value".to_string(), |a| a.title.clone());
        painter.text(Pos2::new(r.center().x, r.bottom() + 22.0), Align2::CENTER_TOP, x_title, FontId::proportional(12.0), pal().title);
        painter.text(Pos2::new(r.left() - 56.0, r.top() - 20.0), Align2::LEFT_TOP, y_title, FontId::proportional(12.0), pal().title);
        let ystep = {
            let px = r.height() as f64 / (y1 - y0);
            [0.005, 0.01, 0.05, 0.1, 0.25].into_iter().find(|s| s * px >= 22.0).unwrap_or(0.25)
        };
        let ydec = if ystep < 0.01 { 3 } else if ystep < 0.1 { 2 } else { 1 };
        let mut v = (y0.max(0.0) / ystep).ceil() * ystep;
        while v <= y1.min(1.0) + 1e-9 {
            let p = self.to_screen(r, 0.0, v);
            let major = ((v * 10.0).round() - v * 10.0).abs() < 1e-6;
            clip.hline(r.x_range(), p.y, Stroke::new(1.0, if major { pal().grid_major } else { pal().grid }));
            let label = match &self.overlay.y {
                Some(a) => a.show(v),
                None if display.is_normalised() => format!("{v:.ydec$}"),
                None => display.show(v),
            };
            painter.text(Pos2::new(r.left() - 6.0, p.y), Align2::RIGHT_CENTER, label, font.clone(), pal().label);
            v += ystep;
        }
        // The 0-1 box, so the editable area is clear when zoomed out.
        let a = self.to_screen(r, 0.0, 0.0);
        let b = self.to_screen(r, 1.0, 1.0);
        clip.rect_stroke(Rect::from_two_pos(a, b), CornerRadius::ZERO, Stroke::new(1.0, pal().border), StrokeKind::Middle);
    }

    fn draw_curve(&self, clip: &egui::Painter, r: Rect, pts: &[Point]) {
        // Sample every 2 px across the visible part of 0-1.
        let xs0 = self.view[0].max(0.0);
        let xs1 = self.view[1].min(1.0);
        if xs1 <= xs0 {
            return;
        }
        let a = self.to_screen(r, xs0, 0.0).x;
        let b = self.to_screen(r, xs1, 0.0).x;
        let n = (((b - a) / 2.0).ceil() as usize).max(2);
        let mut xs: Vec<f64> = (0..=n).map(|i| xs0 + (xs1 - xs0) * i as f64 / n as f64).collect();
        // Include the points themselves (both sides) so corners and steps stay sharp.
        for p in pts.iter().filter(|p| p.x > xs0 && p.x < xs1) {
            xs.push(p.x);
        }
        xs.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
        let line: Vec<Pos2> = xs.iter().map(|&x| self.to_screen(r, x, curve::value_at(pts, x))).collect();
        let base = self.to_screen(r, 0.0, 0.0).y.min(r.bottom());
        let fill = Color32::from_rgba_unmultiplied(pal().curve.r(), pal().curve.g(), pal().curve.b(), pal().curve_fill_alpha);
        let mut mesh = Mesh::default();
        for (i, p) in line.iter().enumerate() {
            mesh.colored_vertex(*p, fill);
            mesh.colored_vertex(Pos2::new(p.x, base), fill);
            if i > 0 {
                let k = (i * 2) as u32;
                mesh.add_triangle(k - 2, k - 1, k);
                mesh.add_triangle(k - 1, k + 1, k);
            }
        }
        clip.add(Shape::mesh(mesh));
        clip.line(line, Stroke::new(2.0, pal().curve));
    }
}

/// Moves a group of points by (dx, dy) from their start positions. Each point
/// stays inside 0-1 and between its nearest *unselected* neighbours, so a
/// group that runs into an edge squeezes against it instead of stopping.
pub fn move_group(pts: &mut [Point], start: &[(usize, f64, f64)], dx: f64, dy: f64) {
    let moving: BTreeSet<usize> = start.iter().map(|s| s.0).collect();
    for &(i, x0, y0) in start {
        if i >= pts.len() {
            continue;
        }
        let lo = (0..i).rev().find(|k| !moving.contains(k)).map_or(0.0, |k| pts[k].x);
        let hi = (i + 1..pts.len()).find(|k| !moving.contains(k)).map_or(1.0, |k| pts[k].x);
        pts[i].x = (x0 + dx).clamp(lo, hi);
        pts[i].y = (y0 + dy).clamp(0.0, 1.0);
    }
    // Clamping can only make neighbours equal, never swap them; keep it tidy.
    sort_points(pts);
}

/// Deletes the selected points, always keeping at least two.
pub fn delete_selected(pts: &mut Vec<Point>, selection: &BTreeSet<usize>) {
    for &i in selection.iter().rev() {
        if pts.len() <= 2 {
            break;
        }
        if i < pts.len() {
            pts.remove(i);
        }
    }
}

/// Inserts a point at (x, y), clamped to 0-1, and returns its index. The new
/// point takes no bend, and the segment it splits keeps its bend on its first
/// point.
pub fn insert_point(pts: &mut Vec<Point>, x: f64, y: f64) -> usize {
    let x = x.clamp(0.0, 1.0);
    let y = y.clamp(0.0, 1.0);
    pts.push(Point { x, y, shape: 0.0, s: false });
    sort_points(pts);
    pts.iter().position(|p| p.x == x && p.y == y).unwrap_or(0)
}

/// Mouse-level tests: press, move and release sequences replayed through egui,
/// covering the 0.1.0 bug where quick or edge grabs panned the view.
#[cfg(test)]
mod mouse_tests {
    #[test]
    fn auto_fit_waits_for_the_pointer_to_leave_then_eases_back() {
        let ctx = egui::Context::default();
        let mut c = Canvas::default();
        c.view = [0.2, 0.4, 0.2, 0.4];
        c.last_touch = Some(std::time::Instant::now());
        c.auto_fit(&ctx, std::time::Duration::from_secs(2));
        assert!(c.is_zoomed(), "not before 2 s");
        c.last_touch = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(3));
        c.active = true;
        c.auto_fit(&ctx, std::time::Duration::from_secs(2));
        assert!(c.is_zoomed(), "never while the pointer is on the graph");
        c.active = false;
        for _ in 0..60 {
            c.auto_fit(&ctx, std::time::Duration::from_secs(2));
        }
        assert!(!c.is_zoomed(), "back to the whole curve");
    }

    use super::*;
    use eframe::egui::{Event, Modifiers, MouseWheelUnit, PointerButton, TouchPhase};
    use egui_kittest::Harness;

    struct St {
        canvas: Canvas,
        pts: Vec<Point>,
        sel: Option<usize>,
        editable: bool,
    }

    fn p(x: f64, y: f64) -> Point {
        Point { x, y, shape: 0.0, s: false }
    }

    /// A four-point test curve (the last point at 0.75).
    fn user_curve() -> Vec<Point> {
        vec![p(0.0927, 0.4088), p(0.2368, 0.3054), Point { x: 0.3532, y: 0.6697, shape: -0.4, s: false }, p(0.75, 0.0)]
    }

    fn harness(pts: Vec<Point>) -> Harness<'static, St> {
        // 60 fps steps: the harness default (0.25 s) is longer than egui's
        // 0.3 s double-click window once two frames pass between taps.
        let mut h = Harness::builder().with_size(Vec2::new(900.0, 600.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, st: &mut St| {
                st.canvas.show(ui, &mut st.pts, &mut st.sel, &DisplayRange::normalised(), st.editable);
            },
            St { canvas: Canvas::default(), pts, sel: None, editable: true },
        );
        h.step();
        h
    }

    fn screen(h: &Harness<'static, St>, x: f64, y: f64) -> Pos2 {
        let st = h.state();
        st.canvas.to_screen(st.canvas.plot_rect, x, y)
    }

    fn button(h: &mut Harness<'static, St>, pos: Pos2, b: PointerButton, pressed: bool) {
        h.event(Event::PointerButton { pos, button: b, pressed, modifiers: Modifiers::NONE });
        h.step();
    }

    /// Press at `from`, move to `to` in `steps` moves, release.
    fn drag(h: &mut Harness<'static, St>, from: Pos2, to: Pos2, steps: usize) {
        h.event(Event::PointerMoved(from));
        h.step();
        button(h, from, PointerButton::Primary, true);
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            h.event(Event::PointerMoved(from + (to - from) * t));
            h.step();
        }
        button(h, to, PointerButton::Primary, false);
    }

    #[test]
    fn quick_grab_near_edge_moves_the_point_not_the_view() {
        let mut h = harness(user_curve());
        let at = screen(&h, 0.3532, 0.6697) + Vec2::new(9.0, 3.0); // near the edge of the hit circle
        drag(&mut h, at, at + Vec2::new(30.0, -40.0), 1); // one big jump, like a fast flick
        let st = h.state();
        assert_eq!(st.canvas.view, Canvas::FIT, "view must not pan");
        assert!(st.pts[2].y > 0.6697 + 0.02, "point should have moved up: {:?}", st.pts[2]);
        assert_eq!(st.sel, Some(2));
    }

    #[test]
    fn rightmost_point_drags() {
        let mut h = harness(user_curve());
        let at = screen(&h, 0.75, 0.0);
        drag(&mut h, at, at + Vec2::new(-60.0, -80.0), 6);
        let st = h.state();
        assert_eq!(st.canvas.view, Canvas::FIT);
        assert!(st.pts[3].x < 0.75 && st.pts[3].y > 0.05, "{:?}", st.pts[3]);
    }

    #[test]
    fn dragging_another_point_than_the_selected_one() {
        let mut h = harness(user_curve());
        h.state_mut().sel = Some(1);
        let at = screen(&h, 0.3532, 0.6697);
        drag(&mut h, at, at + Vec2::new(0.0, 50.0), 5);
        let st = h.state();
        assert_eq!(st.sel, Some(2), "pressing a point selects it");
        assert!(st.pts[2].y < 0.6697 - 0.02, "{:?}", st.pts[2]);
        assert_eq!(st.pts[1], user_curve()[1], "the previously selected point stays put");
        assert_eq!(st.canvas.view, Canvas::FIT);
    }

    #[test]
    fn bend_handle_bends_without_panning() {
        let mut h = harness(user_curve());
        let mx = (0.2368 + 0.3532) / 2.0;
        let handle = screen(&h, mx, curve::value_at(&user_curve(), mx));
        drag(&mut h, handle + Vec2::new(6.0, 0.0), handle + Vec2::new(6.0, -60.0), 4);
        let st = h.state();
        assert_eq!(st.canvas.view, Canvas::FIT);
        assert!(st.pts[1].shape < -0.05, "dragging up on a rising segment should start it fast: {}", st.pts[1].shape);
    }

    #[test]
    fn empty_space_drag_changes_nothing() {
        let mut h = harness(user_curve());
        let at = screen(&h, 0.9, 0.9);
        drag(&mut h, at, at + Vec2::new(-200.0, 100.0), 5);
        let st = h.state();
        assert_eq!(st.canvas.view, Canvas::FIT);
        assert_eq!(st.pts, user_curve());
    }

    #[test]
    fn middle_drag_pans() {
        let mut h = harness(user_curve());
        let at = screen(&h, 0.5, 0.5);
        h.event(Event::PointerMoved(at));
        h.step();
        button(&mut h, at, PointerButton::Middle, true);
        for k in 1..=5 {
            h.event(Event::PointerMoved(at + Vec2::new(-20.0 * k as f32, 0.0)));
            h.step();
        }
        button(&mut h, at + Vec2::new(-100.0, 0.0), PointerButton::Middle, false);
        assert!(h.state().canvas.view[0] > Canvas::FIT[0], "view should pan right");
        assert_eq!(h.state().pts, user_curve());
    }

    // ── Trackpad: the same events a Mac trackpad produces ───────────────────

    #[test]
    fn two_finger_scroll_pans() {
        let mut h = harness(user_curve());
        h.event(Event::PointerMoved(screen(&h, 0.5, 0.5)));
        h.step();
        h.event(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: Vec2::new(-80.0, 0.0),
            phase: TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        for _ in 0..60 {
            h.step(); // egui spreads scrolling over a few frames
        }
        assert!(h.state().canvas.view[0] > Canvas::FIT[0], "{:?}", h.state().canvas.view);
        assert_eq!(h.state().pts, user_curve());
    }

    #[test]
    fn pinch_zooms_around_the_pointer() {
        let mut h = harness(user_curve());
        let at = screen(&h, 0.3532, 0.6697);
        h.event(Event::PointerMoved(at));
        h.step();
        h.event(Event::Zoom(1.5));
        h.step();
        let v = h.state().canvas.view;
        assert!(v[1] - v[0] < (Canvas::FIT[1] - Canvas::FIT[0]) * 0.8, "zoomed in: {v:?}");
        // The point under the fingers stays under them.
        let after = screen(&h, 0.3532, 0.6697);
        assert!(after.distance(at) < 2.0, "{at:?} vs {after:?}");
    }

    #[test]
    fn tap_tap_adds_a_point() {
        let mut h = harness(user_curve());
        let at = screen(&h, 0.55, 0.15);
        h.event(Event::PointerMoved(at));
        h.step();
        for _ in 0..2 {
            button(&mut h, at, PointerButton::Primary, true);
            button(&mut h, at, PointerButton::Primary, false);
        }
        let st = h.state();
        assert_eq!(st.pts.len(), 5, "double tap adds a point");
        assert!(st.pts.iter().any(|q| (q.x - 0.55).abs() < 0.01 && (q.y - 0.15).abs() < 0.01));
    }

    #[test]
    fn read_only_curves_ignore_edits_but_select() {
        let mut h = harness(user_curve());
        h.state_mut().editable = false;
        let at = screen(&h, 0.3532, 0.6697);
        drag(&mut h, at, at + Vec2::new(30.0, -40.0), 4);
        assert_eq!(h.state().pts, user_curve(), "no edits on a read-only curve");
        assert_eq!(h.state().sel, Some(2), "but points can be selected to read their values");
        let empty = screen(&h, 0.55, 0.15);
        h.event(Event::PointerMoved(empty));
        h.step();
        for _ in 0..2 {
            button(&mut h, empty, PointerButton::Primary, true);
            button(&mut h, empty, PointerButton::Primary, false);
        }
        assert_eq!(h.state().pts.len(), 4, "double-click adds nothing");
    }

    // ── Multi-select ────────────────────────────────────────────────────────

    #[test]
    fn marquee_selects_points_and_group_drag_moves_them() {
        let mut h = harness(user_curve());
        // Box around points 1 and 2 (0.0927..0.2368 in x, low y).
        let a = screen(&h, 0.05, 0.5);
        let b = screen(&h, 0.3, 0.2);
        drag(&mut h, a, b, 4);
        assert_eq!(h.state().canvas.selection.iter().copied().collect::<Vec<_>>(), vec![0, 1], "box selects the two points");
        assert_eq!(h.state().pts, user_curve(), "a box selection moves nothing");
        // Drag one of them: both move by the same amount.
        let at = screen(&h, 0.2368, 0.3054);
        drag(&mut h, at, at + Vec2::new(0.0, -40.0), 4);
        let st = h.state();
        let d0 = st.pts[0].y - user_curve()[0].y;
        let d1 = st.pts[1].y - user_curve()[1].y;
        assert!(d0 > 0.02 && (d0 - d1).abs() < 1e-9, "both moved up together: {d0} {d1}");
        assert_eq!(st.pts[2], user_curve()[2], "unselected points stay");
    }

    #[test]
    fn group_squeezes_against_the_edge() {
        let mut pts = vec![p(0.0, 0.5), p(0.2, 0.5), p(0.4, 0.5), p(1.0, 0.5)];
        let start = vec![(1, 0.2, 0.5), (2, 0.4, 0.5)];
        move_group(&mut pts, &start, -0.3, 0.7);
        assert!(pts[1].x == 0.0 && (pts[2].x - 0.1).abs() < 1e-12, "the leading point stops at the left neighbour, the other keeps going: {pts:?}");
        assert_eq!((pts[1].y, pts[2].y), (1.0, 1.0), "both stop at the top edge");
        move_group(&mut pts, &start, 2.0, -2.0);
        assert_eq!((pts[1].x, pts[2].x), (1.0, 1.0), "squeezed together at the right neighbour");
        assert_eq!((pts[1].y, pts[2].y), (0.0, 0.0));
    }

    #[test]
    fn shift_click_toggles_and_delete_keeps_two() {
        let mut h = harness(user_curve());
        let a = screen(&h, 0.0927, 0.4088);
        let c = screen(&h, 0.3532, 0.6697);
        h.event(Event::PointerMoved(a));
        h.step();
        button(&mut h, a, PointerButton::Primary, true);
        button(&mut h, a, PointerButton::Primary, false);
        h.event_modifiers(Event::PointerButton { pos: c, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::SHIFT }, Modifiers::SHIFT);
        h.step();
        h.event_modifiers(Event::PointerButton { pos: c, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::SHIFT }, Modifiers::SHIFT);
        h.step();
        assert_eq!(h.state().canvas.selection.iter().copied().collect::<Vec<_>>(), vec![0, 2]);
        let mut pts = user_curve();
        delete_selected(&mut pts, &[0usize, 1, 2, 3].into_iter().collect());
        assert_eq!(pts.len(), 2, "never fewer than two points");
    }
}
