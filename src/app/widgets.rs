//! Shared magazine UI widgets (buttons, chips, preview cell).
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, TextureHandle, Vec2};
use std::path::Path;

use crate::session::SessionPhase;
use super::theme::{BG, BG_SOFT, FAINT, INK, LINE, LINE_STRONG, MUTED, ON_INK};
pub(crate) fn load_texture(ctx: &egui::Context, id: &str, path: &Path) -> Option<TextureHandle> {
    let img = image::open(path).ok()?.into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = img.into_raw();
    let color = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture(id, color, egui::TextureOptions::LINEAR))
}

pub(crate) fn is_image_path(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif" | "tif" | "tiff"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn mode_chip(ui: &mut egui::Ui, label: &str, active: bool, on_click: impl FnOnce()) {
    let fill = if active { INK } else { BG };
    let fg = if active { ON_INK } else { MUTED };
    let stroke = if active {
        Stroke::new(1.0, INK)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    // Compact enough for workbench toolbar; still easy to hit.
    let btn = egui::Button::new(RichText::new(label).size(13.0).color(fg))
        .fill(fill)
        .stroke(stroke)
        .min_size(Vec2::new(56.0, 32.0));
    if ui.add(btn).clicked() {
        on_click();
    }
}

pub(crate) fn status_pill(ui: &mut egui::Ui, phase: SessionPhase, message: &str) {
    let (text, bg, fg) = match phase {
        SessionPhase::Idle => {
            if message.contains("预览") {
                ("待播放", Color32::from_rgb(0xE7, 0xE0, 0xD6), INK)
            } else {
                ("就绪", BG_SOFT, MUTED)
            }
        }
        SessionPhase::Starting => ("启动中", Color32::from_rgb(0xE7, 0xE0, 0xD6), INK),
        SessionPhase::Playing => ("播放中", INK, ON_INK),
        SessionPhase::Stopping => ("关闭中", Color32::from_rgb(0xE7, 0xE0, 0xD6), INK),
    };
    let label = if message.chars().count() < 18 && phase == SessionPhase::Idle {
        // Prefer short status from message when preview-ready
        if message.contains("预览就绪") {
            "待播放"
        } else if message.chars().count() < 12 {
            message
        } else {
            text
        }
    } else {
        text
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(11.0),
        fg,
    );
    let pad = Vec2::new(10.0, 4.0);
    let size = galley.size() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_stroke(
        rect,
        20.0,
        Stroke::new(1.0, LINE_STRONG),
        egui::StrokeKind::Inside,
    );
    ui.painter().rect_filled(rect, 20.0, bg);
    ui.painter().galley(
        egui::pos2(rect.left() + pad.x, rect.top() + pad.y),
        galley,
        fg,
    );
}

/// One bound row: label + − value +  (vertically centered, fixed height)
pub(crate) fn bound_stepper(
    ui: &mut egui::Ui,
    label: &str,
    value: usize,
    on_dec: impl FnOnce(),
    on_inc: impl FnOnce(),
) {
    let row_h = 36.0;
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), row_h),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_height(row_h);
            ui.label(
                RichText::new(label)
                    .size(13.0)
                    .color(MUTED),
            );
            ui.add_space(8.0);
            if small_step_btn(ui, "−").clicked() {
                on_dec();
            }
            // Fixed-width number so 下限/上限 columns stay aligned
            let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, row_h), Sense::hover());
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{value}"),
                egui::FontId::proportional(16.0),
                INK,
            );
            if small_step_btn(ui, "+").clicked() {
                on_inc();
            }
        },
    );
}

