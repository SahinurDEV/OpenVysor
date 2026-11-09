//! The look of the app: colors, fonts, and the widgets egui does not style well by default.

use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{
    self, Color32, Context, CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin,
    Painter, Pos2, Rect, Response, Sense, Shadow, Stroke, StrokeKind, TextFormat, TextStyle, Theme, Ui, Vec2, Visuals,
    pos2, vec2,
};

pub const RADIUS: u8 = 12;
const CONTROL_RADIUS: u8 = 8;
const BUTTON_HEIGHT: f32 = 36.0;
const SEMIBOLD: &str = "semibold";
const WINDOWS_FONTS: [(&str, &str); 2] =
    [("segoe", r"C:\Windows\Fonts\segoeui.ttf"), (SEMIBOLD, r"C:\Windows\Fonts\seguisb.ttf")];

pub struct Palette {
    pub background: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    pub muted: Color32,
    pub hover: Color32,
    pub foreground: Color32,
    pub secondary: Color32,
    pub border: Color32,
    pub primary: Color32,
    pub primary_hover: Color32,
    pub on_primary: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
}

const fn rgb(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub const DARK: Palette = Palette {
    background: rgb(0x0e1114),
    sidebar: rgb(0x12161a),
    card: rgb(0x171c21),
    muted: rgb(0x20262c),
    hover: rgb(0x2a3138),
    foreground: rgb(0xe8ebee),
    secondary: rgb(0x8e99a5),
    border: rgb(0x272e36),
    primary: rgb(0x3ddc97),
    primary_hover: rgb(0x5ae5a9),
    on_primary: rgb(0x06281b),
    success: rgb(0x3ddc97),
    warning: rgb(0xf2b34b),
    danger: rgb(0xff6b76),
};

pub const LIGHT: Palette = Palette {
    background: rgb(0xf5f6f7),
    sidebar: rgb(0xfbfbfc),
    card: rgb(0xffffff),
    muted: rgb(0xeceef1),
    hover: rgb(0xe1e4e8),
    foreground: rgb(0x14181d),
    secondary: rgb(0x5d6773),
    border: rgb(0xdde1e6),
    primary: rgb(0x0c8f6f),
    primary_hover: rgb(0x0a7a5f),
    on_primary: rgb(0xffffff),
    success: rgb(0x0c8f6f),
    warning: rgb(0xb26a00),
    danger: rgb(0xcc3340),
};

/// Saturated enough to read on either theme, for the app tiles' letter badges.
const AVATAR_COLORS: [Color32; 8] = [
    rgb(0x2f80ed),
    rgb(0x9b51e0),
    rgb(0xeb5757),
    rgb(0xf2994a),
    rgb(0x27ae60),
    rgb(0x00a3a3),
    rgb(0xd6409f),
    rgb(0x5e6ad2),
];

pub fn palette(ui: &Ui) -> &'static Palette {
    if ui.visuals().dark_mode { &DARK } else { &LIGHT }
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

fn load_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    for (name, path) in WINDOWS_FONTS.iter().rev() {
        // Windows always ships these, but a missing one should only cost the nicer look.
        let Ok(bytes) = std::fs::read(path) else { continue };
        fonts.font_data.insert((*name).to_string(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, (*name).to_string());
    }
    let regular = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut bold: Vec<String> = regular.into_iter().filter(|name| name != "segoe").collect();
    bold.sort_by_key(|name| name != SEMIBOLD);
    let mut regular = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    regular.retain(|name| name != SEMIBOLD);
    fonts.families.insert(FontFamily::Proportional, regular);
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), bold);
    ctx.set_fonts(fonts);
}

fn visuals(base: Visuals, colors: &Palette) -> Visuals {
    let mut visuals = base;
    let radius = CornerRadius::same(CONTROL_RADIUS);
    let set = |widget: &mut egui::style::WidgetVisuals, fill: Color32, text: Color32| {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::NONE;
        widget.fg_stroke = Stroke::new(1.5, text);
        widget.corner_radius = radius;
        widget.expansion = 0.0;
    };
    set(&mut visuals.widgets.noninteractive, colors.card, colors.foreground);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colors.border);
    set(&mut visuals.widgets.inactive, colors.muted, colors.foreground);
    set(&mut visuals.widgets.hovered, colors.hover, colors.foreground);
    set(&mut visuals.widgets.active, colors.hover, colors.foreground);
    set(&mut visuals.widgets.open, colors.hover, colors.foreground);

    visuals.override_text_color = None;
    visuals.weak_text_color = Some(colors.secondary);
    visuals.hyperlink_color = colors.primary;
    visuals.panel_fill = colors.background;
    visuals.window_fill = colors.card;
    visuals.window_stroke = Stroke::new(1.0, colors.border);
    visuals.window_corner_radius = CornerRadius::same(RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(CONTROL_RADIUS);
    visuals.extreme_bg_color = colors.muted;
    visuals.text_edit_bg_color = Some(colors.muted);
    visuals.faint_bg_color = colors.muted;
    visuals.selection.bg_fill = colors.primary.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.5, colors.primary);
    visuals.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(70) };
    visuals
}

