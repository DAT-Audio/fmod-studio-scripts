//! The app's looks.
//!
//! * **Classic** (the default; 0.9.1 took DAT Audio's graphite grounds and
//!   both looks took the system font).
//! * **DAT Audio** (an experiment from 0.9.0): the Network Audio Grabber's
//!   "Graphite & Amber" — graphite grounds a shade apart (list #242425, side
//!   panels #2a2a2b, controls #343435), text #ECECEC with muted labels at 66 %
//!   white (#A7A7A8, 6:1 on the ground), and one amber accent (#F0A028) kept
//!   for actions and the curve's points, which are drawn as the family's
//!   rounded-square "packets". The macOS system font (SF) when it can be read.
//!   Values from DAT Audio's brand palette and the Grabber's own screenshots.
//!
//! Both are dark, always: the app never follows the system's light mode
//! (0.8.0 did, which put dark text on these dark panels).
//!
//! Colours are read through `pal()`: the current palette, per thread (the UI
//! runs on one), so drawing code needs no extra parameter.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::cell::Cell;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Dat,
    Classic,
}

impl Look {
    pub const ALL: [Look; 2] = [Look::Classic, Look::Dat];
    pub fn name(self) -> &'static str {
        match self {
            Look::Dat => "DAT Audio",
            Look::Classic => "Classic",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Look::Dat => "dat",
            Look::Classic => "classic",
        }
    }
    pub fn from_key(k: &str) -> Look {
        if k == "dat" { Look::Dat } else { Look::Classic }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    // Window
    pub ground: Color32,
    pub side: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub field: Color32,
    pub selected_row: Color32,
    pub hairline: Color32,
    pub text: Color32,
    pub strong: Color32,
    pub dim: Color32,
    pub warn: Color32,
    pub bad: Color32,
    pub ok: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub banner: Color32,
    pub star: Color32,
    // Graph
    pub curve: Color32,
    pub curve_fill_alpha: u8,
    pub bg: Color32,
    pub plot: Color32,
    pub grid: Color32,
    pub grid_major: Color32,
    pub border: Color32,
    pub point: Color32,
    pub point_selected: Color32,
    pub label: Color32,
    pub title: Color32,
    pub ghost: Color32,
    pub marquee: Color32,
    /// Points drawn as rounded squares (the family's packets) rather than dots.
    pub square_points: bool,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Palette {
    pub const DAT: Palette = Palette {
        ground: rgb(0x24, 0x24, 0x25),
        side: rgb(0x2a, 0x2a, 0x2b),
        raised: rgb(0x34, 0x34, 0x35),
        hover: rgb(0x3e, 0x3e, 0x40),
        pressed: rgb(0x48, 0x48, 0x4b),
        field: rgb(0x2f, 0x2f, 0x30),
        selected_row: rgb(0x3a, 0x3a, 0x3b),
        hairline: rgb(0x38, 0x38, 0x3a),
        text: rgb(0xec, 0xec, 0xec),
        strong: rgb(0xf5, 0xf5, 0xf5),
        dim: rgb(0xa7, 0xa7, 0xa8),
        warn: rgb(0xf0, 0xa0, 0x28),
        bad: rgb(0xff, 0x6b, 0x60),
        ok: rgb(0x6c, 0xc9, 0x74),
        accent: rgb(0xf0, 0xa0, 0x28),
        on_accent: rgb(0x1d, 0x1d, 0x1f),
        danger: rgb(0xd9, 0x47, 0x3f),
        banner: rgb(0x3d, 0x2c, 0x12),
        star: rgb(0xf0, 0xa0, 0x28),
        // The graph keeps its charcoal; bone line, amber packets.
        curve: rgb(0xed, 0xeb, 0xe3),
        curve_fill_alpha: 26,
        bg: rgb(0x1c, 0x1c, 0x1c),
        plot: rgb(0x20, 0x20, 0x20),
        grid: rgb(0x2b, 0x2b, 0x2c),
        grid_major: rgb(0x38, 0x38, 0x3a),
        border: rgb(0x5c, 0x5c, 0x60),
        point: rgb(0xf0, 0xa0, 0x28),
        point_selected: rgb(0xff, 0xff, 0xff),
        label: rgb(0xa7, 0xa7, 0xa8),
        title: rgb(0xc8, 0xc8, 0xcc),
        ghost: rgb(0x8c, 0x8c, 0x92),
        marquee: Color32::from_rgba_premultiplied(0x30, 0x22, 0x08, 0x40),
        square_points: true,
    };

    // 0.9.1: the DAT Audio graphite grounds (better than the off-purple
    // #232327), neutral greys for controls; the rest as before.
    pub const CLASSIC: Palette = Palette {
        ground: rgb(0x24, 0x24, 0x25),
        side: rgb(0x2a, 0x2a, 0x2b),
        raised: rgb(0x3a, 0x3a, 0x3c),
        hover: rgb(0x47, 0x47, 0x4a),
        pressed: rgb(0x52, 0x52, 0x56),
        field: rgb(0x1c, 0x1c, 0x1d),
        selected_row: rgb(0x44, 0x4e, 0x5e),
        hairline: rgb(0x3a, 0x3a, 0x3c),
        text: rgb(0xe6, 0xe6, 0xe8),
        strong: rgb(0xf5, 0xf5, 0xf5),
        dim: rgb(0xa7, 0xa7, 0xa8),
        warn: rgb(0xff, 0xb3, 0x47),
        bad: rgb(0xff, 0x6b, 0x6b),
        ok: rgb(0x7f, 0xd1, 0x8b),
        accent: rgb(0xff, 0xd2, 0x3f),
        on_accent: rgb(0x1e, 0x1a, 0x0c),
        danger: rgb(0xc0, 0x39, 0x3b),
        banner: rgb(0x4a, 0x2a, 0x10),
        star: rgb(0xff, 0xd2, 0x3f),
        curve: rgb(0xff, 0x3d, 0x6e),
        curve_fill_alpha: 46,
        bg: rgb(0x1c, 0x1c, 0x1c),
        plot: rgb(0x20, 0x20, 0x20),
        grid: rgb(0x2c, 0x2c, 0x2c),
        grid_major: rgb(0x3a, 0x3a, 0x3a),
        border: rgb(0x6a, 0x6a, 0x6a),
        point: rgb(0xf0, 0xf0, 0xf0),
        point_selected: rgb(0xff, 0xd2, 0x3f),
        label: rgb(0x9a, 0x9a, 0x9a),
        title: rgb(0xc4, 0xc4, 0xcc),
        ghost: rgb(0x8f, 0x9b, 0xb0),
        marquee: Color32::from_rgba_premultiplied(0x30, 0x2a, 0x10, 0x40),
        square_points: false,
    };

    pub fn of(look: Look) -> Palette {
        match look {
            Look::Dat => Palette::DAT,
            Look::Classic => Palette::CLASSIC,
        }
    }
}

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(Palette::CLASSIC) };
}