pub(crate) fn small_step_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let size = Vec2::new(34.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    let draw = press_draw_rect(rect, &resp, true);
    let stroke = if pressed || resp.hovered() {
        Stroke::new(1.0, MUTED)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    if pressed {
        ui.painter().rect_filled(draw, 2.0, BG_SOFT);
    }
    ui.painter()
        .rect_stroke(draw, 2.0, stroke, egui::StrokeKind::Inside);
    ui.painter().text(
        draw.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(16.0),
        INK,
    );
    resp
}

pub(crate) fn toggle(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let size = Vec2::new(36.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let bg = if *on { INK } else { LINE_STRONG };
    ui.painter().rect_filled(rect, 10.0, bg);
    let knob_x = if *on {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    ui.painter()
        .circle_filled(egui::pos2(knob_x, rect.center().y), 8.0, ON_INK);
    resp.clicked()
}

/// Visual press: shrink draw rect ~3% (scale 0.97 feel) without changing layout.
pub(crate) fn press_draw_rect(rect: egui::Rect, resp: &egui::Response, enabled: bool) -> egui::Rect {
    if enabled && resp.is_pointer_button_down_on() {
        rect.shrink(1.5)
    } else {
        rect
    }
}

pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub(crate) fn primary_btn(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    primary_btn_w(ui, ui.available_width(), 48.0, text, enabled)
}

/// Fixed-size primary for the main-stage action strip.
pub(crate) fn primary_btn_w(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    text: &str,
    enabled: bool,
) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if !enabled {
        resp = resp.on_disabled_hover_text("请先完成片库设置并确保有视频");
    }
    let pressed = enabled && resp.is_pointer_button_down_on();
    let bg = if !enabled {
        Color32::from_rgb(0xA8, 0xA2, 0x9E)
    } else if pressed {
        Color32::from_rgb(0x14, 0x12, 0x11)
    } else if resp.hovered() {
        Color32::from_rgb(0x29, 0x25, 0x24)
    } else {
        INK
    };
    let draw = press_draw_rect(rect, &resp, enabled);
    ui.painter().rect_filled(draw, 2.0, bg);
    let font = if height < 40.0 { 13.5 } else { 15.0 };
    ui.painter().text(
        draw.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(font),
        ON_INK,
    );
    if enabled {
        resp
    } else {
        ui.interact(rect, ui.id().with("disabled_primary"), Sense::hover())
    }
}

pub(crate) fn mini_text_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let pad = Vec2::new(10.0, 5.0);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(12.5),
        INK,
    );
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    let draw = press_draw_rect(rect, &resp, true);
    let stroke = if pressed {
        Stroke::new(1.0, INK)
    } else if resp.hovered() {
        Stroke::new(1.0, INK)
    } else {
        Stroke::new(1.0, FAINT)
    };
    if pressed {
        ui.painter()
            .rect_filled(draw, 1.0, Color32::from_rgb(0xE7, 0xE0, 0xD6));
    } else if resp.hovered() {
        ui.painter().rect_filled(draw, 1.0, BG_SOFT);
    }
    ui.painter()
        .rect_stroke(draw, 1.0, stroke, egui::StrokeKind::Inside);
    let galley_pos = egui::pos2(
        draw.center().x - galley.size().x * 0.5,
        draw.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(galley_pos, galley, INK);
    resp
}

/// Result of the multi-select batch bar (idle preview).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionBarAction {
    Replace,
    Remove,
    Blacklist,
    Clear,
}

/// Quiet paper selection strip: `3` · 换 · 剔除 · 拉黑 · 取消
pub(crate) fn selection_bar(ui: &mut egui::Ui, selected: usize) -> Option<SelectionBarAction> {
    if selected == 0 {
        return None;
    }
    let mut action = None;
    egui::Frame::NONE
        .fill(BG_SOFT)
        .stroke(Stroke::new(1.0, LINE))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .corner_radius(2.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.set_min_height(28.0);

                // Count only — no “已选” noise
                ui.label(
                    RichText::new(format!("{selected}"))
                        .size(14.0)
                        .color(INK)
                        .strong(),
                );
                ui.label(RichText::new("项").size(12.0).color(MUTED));

                let (div, _) = ui.allocate_exact_size(Vec2::new(1.0, 16.0), Sense::hover());
                ui.painter().line_segment(
                    [
                        egui::pos2(div.center().x, div.top() + 1.0),
                        egui::pos2(div.center().x, div.bottom() - 1.0),
                    ],
                    Stroke::new(1.0, LINE_STRONG),
                );

                if bar_solid_btn(ui, "换").clicked() {
                    action = Some(SelectionBarAction::Replace);
                }
                if bar_outline_btn(ui, "剔除").clicked() {
                    action = Some(SelectionBarAction::Remove);
                }
                if bar_outline_btn(ui, "拉黑").clicked() {
                    action = Some(SelectionBarAction::Blacklist);
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let clear = ui.add(
                        egui::Label::new(RichText::new("取消").size(12.5).color(MUTED))
                            .sense(Sense::click()),
                    );
                    if clear.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if clear.clicked() {
                        action = Some(SelectionBarAction::Clear);
                    }
                });
            });
        });
    action
}

