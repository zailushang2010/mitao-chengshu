use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke, Style, Visuals};
use std::path::PathBuf;
use std::sync::Arc;

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
    install_cjk_fonts(ctx);

    let mut style = Style::default();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(0);
    // Larger base for readability on the enlarged window
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(16.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(15.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(28.0, FontFamily::Proportional),
    );
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 12.0);
    style.visuals = magazine_visuals();
    ctx.set_style(style);
}

/// Load a system Chinese font as the primary proportional face so UI text is not tofu/mojibake.
fn install_cjk_fonts(ctx: &egui::Context) {
    let Some((name, bytes, index)) = load_best_cjk_font() else {
        eprintln!("蜜桃成熟: no CJK font found under %WINDIR%\\Fonts");
        return;
    };

    let mut fonts = FontDefinitions::default();
    let mut data = FontData::from_owned(bytes);
    data.index = index;
    fonts.font_data.insert(name.clone(), Arc::new(data));

    // Put CJK font first so Chinese glyphs resolve; Latin falls back to default fonts after.
    if let Some(fam) = fonts.families.get_mut(&FontFamily::Proportional) {
        fam.insert(0, name.clone());
    }
    if let Some(fam) = fonts.families.get_mut(&FontFamily::Monospace) {
        fam.insert(0, name);
    }

    ctx.set_fonts(fonts);
}

/// Candidates: (path relative to Fonts, ttc index). Prefer clean single-file TTF, then YaHei TTC.
fn load_best_cjk_font() -> Option<(String, Vec<u8>, u32)> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let fonts_dir = PathBuf::from(windir).join("Fonts");

    // (filename, font name key, collection index)
    let candidates: &[(&str, &str, u32)] = &[
        ("msyh.ttc", "MicrosoftYaHei", 0),     // 微软雅黑
        ("msyhbd.ttc", "MicrosoftYaHeiBold", 0),
        ("msyh.ttf", "MicrosoftYaHeiTtf", 0),
        ("simhei.ttf", "SimHei", 0),           // 黑体
        ("simsun.ttc", "SimSun", 0),           // 宋体
        ("msjh.ttc", "MicrosoftJhengHei", 0),  // 微软正黑
        ("Deng.ttf", "DengXian", 0),           // 等线
        ("Dengb.ttf", "DengXianBold", 0),
        ("NotoSansSC-Regular.otf", "NotoSansSC", 0),
        ("SourceHanSansSC-Regular.otf", "SourceHanSans", 0),
    ];

    for (file, name, index) in candidates {
        let path = fonts_dir.join(file);
        if let Ok(bytes) = std::fs::read(&path) {
            if bytes.len() > 1000 {
                eprintln!("蜜桃成熟: using CJK font {}", path.display());
                return Some((name.to_string(), bytes, *index));
            }
        }
    }
    None
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