pub fn apply(ctx: &Context) {
    load_fonts(ctx);
    for (theme, base, colors) in [(Theme::Dark, Visuals::dark(), &DARK), (Theme::Light, Visuals::light(), &LIGHT)] {
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals(base.clone(), colors);
            style.text_styles = BTreeMap::from([
                (TextStyle::Small, FontId::proportional(12.0)),
                (TextStyle::Body, FontId::proportional(14.0)),
                (TextStyle::Button, FontId::proportional(14.0)),
                (TextStyle::Heading, semibold(24.0)),
                (TextStyle::Monospace, FontId::monospace(13.0)),
            ]);
            style.spacing.item_spacing = vec2(8.0, 8.0);
            style.spacing.button_padding = vec2(12.0, 7.0);
            style.spacing.interact_size.y = 32.0;
            style.spacing.combo_height = 280.0;
        });
    }
}

pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let colors = palette(ui);
    Frame::NONE
        .fill(colors.card)
        .stroke(Stroke::new(1.0, colors.border))
        .corner_radius(RADIUS)
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Quiet,
}

pub fn button(ui: &mut Ui, kind: ButtonKind, label: &str, enabled: bool) -> Response {
    let colors = palette(ui);
    let galley = ui.painter().layout_no_wrap(label.to_string(), semibold(14.0), Color32::PLACEHOLDER);
    let size = vec2(galley.size().x + 32.0, BUTTON_HEIGHT);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let hover = ui.ctx().animate_bool_responsive(response.id, enabled && response.hovered());
        let (fill, text, stroke) = match kind {
            ButtonKind::Primary => (colors.primary.lerp_to_gamma(colors.primary_hover, hover), colors.on_primary, Stroke::NONE),
            ButtonKind::Secondary => (colors.muted.lerp_to_gamma(colors.hover, hover), colors.foreground, Stroke::NONE),
            ButtonKind::Quiet => (
                Color32::TRANSPARENT.lerp_to_gamma(colors.muted, hover),
                colors.foreground,
                Stroke::new(1.0, colors.border),
            ),
        };
        let opacity = if enabled { 1.0 } else { 0.4 };
        ui.painter().rect(
            rect,
            CONTROL_RADIUS,
            fill.gamma_multiply(opacity),
            stroke,
            StrokeKind::Inside,
        );
        let position = rect.center() - galley.size() / 2.0;
        ui.painter().galley(position, galley, text.gamma_multiply(opacity));
    }
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

/// Returns true on the frame the switch was flipped.
pub fn switch(ui: &mut Ui, on: &mut bool) -> bool {
    let colors = palette(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::click());
    if response.clicked() {
        *on = !*on;
    }
    let travel = ui.ctx().animate_bool_responsive(response.id, *on);
    if ui.is_rect_visible(rect) {
        let track = colors.hover.lerp_to_gamma(colors.primary, travel);
        ui.painter().rect_filled(rect, 12.0, track);
        let x = egui::lerp((rect.left() + 12.0)..=(rect.right() - 12.0), travel);
        ui.painter().circle_filled(pos2(x, rect.center().y), 9.0, Color32::WHITE);
    }
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, options: &[(T, &str)], selected: &mut T) -> bool {
    let colors = palette(ui);
    let mut changed = false;
    Frame::NONE.fill(colors.muted).corner_radius(CONTROL_RADIUS).inner_margin(Margin::same(3)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (value, label) in options {
                let active = *value == *selected;
                let color = if active { colors.foreground } else { colors.secondary };
                let galley = ui.painter().layout_no_wrap((*label).to_string(), semibold(13.0), color);
                let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 24.0, 28.0), Sense::click());
                if active {
                    ui.painter().rect_filled(rect, 6.0, colors.card);
                }
                ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
                if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && !active {
                    *selected = *value;
                    changed = true;
                }
            }
        });
    });
    changed
}

pub fn pill(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), semibold(12.0), color);
    let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 26.0, 24.0), Sense::hover());
    ui.painter().rect_filled(rect, 12.0, color.gamma_multiply(0.14));
    ui.painter().circle_filled(pos2(rect.left() + 12.0, rect.center().y), 3.0, color);
    let text_at = pos2(rect.left() + 18.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_at, galley, color);
}

/// Text cut off with an ellipsis so it fits on one line.
pub fn one_line(text: &str, font: FontId, color: Color32, width: f32) -> LayoutJob {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat::simple(font, color));
    job.wrap = TextWrapping::truncate_at_width(width);
    job
}

pub fn avatar(painter: &Painter, center: Pos2, radius: f32, name: &str) {
    let hash = name.bytes().fold(0usize, |sum, byte| sum.wrapping_mul(31).wrapping_add(usize::from(byte)));
    painter.circle_filled(center, radius, AVATAR_COLORS[hash % AVATAR_COLORS.len()]);
    let letter: String = name.chars().next().map_or_else(|| "?".to_string(), |c| c.to_uppercase().collect());
    painter.text(center, egui::Align2::CENTER_CENTER, letter, semibold(radius * 0.9), Color32::WHITE);
}