/// The current palette.
pub fn pal() -> Palette {
    CURRENT.with(Cell::get)
}

/// Switches the look: palette, widget style and fonts.
pub fn apply(ctx: &egui::Context, look: Look) {
    let p = Palette::of(look);
    CURRENT.with(|c| c.set(p));
    // Always dark, whatever the system's appearance.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_fonts(fonts(look));
    let dat = look == Look::Dat;
    ctx.all_styles_mut(|s| {
        s.visuals = egui::Visuals::dark();
        s.spacing.item_spacing = egui::vec2(8.0, if dat { 6.0 } else { 7.0 });
        s.spacing.button_padding = egui::vec2(10.0, if dat { 4.0 } else { 5.0 });
        s.spacing.interact_size.y = if dat { 24.0 } else { 26.0 };
        s.spacing.window_margin = egui::Margin::same(14);
        s.spacing.menu_margin = egui::Margin::same(8);
        let body = if dat { 13.5 } else { 14.0 };
        s.text_styles.insert(TextStyle::Body, FontId::proportional(body));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(body));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(if dat { 19.0 } else { 21.0 }));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(if dat { 11.5 } else { 12.0 }));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(if dat { 12.5 } else { 13.0 }));
        let v = &mut s.visuals;
        v.text_options = text_options();
        v.override_text_color = Some(p.text);
        v.panel_fill = p.ground;
        v.window_fill = if dat { rgb(0x2c, 0x2c, 0x2e) } else { rgb(0x2a, 0x2a, 0x30) };
        v.window_stroke = egui::Stroke::new(1.0, p.hairline);
        v.window_corner_radius = CornerRadius::same(10);
        v.extreme_bg_color = p.field;
        v.text_edit_bg_color = Some(p.field);
        v.faint_bg_color = p.side;
        v.code_bg_color = p.field;
        v.hyperlink_color = p.accent;
        v.warn_fg_color = p.warn;
        v.error_fg_color = p.bad;
        v.selection.bg_fill = p.selected_row;
        v.selection.stroke = egui::Stroke::new(1.0, if dat { p.accent } else { rgb(0xe8, 0xe8, 0xec) });
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, p.hairline);
        v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, p.text);
        for (w, fill) in [
            (&mut v.widgets.inactive, p.raised),
            (&mut v.widgets.hovered, p.hover),
            (&mut v.widgets.active, p.pressed),
            (&mut v.widgets.open, p.hover),
        ] {
            w.corner_radius = CornerRadius::same(6);
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.fg_stroke = egui::Stroke::new(1.0, p.strong);
            w.bg_stroke = if dat { egui::Stroke::NONE } else { w.bg_stroke };
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.corner_radius = CornerRadius::same(6);
        // Tick boxes and radio dots: the accent when on.
        v.widgets.inactive.fg_stroke = egui::Stroke::new(1.5, p.strong);
    });
}

