// Hallmark · v2.0.0 · 2026-10-31 · theme: Warm Graphite, amber accent (by request)
// scope: app screen (native egui) · genre: app · shell kept from the project
// signature: the diff reader · icons: vector epaint strokes, no emoji (30a)
// sweep: sloplint n/a on Rust sources · rebuild + before/after captures + manual gate walk
// redesign: full · keeps content and wiring · built: the project · redesign: full
// pre-emit critique: P5 H4 E4 S4 R5 V4 · weakest: S · fix: status chips + gutter row language
//
// Pre-emit critique line, gates and sweep live at the top of this file: there is
// no CSS in this project, so this header is the Hallmark stamp (goal 20).
//
// One source of truth for every colour, size, radius and gap. No Color32 literal
// is allowed outside this file (goals 48, 24a, 7): app.rs reads only `tokens::*`.

use eframe::egui;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

pub mod tokens {
    use eframe::egui::Color32;

    /// Warm graphite ramp — every neutral is tinted toward the amber hue (goal 22, 7).
    pub const BG: Color32 = Color32::from_rgb(23, 22, 19);
    pub const SURFACE: Color32 = Color32::from_rgb(33, 31, 27);
    pub const SURFACE_HI: Color32 = Color32::from_rgb(44, 41, 36); // hover wells
    pub const SURFACE_ACTIVE: Color32 = Color32::from_rgb(52, 48, 42);
    pub const WELL: Color32 = Color32::from_rgb(19, 18, 16); // inputs, editor bg
    pub const HAIRLINE: Color32 = Color32::from_rgb(55, 51, 45);
    pub const HAIRLINE_STRONG: Color32 = Color32::from_rgb(78, 73, 63);

    /// Ink.
    pub const TEXT: Color32 = Color32::from_rgb(233, 228, 219);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(158, 150, 138);
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(122, 114, 103);

    /// Amber accent with its paired dark ink for text on fills (goal 41).
    pub const ACCENT: Color32 = Color32::from_rgb(232, 160, 60);
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(245, 181, 86);
    pub const ACCENT_ACTIVE: Color32 = Color32::from_rgb(205, 138, 50);
    pub const ACCENT_INK: Color32 = Color32::from_rgb(32, 20, 6);
    pub const ACCENT_BG: Color32 = Color32::from_rgba_unmultiplied_const(232, 160, 60, 40);

    /// Semantic colours, only where a status carries meaning (goal 23, craft §2).
    pub const SUCCESS: Color32 = Color32::from_rgb(128, 199, 125);
    pub const ADD_BG: Color32 = Color32::from_rgba_unmultiplied_const(80, 160, 95, 34);
    pub const ADD_BAR: Color32 = Color32::from_rgba_unmultiplied_const(80, 160, 95, 110);
    pub const DANGER: Color32 = Color32::from_rgb(185, 68, 60); // fills
    pub const DANGER_HOVER: Color32 = Color32::from_rgb(166, 58, 51);
    pub const DANGER_ACTIVE: Color32 = Color32::from_rgb(148, 51, 45);
    pub const DANGER_INK: Color32 = Color32::from_rgb(255, 240, 235);
    pub const DANGER_TEXT: Color32 = Color32::from_rgb(235, 120, 110); // text on dark bg
    pub const DEL_BG: Color32 = Color32::from_rgba_unmultiplied_const(170, 60, 55, 34);
    pub const DEL_BAR: Color32 = Color32::from_rgba_unmultiplied_const(170, 60, 55, 110);
    pub const INFO: Color32 = Color32::from_rgb(132, 180, 210); // untracked, remote, renamed
    pub const INFO_BG: Color32 = Color32::from_rgba_unmultiplied_const(123, 172, 205, 30);

    /// Diff and code surfaces.
    pub const GUTTER_BG: Color32 = Color32::from_rgb(26, 25, 22);
    pub const GUTTER_NUM: Color32 = Color32::from_rgb(140, 131, 119); // 4.5:1 on GUTTER_BG
    pub const HUNK_BG: Color32 = Color32::from_rgb(36, 33, 29);
    pub const META_BG: Color32 = Color32::from_rgb(28, 27, 24);

    /// One radius per nesting level, concentric (craft §3).
    pub const RADIUS_WINDOW: f32 = 8.0;
    pub const RADIUS_WIDGET: f32 = 5.0;
    pub const RADIUS_CHIP: f32 = 3.0;
}