pub fn logo(painter: &Painter, rect: Rect) {
    let scale = rect.width() / 32.0;
    let at = |x: f32, y: f32| rect.min + vec2(x, y) * scale;
    painter.rect_filled(rect, 8.0 * scale, rgb(0x3ddc97));
    let ink = rgb(0x06281b);
    let phone = Rect::from_min_max(at(10.5, 5.5), at(21.5, 26.5));
    painter.rect_stroke(phone, 2.5 * scale, Stroke::new(2.0 * scale, ink), StrokeKind::Middle);
    painter.add(egui::Shape::convex_polygon(vec![at(14.0, 13.2), at(18.8, 16.0), at(14.0, 18.8)], ink, Stroke::NONE));
}

#[derive(Clone, Copy)]
pub enum Glyph {
    Phone,
    Grid,
    Gear,
    Wifi,
    Search,
    Folder,
    File,
    Bell,
}

fn arc(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    const STEPS: usize = 16;
    (0..=STEPS)
        .map(|step| {
            let angle = from + (to - from) * step as f32 / STEPS as f32;
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

/// Draws a line icon that fills a `size` by `size` square around `center`.
pub fn glyph(painter: &Painter, kind: Glyph, center: Pos2, size: f32, color: Color32) {
    let scale = size / 20.0;
    let stroke = Stroke::new(1.6 * scale.max(0.9), color);
    let at = |x: f32, y: f32| center + vec2(x, y) * scale;
    match kind {
        Glyph::Phone => {
            let body = Rect::from_center_size(center, Vec2::new(11.0, 18.0) * scale);
            painter.rect_stroke(body, 2.5 * scale, stroke, StrokeKind::Middle);
            painter.line_segment([at(-1.5, 6.0), at(1.5, 6.0)], stroke);
        }
        Glyph::Grid => {
            for (x, y) in [(-4.5, -4.5), (4.5, -4.5), (-4.5, 4.5), (4.5, 4.5)] {
                let cell = Rect::from_center_size(at(x, y), Vec2::splat(6.5 * scale));
                painter.rect_stroke(cell, 1.5 * scale, stroke, StrokeKind::Middle);
            }
        }
        Glyph::Gear => {
            painter.circle_stroke(center, 3.0 * scale, stroke);
            painter.circle_stroke(center, 6.5 * scale, stroke);
            for tooth in 0..8 {
                let angle = tooth as f32 * FRAC_PI_4;
                let direction = vec2(angle.cos(), angle.sin());
                painter.line_segment([center + direction * 6.5 * scale, center + direction * 9.0 * scale], stroke);
            }
        }
        Glyph::Wifi => {
            let origin = at(0.0, 7.0);
            for radius in [6.0, 11.0, 16.0] {
                let span = FRAC_PI_4 * 0.95;
                let points = arc(origin, radius * scale, -FRAC_PI_2 - span, -FRAC_PI_2 + span);
                painter.add(egui::Shape::line(points, stroke));
            }
            painter.circle_filled(origin, 1.6 * scale, color);
        }
        Glyph::Search => {
            painter.circle_stroke(at(-1.5, -1.5), 6.0 * scale, stroke);
            painter.line_segment([at(3.0, 3.0), at(8.0, 8.0)], stroke);
        }
        Glyph::Folder => {
            painter.add(egui::Shape::line(vec![at(-8.0, -3.0), at(-8.0, -7.0), at(-2.5, -7.0), at(0.0, -3.5)], stroke));
            painter.rect_stroke(Rect::from_min_max(at(-8.0, -3.5), at(8.0, 7.0)), 2.0 * scale, stroke, StrokeKind::Middle);
        }
        Glyph::File => {
            painter.add(egui::Shape::closed_line(vec![at(-6.0, -8.0), at(2.0, -8.0), at(6.0, -4.0), at(6.0, 8.0), at(-6.0, 8.0)], stroke));
            painter.line_segment([at(2.0, -8.0), at(2.0, -4.0)], stroke);
            painter.line_segment([at(2.0, -4.0), at(6.0, -4.0)], stroke);
        }
        Glyph::Bell => {
            painter.add(egui::Shape::line(arc(at(0.0, -2.0), 5.5 * scale, std::f32::consts::PI, std::f32::consts::TAU), stroke));
            painter.line_segment([at(-5.5, -2.0), at(-7.0, 5.0)], stroke);
            painter.line_segment([at(5.5, -2.0), at(7.0, 5.0)], stroke);
            painter.line_segment([at(-8.0, 5.0), at(8.0, 5.0)], stroke);
            painter.circle_filled(at(0.0, 8.0), 1.6 * scale, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_app_always_gets_the_same_badge_color() {
        let index = |name: &str| name.bytes().fold(0usize, |sum, byte| sum.wrapping_mul(31).wrapping_add(usize::from(byte))) % 8;
        assert_eq!(index("Calculator"), index("Calculator"));
    }

    #[test]
    fn colors_decode_from_hex() {
        assert_eq!(rgb(0x3ddc97), Color32::from_rgb(0x3d, 0xdc, 0x97));
    }
}
