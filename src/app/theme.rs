use egui::{Color32, CornerRadius, Stroke, Style, Visuals};

/// 纸感杂志色板
pub const BG: Color32 = Color32::from_rgb(0xF7, 0xF3, 0xEC);
pub const BG_SOFT: Color32 = Color32::from_rgb(0xEF, 0xE9, 0xE0);
pub const INK: Color32 = Color32::from_rgb(0x1C, 0x19, 0x17);
pub const MUTED: Color32 = Color32::from_rgb(0x78, 0x71, 0x6C);
pub const FAINT: Color32 = Color32::from_rgb(0xA8, 0xA2, 0x9E);
pub const LINE: Color32 = Color32::from_rgb(0xE7, 0xE0, 0xD6);
pub const LINE_STRONG: Color32 = Color32::from_rgb(0xD6, 0xD3, 0xD1);
pub const ON_INK: Color32 = Color32::from_rgb(0xFA, 0xFA, 0xF9);

pub fn apply_magazine_style(ctx: &egui::Context) {
    let mut style = Style::default();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(0);
    style.visuals = magazine_visuals();
    ctx.set_style(style);
}

fn magazine_visuals() -> Visuals {
    let mut v = Visuals::light();
    v.override_text_color = Some(INK);
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = BG_SOFT;
    v.faint_bg_color = BG_SOFT;
    v.widgets.inactive.bg_fill = BG;
    v.widgets.inactive.weak_bg_fill = BG_SOFT;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, MUTED);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE_STRONG);
    v.widgets.hovered.bg_fill = BG_SOFT;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);
    v.widgets.active.bg_fill = INK;
    v.widgets.active.fg_stroke = Stroke::new(1.0, ON_INK);
    v.widgets.active.bg_stroke = Stroke::new(1.0, INK);
    v.selection.bg_fill = Color32::from_rgb(0xD6, 0xD3, 0xD1);
    v.selection.stroke = Stroke::new(1.0, INK);
    v.widgets.noninteractive.bg_fill = BG;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(0);
    v.menu_corner_radius = CornerRadius::same(2);
    v
}