pub mod metrics {
    /// Spacing scale: 4 · 8 · 12 · 16 · 24 · 40 (no raw gaps off the scale, 24a/b).
    pub const SP_2XS: f32 = 4.0;
    pub const SP_XS: f32 = 8.0;
    pub const SP_SM: f32 = 12.0;
    pub const SP_MD: f32 = 16.0;
    pub const SP_LG: f32 = 24.0;
    pub const SP_XL: f32 = 40.0;

    /// Type scale, ratio ~1.2, fixed for app scope (flow § App screens).
    pub const TEXT_SMALL: f32 = 12.0;
    pub const TEXT_MONO: f32 = 12.5;
    pub const TEXT_BODY: f32 = 14.0;
    pub const TEXT_HEADING: f32 = 17.0;

    /// Controls (39b, app range) and rows.
    pub const CTRL_H: f32 = 32.0;
    pub const CTRL_H_SM: f32 = 26.0;
    pub const ROW_H: f32 = 28.0;
    pub const DIFF_ROW_H: f32 = 19.0;

    /// Panel geometry (the shell — kept from the project).
    pub const TOPBAR_H: f32 = 48.0;
    pub const STATUSBAR_H: f32 = 30.0;

    /// Sidebar composer block height reserved under the single list scroll.
    pub const COMPOSER_H: f32 = 172.0;

    /// Icon glyph size inside buttons/rows.
    pub const ICON_SIZE: f32 = 16.0;
    pub const ICON_LOGO: f32 = 22.0;
}

// ---------------------------------------------------------------------------
// Status mapping — Git status letters get exactly one pair of tokens.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct StatusStyle {
    pub fg: egui::Color32,
    pub bg: egui::Color32,
    pub bar: egui::Color32,
}

pub fn status_style(ch: char) -> StatusStyle {
    use tokens::*;
    match ch {
        'A' => StatusStyle { fg: SUCCESS, bg: ADD_BG, bar: ADD_BAR },
        'D' => StatusStyle { fg: DANGER_TEXT, bg: DEL_BG, bar: DEL_BAR },
        'M' | 'T' => StatusStyle { fg: ACCENT, bg: HUNK_BG, bar: ACCENT },
        'R' | 'C' => StatusStyle { fg: INFO, bg: INFO_BG, bar: INFO },
        '?' => StatusStyle { fg: INFO, bg: INFO_BG, bar: INFO },
        _ => StatusStyle { fg: TEXT_MUTED, bg: META_BG, bar: TEXT_FAINT },
    }
}

// ---------------------------------------------------------------------------
// Icons — epaint strokes only, no emoji, no icon library (30a).
// ---------------------------------------------------------------------------

pub mod icons {
    use super::metrics;
    use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Icon {
        Folder,
        Refresh,
        Push,
        Pull,
        Fetch,
        Branch,
        Plus,
        Minus,
        X,
        Trash,
        Check,
        Warn,
        File,
    }

    fn pt(rect: Rect, fx: f32, fy: f32) -> Pos2 {
        let inset = rect.size().min_elem() * 0.12;
        let inner = rect.shrink(inset);
        Pos2::new(inner.left() + fx * inner.width(), inner.top() + fy * inner.height())
    }

    fn stroke(w: f32, color: Color32) -> Stroke {
        Stroke::new(w, color)
    }

    /// Paints a stroke icon inside `rect`. Drawn on a 0..1 field, so it scales
    /// crisp at any size.
    pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
        let s = (metrics::ICON_SIZE.min(rect.width()) / 16.0).max(1.0);
        let w = 1.5 * s;
        let pts = |v: &[(f32, f32)]| -> Vec<Pos2> {
            v.iter().map(|(x, y)| pt(rect, *x, *y)).collect()
        };