fn bar_solid_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let pad = Vec2::new(16.0, 5.0);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(13.0),
        ON_INK,
    );
    let size = Vec2::new((galley.size().x + pad.x * 2.0).max(40.0), 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    let draw = press_draw_rect(rect, &resp, true);
    let bg = if pressed {
        Color32::from_rgb(0x14, 0x12, 0x11)
    } else if resp.hovered() {
        Color32::from_rgb(0x29, 0x25, 0x24)
    } else {
        INK
    };
    ui.painter().rect_filled(draw, 2.0, bg);
    ui.painter().galley(
        egui::pos2(
            draw.center().x - galley.size().x * 0.5,
            draw.center().y - galley.size().y * 0.5,
        ),
        galley,
        ON_INK,
    );
    resp
}

fn bar_outline_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let pad = Vec2::new(12.0, 5.0);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(13.0),
        INK,
    );
    let size = Vec2::new((galley.size().x + pad.x * 2.0).max(44.0), 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    let draw = press_draw_rect(rect, &resp, true);
    let fill = if pressed {
        Color32::from_rgb(0xD6, 0xD0, 0xC6)
    } else if resp.hovered() {
        BG
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if pressed || resp.hovered() {
        Stroke::new(1.0, INK)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    ui.painter().rect_filled(draw, 2.0, fill);
    ui.painter()
        .rect_stroke(draw, 2.0, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(
        egui::pos2(
            draw.center().x - galley.size().x * 0.5,
            draw.center().y - galley.size().y * 0.5,
        ),
        galley,
        INK,
    );
    resp
}

pub(crate) fn secondary_btn(ui: &mut egui::Ui, width: f32, text: &str, enabled: bool) -> egui::Response {
    // Match main-stage primary strip height (32) for aligned action row.
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    let pressed = enabled && resp.is_pointer_button_down_on();
    let draw = press_draw_rect(rect, &resp, enabled);
    let stroke = if !enabled {
        Stroke::new(1.0, LINE)
    } else if pressed {
        Stroke::new(1.2, INK)
    } else if resp.hovered() {
        Stroke::new(1.0, MUTED)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    let fg = if enabled { INK } else { FAINT };
    if pressed {
        ui.painter().rect_filled(draw, 2.0, BG_SOFT);
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(draw, 2.0, BG);
    }
    ui.painter()
        .rect_stroke(draw, 2.0, stroke, egui::StrokeKind::Inside);
    ui.painter().text(
        draw.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(13.5),
        fg,
    );
    if enabled {
        resp
    } else {
        ui.interact(rect, ui.id().with(text).with("dis"), Sense::hover())
    }
}

pub(crate) fn link_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Label::new(RichText::new(text).size(13.0).color(MUTED)).sense(Sense::click()),
    )
}

#[derive(Clone, Copy)]
pub(crate) enum IconKind {
    Settings,
    Rescan,
    Tray,
    Pin,
}

pub(crate) fn icon_btn(ui: &mut egui::Ui, kind: IconKind, tip: &str) -> egui::Response {
    icon_btn_toggle(ui, kind, tip, false)
}

/// Magazine-style square icon button; `active` draws stronger (for pin on).
pub(crate) fn icon_btn_toggle(ui: &mut egui::Ui, kind: IconKind, tip: &str, active: bool) -> egui::Response {
    let size = Vec2::new(40.0, 40.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let pressed = resp.is_pointer_button_down_on();
    let draw = press_draw_rect(rect, &resp, true);
    let stroke = if pressed || active || hovered {
        Stroke::new(1.2, INK)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    let fill = if pressed {
        Color32::from_rgb(0xD6, 0xD0, 0xC6)
    } else if active {
        Color32::from_rgb(0xE7, 0xE0, 0xD6)
    } else if hovered {
        BG_SOFT
    } else {
        BG
    };
    ui.painter().rect_filled(draw, 2.0, fill);
    ui.painter()
        .rect_stroke(draw, 2.0, stroke, egui::StrokeKind::Inside);

    let c = draw.center();
    let ink = if active || hovered || pressed {
        INK
    } else {
        MUTED
    };
    let s = Stroke::new(1.4, ink);

    match kind {
        IconKind::Settings => {
            ui.painter().circle_stroke(c, 7.0, s);
            ui.painter().circle_filled(c, 2.2, ink);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.0;
                let inner = c + Vec2::angled(a) * 5.5;
                let outer = c + Vec2::angled(a) * 9.0;
                ui.painter().line_segment([inner, outer], s);
            }
        }
        IconKind::Rescan => {
            let r = 8.0;
            let mut pts = Vec::new();
            for i in 0..=14 {
                let t = i as f32 / 14.0;
                let a = -0.2 + t * (std::f32::consts::TAU * 0.72);
                pts.push(c + Vec2::new(a.cos() * r, a.sin() * r));
            }
            for w in pts.windows(2) {
                ui.painter().line_segment([w[0], w[1]], s);
            }
            if let Some(&p) = pts.last() {
                let a = -0.2 + 0.72 * std::f32::consts::TAU;
                let dir = Vec2::new(a.cos(), a.sin());
                let n = Vec2::new(-dir.y, dir.x);
                ui.painter().line_segment([p, p - dir * 4.0 + n * 3.0], s);
                ui.painter().line_segment([p, p - dir * 4.0 - n * 3.0], s);
            }
        }
        IconKind::Tray => {
            let box_r =
                egui::Rect::from_center_size(c + Vec2::new(0.0, -2.0), Vec2::new(14.0, 10.0));
            ui.painter()
                .rect_stroke(box_r, 1.0, s, egui::StrokeKind::Outside);
            ui.painter().line_segment(
                [
                    egui::pos2(box_r.left() + 2.0, box_r.bottom() - 2.5),
                    egui::pos2(box_r.right() - 2.0, box_r.bottom() - 2.5),
                ],
                s,
            );
            let tip_pt = egui::pos2(c.x, c.y + 11.0);
            ui.painter()
                .line_segment([egui::pos2(c.x - 4.0, c.y + 6.0), tip_pt], s);
            ui.painter()
                .line_segment([egui::pos2(c.x + 4.0, c.y + 6.0), tip_pt], s);
        }
        IconKind::Pin => {
            // Simple pin / thumbtack
            ui.painter().circle_stroke(c + Vec2::new(0.0, -2.0), 4.5, s);
            ui.painter().line_segment(
                [
                    egui::pos2(c.x, c.y + 2.0),
                    egui::pos2(c.x, c.y + 10.0),
                ],
                s,
            );
            ui.painter().line_segment(
                [
                    egui::pos2(c.x - 5.0, c.y + 1.0),
                    egui::pos2(c.x + 5.0, c.y + 1.0),
                ],
                s,
            );
        }
    }

    resp.on_hover_text(tip)
}

pub(crate) fn sized_outline_button(text: &str, width: f32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(INK))
        .fill(BG)
        .stroke(Stroke::new(1.0, FAINT))
        .min_size(Vec2::new(width, 32.0))
}

/// Preview grid cell. When `selectable`, click toggles selection (caller updates state).
/// Returns the response so the app can handle click / hover.
pub(crate) fn preview_cell(
    ui: &mut egui::Ui,
    w: f32,
    h: f32,
    label: &str,
    texture: Option<&TextureHandle>,
    selected: bool,
    selectable: bool,
) -> egui::Response {
    let sense = if selectable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), sense);
    if let Some(tex) = texture {
        let size = tex.size_vec2();
        let fit = (rect.width() / size.x).min(rect.height() / size.y);
        let draw = size * fit;
        let img_rect = egui::Rect::from_center_size(rect.center(), draw);
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_rgb(0x1C, 0x19, 0x17));
        ui.painter().image(
            tex.id(),
            img_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_rgb(0xE7, 0xE5, 0xE4));
    }

    // Selection / hover chrome
    if selected {
        ui.painter().rect_filled(
            rect,
            0.0,
            Color32::from_rgba_unmultiplied(0x2A, 0x24, 0x1F, 48),
        );
        ui.painter().rect_stroke(
            rect,
            0.0,
            Stroke::new(2.5, INK),
            egui::StrokeKind::Inside,
        );
        // Corner selected badge (drawn, no missing-glyph chars)
        let badge = 16.0_f32.min(rect.width() * 0.2).min(rect.height() * 0.2).max(12.0);
        let br = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 5.0, rect.top() + 5.0),
            Vec2::splat(badge),
        );
        ui.painter().rect_filled(br, 2.0, INK);
        // Simple white check: two short strokes
        let c = br.center();
        let s = badge * 0.22;
        ui.painter().line_segment(
            [
                egui::pos2(c.x - s * 1.1, c.y + s * 0.15),
                egui::pos2(c.x - s * 0.15, c.y + s * 0.95),
            ],
            Stroke::new(1.8, ON_INK),
        );
        ui.painter().line_segment(
            [
                egui::pos2(c.x - s * 0.15, c.y + s * 0.95),
                egui::pos2(c.x + s * 1.25, c.y - s * 0.85),
            ],
            Stroke::new(1.8, ON_INK),
        );
    } else {
        let stroke = if selectable && resp.hovered() {
            Stroke::new(1.5, MUTED)
        } else {
            Stroke::new(1.0, LINE_STRONG)
        };
        ui.painter()
            .rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
    }

    if !label.is_empty() {
        let short = truncate_path(label, 14);
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - 16.0),
            rect.max,
        );
        ui.painter()
            .rect_filled(bar, 0.0, Color32::from_rgba_unmultiplied(28, 25, 23, 160));
        ui.painter().text(
            egui::pos2(rect.left() + 4.0, rect.bottom() - 14.0),
            egui::Align2::LEFT_TOP,
            short,
            egui::FontId::proportional(11.0),
            ON_INK,
        );
    }

    if selectable {
        resp.on_hover_text(if selected { "取消" } else { "选中" })
    } else {
        resp
    }
}