/// How glyphs are rasterised. Development: ACL_TEXT="hint,bin,ctf" (each 0/1,
/// ctf 0 = egui's dark default, 1 = off, g<value> = gamma) to compare.
fn text_options() -> egui::epaint::text::TextOptions {
    let mut o = egui::Visuals::dark().text_options;
    // Sub-pixel binning renders each glyph at four fractional offsets, which
    // egui itself notes looks blurrier; whole-pixel glyphs with hinting are
    // crisper when compared zoomed in.
    o.subpixel_binning = false;
    if let Ok(spec) = std::env::var("ACL_TEXT") {
        let parts: Vec<&str> = spec.split(',').collect();
        if let Some(h) = parts.first() {
            o.font_hinting = *h == "1";
        }
        if let Some(b) = parts.get(1) {
            o.subpixel_binning = *b == "1";
        }
        match parts.get(2).copied() {
            Some("1") => o.color_transfer_function = egui::epaint::FontColorTransferFunction::Off,
            Some(g) if g.starts_with('g') => o.color_transfer_function = egui::epaint::FontColorTransferFunction::Gamma(g[1..].parse().unwrap_or(1.0)),
            _ => {}
        }
    }
    o
}

/// The system font when it's there (macOS: SF; Windows: Segoe UI and
/// Consolas), read at run time, never bundled; otherwise egui's own fonts.
fn fonts(look: Look) -> FontDefinitions {
    let _ = look; // both looks use it since 0.9.1 (egui's Ubuntu Light read thin and soft)
    let mut f = FontDefinitions::default();
    let win = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let candidates: Vec<(&str, String, FontFamily)> = if cfg!(windows) {
        vec![
            ("segoe", format!(r"{win}\Fonts\segoeui.ttf"), FontFamily::Proportional),
            ("consolas", format!(r"{win}\Fonts\consola.ttf"), FontFamily::Monospace),
        ]
    } else {
        vec![
            ("sf", "/System/Library/Fonts/SFNS.ttf".into(), FontFamily::Proportional),
            ("sf-mono", "/System/Library/Fonts/SFNSMono.ttf".into(), FontFamily::Monospace),
        ]
    };
    for (name, path, family) in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            f.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            f.families.entry(family).or_default().insert(0, name.into());
        }
    }
    f
}

/// A favourite star: filled when on, an outline when off. Drawn rather than
/// typed, since not every font has ★.
pub fn star(painter: &egui::Painter, centre: egui::Pos2, radius: f32, on: bool, colour: Color32) {
    let pts: Vec<egui::Pos2> = (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { radius } else { radius * 0.45 };
            let a = std::f32::consts::PI * (i as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
            centre + egui::vec2(a.cos() * r, a.sin() * r)
        })
        .collect();
    if on {
        // A star isn't convex: fill it as five triangles from the centre plus the inner pentagon.
        for i in 0..5 {
            let tip = pts[i * 2];
            let a = pts[(i * 2 + 9) % 10];
            let b = pts[i * 2 + 1];
            painter.add(egui::Shape::convex_polygon(vec![a, tip, b], colour, egui::Stroke::NONE));
        }
        let inner: Vec<egui::Pos2> = (0..5).map(|i| pts[i * 2 + 1]).collect();
        painter.add(egui::Shape::convex_polygon(inner, colour, egui::Stroke::NONE));
    } else {
        let mut ring = pts.clone();
        ring.push(pts[0]);
        painter.add(egui::Shape::line(ring, egui::Stroke::new(1.2, colour)));
    }
}

/// A curve point: a dot (Classic) or a rounded-square packet (DAT Audio).
pub fn point(painter: &egui::Painter, c: egui::Pos2, radius: f32, fill: Color32, stroke: egui::Stroke) {
    if pal().square_points {
        let side = radius * 1.8;
        let rect = egui::Rect::from_center_size(c, egui::vec2(side, side));
        let r = CornerRadius::same((side * 0.24).round() as u8);
        painter.rect_filled(rect, r, fill);
        if stroke.width > 0.0 {
            painter.rect_stroke(rect, r, stroke, egui::StrokeKind::Outside);
        }
    } else {
        painter.circle_filled(c, radius, fill);
        if stroke.width > 0.0 {
            painter.circle_stroke(c, radius, stroke);
        }
    }
}