        match icon {
            Icon::Folder => {
                p.add(Shape::line(
                    pts(&[
                        (0.06, 0.29), (0.34, 0.29), (0.42, 0.19), (0.94, 0.19),
                        (0.94, 0.80), (0.06, 0.80), (0.06, 0.29),
                    ]),
                    stroke(w, color),
                ));
            }
            Icon::File => {
                p.add(Shape::line(
                    pts(&[(0.22, 0.06), (0.60, 0.06), (0.80, 0.26), (0.80, 0.94), (0.22, 0.94), (0.22, 0.06)]),
                    stroke(w, color),
                ));
                p.add(Shape::line(
                    pts(&[(0.60, 0.06), (0.60, 0.26), (0.80, 0.26)]),
                    stroke(w, color),
                ));
            }
            Icon::Refresh => {
                // Open circular arc with an arrowhead.
                let cx = 0.5_f32;
                let cy = 0.5_f32;
                let r = 0.34_f32;
                let a0 = -230_f32.to_radians();
                let a1 = 55_f32.to_radians();
                let mut pts = Vec::new();
                for i in 0..=22 {
                    let t = a0 + (a1 - a0) * (i as f32 / 22.0);
                    pts.push(pt(rect, cx + r * t.cos(), cy + r * t.sin()));
                }
                p.add(Shape::line(pts, stroke(w, color)));
                p.add(Shape::line(
                    vec![pt(rect, 0.84, 0.28), pt(rect, 0.86, 0.52)],
                    stroke(w, color),
                ));
                p.add(Shape::line(
                    vec![pt(rect, 0.84, 0.28), pt(rect, 0.62, 0.26)],
                    stroke(w, color),
                ));
            }
            Icon::Push => {
                p.add(Shape::line(pts(&[(0.50, 0.88), (0.50, 0.14)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.50, 0.14), (0.32, 0.34)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.50, 0.14), (0.68, 0.34)]), stroke(w, color)));
            }
            Icon::Pull => {
                p.add(Shape::line(pts(&[(0.50, 0.14), (0.50, 0.88)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.50, 0.88), (0.32, 0.68)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.50, 0.88), (0.68, 0.68)]), stroke(w, color)));
            }
            Icon::Fetch => {
                // Two opposed shafts (sync pair).
                p.add(Shape::line(pts(&[(0.14, 0.32), (0.86, 0.32)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.86, 0.32), (0.70, 0.16)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.86, 0.32), (0.70, 0.48)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.86, 0.68), (0.14, 0.68)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.14, 0.68), (0.30, 0.52)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.14, 0.68), (0.30, 0.84)]), stroke(w, color)));
            }
            Icon::Branch => {
                let c1 = pt(rect, 0.28, 0.22);
                let c2 = pt(rect, 0.28, 0.78);
                let c3 = pt(rect, 0.72, 0.26);
                let r = rect.height() * 0.11;
                p.add(Shape::circle_stroke(c1, r, stroke(w, color)));
                p.add(Shape::circle_stroke(c2, r, stroke(w, color)));
                p.add(Shape::circle_stroke(c3, r, stroke(w, color)));
                p.add(Shape::line(
                    vec![Pos2::new(c1.x, c1.y + r), Pos2::new(c2.x, c2.y - r)],
                    stroke(w, color),
                ));
                p.add(Shape::line(
                    vec![Pos2::new(c1.x + r * 0.9, c1.y), Pos2::new(c3.x - r * 1.2, c3.y)],
                    stroke(w, color),
                ));
            }
            Icon::Plus => {
                p.add(Shape::line(pts(&[(0.50, 0.14), (0.50, 0.86)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.14, 0.50), (0.86, 0.50)]), stroke(w, color)));
            }
            Icon::Minus => {
                p.add(Shape::line(pts(&[(0.14, 0.50), (0.86, 0.50)]), stroke(w, color)));
            }
            Icon::X => {
                p.add(Shape::line(pts(&[(0.20, 0.20), (0.80, 0.80)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.80, 0.20), (0.20, 0.80)]), stroke(w, color)));
            }
            Icon::Trash => {
                p.add(Shape::line(pts(&[(0.18, 0.22), (0.82, 0.22)]), stroke(w, color)));
                p.add(Shape::line(
                    pts(&[(0.40, 0.22), (0.40, 0.13), (0.60, 0.13), (0.60, 0.22)]),
                    stroke(w, color),
                ));
                p.add(Shape::line(
                    pts(&[(0.26, 0.28), (0.28, 0.88), (0.72, 0.88), (0.74, 0.28)]),
                    stroke(w, color),
                ));
                p.add(Shape::line(pts(&[(0.44, 0.44), (0.44, 0.70)]), stroke(w, color)));
                p.add(Shape::line(pts(&[(0.56, 0.44), (0.56, 0.70)]), stroke(w, color)));
            }
            Icon::Check => {
                p.add(Shape::line(
                    pts(&[(0.16, 0.52), (0.40, 0.76), (0.84, 0.22)]),
                    stroke(w, color),
                ));
            }
            Icon::Warn => {
                p.add(Shape::line(
                    pts(&[(0.50, 0.10), (0.92, 0.86), (0.08, 0.86), (0.50, 0.10)]),
                    stroke(w, color),
                ));
                p.add(Shape::line(
                    pts(&[(0.50, 0.38), (0.50, 0.60)]),
                    stroke(w * 1.2, color),
                ));
                p.add(Shape::circle_filled(pt(rect, 0.50, 0.74), rect.height() * 0.035, color));
            }
        }
    }

    /// Rect of size `size` centred inside `rect` — the glyph area of a control.
    pub fn glyph_in(rect: Rect, size: f32) -> Rect {
        Rect::from_center_size(rect.center(), Vec2::splat(size))
    }
}