pub(crate) fn truncate_path(s: &str, max_chars: usize) -> String {
    let display = file_title(s);
    let count = display.chars().count();
    if count <= max_chars {
        display
    } else {
        let take = max_chars.saturating_sub(1);
        let t: String = display.chars().take(take).collect();
        format!("{t}…")
    }
}

/// File title for UI lists: stem only, no extension / path noise.
pub(crate) fn file_title(s: &str) -> String {
    let p = Path::new(s);
    p.file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .or_else(|| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .filter(|n| !n.is_empty())
        })
        .unwrap_or_else(|| s.to_string())
}

/// Sidebar list row: one line — index · title… · actions.
/// Keeps workbench density; actions sit on the right without a second empty row.
/// Call `actions` with right-to-left order (first drawn = rightmost).
pub(crate) fn sidebar_list_row(
    ui: &mut egui::Ui,
    index: usize,
    title: &str,
    // Reserve width for action buttons so the title truncates cleanly.
    actions_w: f32,
    actions: impl FnOnce(&mut egui::Ui),
) {
    sidebar_list_row_select(ui, index, title, None, actions_w, actions);
}

/// Like [`sidebar_list_row`], optional checkbox for multi-select preview.
pub(crate) fn sidebar_list_row_select(
    ui: &mut egui::Ui,
    index: usize,
    title: &str,
    selected: Option<&mut bool>,
    actions_w: f32,
    actions: impl FnOnce(&mut egui::Ui),
) {
    let checked = selected.as_ref().map(|s| **s).unwrap_or(false);
    let fill = if checked {
        Color32::from_rgb(0xE7, 0xE0, 0xD6)
    } else {
        BG_SOFT
    };
    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, if checked { MUTED } else { LINE }))
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                ui.spacing_mut().item_spacing.x = 4.0;

                if let Some(sel) = selected {
                    ui.checkbox(sel, "");
                }

                ui.label(
                    RichText::new(format!("{index}."))
                        .size(11.0)
                        .color(MUTED)
                        .strong(),
                );

                let title_w = (ui.available_width() - actions_w - 4.0).max(36.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(title_w, 20.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_max_width(title_w);
                        ui.add(
                            egui::Label::new(RichText::new(title).size(12.0).color(INK)).truncate(),
                        );
                    },
                );

                ui.with_layout(Layout::right_to_left(Align::Center), actions);
            });
        });
}

/// Compact text action for dense sidebar rows (smaller than mini_text_btn).
pub(crate) fn row_action_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let pad = Vec2::new(7.0, 3.0);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(11.5),
        INK,
    );
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    let draw = press_draw_rect(rect, &resp, true);
    let fill = if pressed {
        Color32::from_rgb(0xD6, 0xD0, 0xC6)
    } else if resp.hovered() {
        BG
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if resp.hovered() || pressed {
        Stroke::new(1.0, MUTED)
    } else {
        Stroke::new(1.0, LINE)
    };
    ui.painter().rect_filled(draw, 2.0, fill);
    ui.painter()
        .rect_stroke(draw, 2.0, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(
        egui::pos2(draw.left() + pad.x, draw.top() + pad.y),
        galley,
        INK,
    );
    resp
}