// ---------------------------------------------------------------------------
// Custom controls — one paint path so all eight widget states match (26a, 39a).
// ---------------------------------------------------------------------------

pub mod widgets {
    use super::icons::{self, Icon};
    use super::metrics;
    use super::tokens as t;
    use eframe::egui::{
        self, Align2, Color32, FontId, Label, Margin, RichText, Sense, Stroke, Ui, Vec2,
    };

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Btn {
        /// Filled accent — the one primary style per screen (craft §9).
        Primary,
        /// Flat surface fill for secondary actions.
        Soft,
        /// Text-only, hover reveals a well.
        Quiet,
        /// Destructive fill.
        Danger,
    }

    struct BtnColors {
        idle: Option<Color32>,
        hover: Color32,
        active: Color32,
        text: Color32,
    }

    fn palette(kind: Btn, enabled: bool) -> BtnColors {
        if !enabled {
            return BtnColors {
                idle: Some(t::SURFACE),
                hover: t::SURFACE,
                active: t::SURFACE,
                text: t::TEXT_FAINT,
            };
        }
        match kind {
            Btn::Primary => BtnColors {
                idle: Some(t::ACCENT),
                hover: t::ACCENT_HOVER,
                active: t::ACCENT_ACTIVE,
                text: t::ACCENT_INK,
            },
            Btn::Soft => BtnColors {
                idle: Some(t::SURFACE),
                hover: t::SURFACE_HI,
                active: t::SURFACE_ACTIVE,
                text: t::TEXT,
            },
            Btn::Quiet => BtnColors {
                idle: None,
                hover: t::SURFACE,
                active: t::SURFACE_HI,
                text: t::TEXT_MUTED,
            },
            Btn::Danger => BtnColors {
                idle: Some(t::DANGER),
                hover: t::DANGER_HOVER,
                active: t::DANGER_ACTIVE,
                text: t::DANGER_INK,
            },
        }
    }

    #[derive(Clone, Copy)]
    struct Painted {
        hovered: bool,
        pressed: bool,
        focus: bool,
    }

    fn paint_frame(p: &egui::Painter, rect: egui::Rect, pa: &Painted, c: &BtnColors, radius: f32) {
        let (fill, _) = if pa.pressed {
            (Some(c.active), c.text)
        } else if pa.hovered {
            (Some(c.hover), c.text)
        } else {
            (c.idle, c.text)
        };
        if let Some(f) = fill {
            p.rect_filled(rect, radius, f);
        }
        if pa.focus {
            p.rect_stroke(rect.expand(1.5), radius, Stroke::new(1.5, t::ACCENT), egui::StrokeKind::Middle);
        }
    }

    const PAD_X: f32 = 10.0;
    const GAP: f32 = 6.0;

    /// Icon + label button of `Ctrl` height. The app's only text-button recipe
    /// (F4, F15): one primary style, secondary quieter.
    pub fn text_button(
        ui: &mut Ui,
        icon: Option<Icon>,
        label: &str,
        kind: Btn,
        enabled: bool,
    ) -> egui::Response {
        let font = FontId::proportional(metrics::TEXT_BODY);
        let c = palette(kind, enabled);
        let p = ui.painter();
        let text_w = if label.is_empty() {
            0.0
        } else {
            p.layout_no_wrap(label.to_owned(), font.clone(), t::TEXT).size().x
        };
        let mut w = text_w + 2.0 * PAD_X + if icon.is_some() { metrics::ICON_SIZE + GAP } else { 0.0 };
        w = w.max(metrics::CTRL_H);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, metrics::CTRL_H), Sense::click());

        let focus = resp.has_focus();
        let pa = Painted {
            hovered: enabled && resp.hovered(),
            pressed: enabled && resp.is_pointer_button_down_on(),
            focus,
        };
        let p = ui.painter_at(rect);
        paint_frame(&p, rect, &pa, &c, t::RADIUS_WIDGET);

        let cy = rect.center().y;
        let mut x = rect.left() + PAD_X;
        if let Some(icon) = icon {
            let _ = icons::paint(&p, egui::Rect::from_center_size(
                egui::Pos2::new(x + metrics::ICON_SIZE * 0.5, cy),
                Vec2::splat(metrics::ICON_SIZE),
            ), icon, c.text);
            x += metrics::ICON_SIZE + GAP;
        }
        if !label.is_empty() {
            p.text(egui::Pos2::new(x, cy), Align2::LEFT_CENTER, label, font, c.text);
        }
        resp
    }

    /// A status or meta chip: text on a tinted well, one radius per level.
    pub fn chip(ui: &mut Ui, text: &str, fg: Color32, bg: Color32, mono: bool) {
        egui::Frame::NONE
            .fill(bg)
            .corner_radius(t::RADIUS_CHIP)
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| {
                let mut rt = if mono { RichText::new(text).monospace() } else { RichText::new(text) };
                rt = rt.size(metrics::TEXT_SMALL).color(fg);
                ui.add(Label::new(rt).truncate());
            });
    }

    /// Measure-free line: paints text at a point, returns the laid-out rect.
    pub fn paint_text(
        p: &egui::Painter,
        pos: egui::Pos2,
        anchor: Align2,
        text: &str,
        size: f32,
        color: Color32,
        mono: bool,
    ) -> egui::Rect {
        let font = if mono { FontId::monospace(size) } else { FontId::proportional(size) };
        p.text(pos, anchor, text, font, color)
    }
}

// ---------------------------------------------------------------------------
// Global style application.
// ---------------------------------------------------------------------------

/// Installs the type scale, spacing scale, palette and the vector icons' font.
/// Called once at startup (`main.rs`).
pub fn apply(ctx: &egui::Context) {
    // Craft §1: two families at most — the built-in proportional face plus
    // JetBrains Mono for the one mono role (code and hashes).
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "JetBrainsMono".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/JetBrainsMono-Regular.ttf"
        ))),
    );
    if let Some(monos) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        monos.insert(0, "JetBrainsMono".to_owned());
    }
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);

    ctx.style_mut_of(egui::Theme::Dark, |style| {
        // Motion: none. State changes land instantly (goal 15, craft §5).
        style.animation_time = 0.0;

        // Fixed type scale for app scope.
        use egui::{FontId, TextStyle};
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(metrics::TEXT_HEADING));
        style.text_styles.insert(TextStyle::Body, FontId::proportional(metrics::TEXT_BODY));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(metrics::TEXT_BODY));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(metrics::TEXT_SMALL));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(metrics::TEXT_MONO));

        // One spacing language.
        style.spacing.item_spacing = egui::Vec2::new(metrics::SP_XS, metrics::SP_XS * 0.75);
        style.spacing.button_padding = egui::Vec2::new(10.0, 7.0);
        style.spacing.icon_spacing = 4.0;

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(tokens::TEXT);
        v.panel_fill = tokens::BG;
        v.window_fill = tokens::SURFACE;
        v.window_stroke = egui::Stroke::new(1.0, tokens::HAIRLINE);
        v.window_corner_radius = tokens::RADIUS_WINDOW.into();
        v.extreme_bg_color = tokens::WELL;
        v.faint_bg_color = tokens::SURFACE_HI;
        v.code_bg_color = tokens::WELL;
        v.hyperlink_color = tokens::ACCENT;
        v.selection = egui::style::Selection {
            bg_fill: tokens::ACCENT_BG,
            stroke: egui::Stroke::new(1.0, tokens::TEXT),
        };
        v.text_cursor.stroke = egui::Stroke::new(1.6, tokens::ACCENT);

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = tokens::SURFACE;
        w.noninteractive.fg_stroke = egui::Stroke::new(1.0, tokens::TEXT_FAINT);
        w.noninteractive.bg_stroke = egui::Stroke::new(1.0, tokens::HAIRLINE);
        w.inactive.bg_fill = tokens::SURFACE;
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, tokens::HAIRLINE);
        w.inactive.fg_stroke = egui::Stroke::new(1.0, tokens::TEXT_MUTED);
        w.inactive.corner_radius = tokens::RADIUS_WIDGET.into();
        w.hovered.bg_fill = tokens::SURFACE_HI;
        w.hovered.weak_bg_fill = tokens::SURFACE_HI;
        w.hovered.bg_stroke = egui::Stroke::new(1.0, tokens::HAIRLINE_STRONG);
        w.hovered.fg_stroke = egui::Stroke::new(1.0, tokens::TEXT);
        w.hovered.corner_radius = tokens::RADIUS_WIDGET.into();
        w.active.bg_fill = tokens::SURFACE_ACTIVE;
        w.active.weak_bg_fill = tokens::SURFACE_ACTIVE;
        w.active.bg_stroke = egui::Stroke::new(1.0, tokens::ACCENT);
        w.active.fg_stroke = egui::Stroke::new(1.0, tokens::ACCENT);
        w.active.corner_radius = tokens::RADIUS_WIDGET.into();
    });
}
