//! In-app image slideshow and tile wall overlays.
use eframe::egui::{self, Color32, Sense, TextureHandle};
use std::path::PathBuf;

use super::widgets::{ease_out_cubic, load_texture};
use super::SuijiApp;

const SLIDE_CROSSFADE: f32 = 0.18;

impl SuijiApp {
    pub(super) fn tick_slideshow(&mut self, ctx: &egui::Context, dt: f32, paths: &[PathBuf], interval: f32) {
        if paths.is_empty() {
            return;
        }
        if self.slide_index >= paths.len() {
            self.slide_index = 0;
        }
        // Keyboard
        let mut step: i32 = 0;
        let mut stop = false;
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                stop = true;
            }
            if i.key_pressed(egui::Key::Space) {
                self.slide_paused = !self.slide_paused;
            }
            if i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::N) {
                step = 1;
            }
            if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::P) {
                step = -1;
            }
        });
        if stop {
            self.session.stop();
            self.clear_slides();
            self.show_toast("已结束幻灯");
            return;
        }
        if step != 0 {
            let n = paths.len() as i32;
            self.slide_index = ((self.slide_index as i32 + step).rem_euclid(n)) as usize;
            self.slide_elapsed = 0.0;
            self.begin_slide_change();
        } else if !self.slide_paused {
            self.slide_elapsed += dt;
            if self.slide_elapsed >= interval {
                self.slide_elapsed = 0.0;
                self.slide_index = (self.slide_index + 1) % paths.len();
                self.begin_slide_change();
            }
        }
        // Crossfade progress
        if self.slide_fade < 1.0 {
            self.slide_fade = (self.slide_fade + dt / SLIDE_CROSSFADE).min(1.0);
            if self.slide_fade >= 1.0 {
                self.slide_prev = None;
            }
        }
        // Load texture for current
        let path = &paths[self.slide_index];
        let need = self
            .slide_tex
            .as_ref()
            .map(|(p, _)| p != path)
            .unwrap_or(true);
        if need {
            let key = format!("slide:{}", path.display());
            if let Some(tex) = load_texture(ctx, &key, path) {
                self.slide_tex = Some((path.clone(), tex));
            }
        }
        ctx.request_repaint();
    }

    /// Keep previous frame for short crossfade (prevents hard cut).
    pub(super) fn begin_slide_change(&mut self) {
        if let Some(cur) = self.slide_tex.take() {
            self.slide_prev = Some(cur);
            self.slide_fade = 0.0;
        } else {
            self.slide_fade = 1.0;
        }
    }

    pub(super) fn clear_slides(&mut self) {
        self.slide_tex = None;
        self.slide_prev = None;
        self.slide_fade = 1.0;
    }

    pub(super) fn draw_slideshow_overlay(&mut self, ctx: &egui::Context, paths: &[PathBuf], interval: u8) {
        let n = paths.len();
        if n == 0 {
            return;
        }
        if self.slide_index >= n {
            self.slide_index = 0;
        }
        egui::Area::new(egui::Id::new("slideshow_overlay"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.allocate_ui_at_rect(screen, |ui| {
                    ui.painter()
                        .rect_filled(screen, 0.0, Color32::from_rgb(12, 12, 14));

                    let paint_slide = |ui: &mut egui::Ui, tex: &TextureHandle, alpha: f32| {
                        if alpha <= 0.01 {
                            return;
                        }
                        let size = tex.size_vec2();
                        let fit = (screen.width() / size.x)
                            .min(screen.height() / size.y)
                            .min(1.5);
                        let draw = size * fit;
                        let rect = egui::Rect::from_center_size(screen.center(), draw);
                        let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
                        ui.painter().image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(
                                egui::pos2(0.0, 0.0),
                                egui::pos2(1.0, 1.0),
                            ),
                            Color32::from_rgba_unmultiplied(255, 255, 255, a),
                        );
                    };

                    let fade = ease_out_cubic(self.slide_fade.clamp(0.0, 1.0));
                    if let Some((_, tex)) = &self.slide_prev {
                        paint_slide(ui, tex, 1.0 - fade);
                    }
                    if let Some((_, tex)) = &self.slide_tex {
                        paint_slide(ui, tex, if self.slide_prev.is_some() { fade } else { 1.0 });
                    } else if self.slide_prev.is_none() {
                        ui.painter().text(
                            screen.center(),
                            egui::Align2::CENTER_CENTER,
                            "加载图片…",
                            egui::FontId::proportional(18.0),
                            Color32::from_gray(180),
                        );
                    }

                    // HUD — avoid ←/→ glyphs (CJK font tofu risk)
                    let hud = format!(
                        "{}/{}  ·  {}s  ·  {}  ·  空格暂停  左右切换  Esc 结束",
                        self.slide_index + 1,
                        n,
                        interval,
                        if self.slide_paused {
                            "已暂停"
                        } else {
                            "播放中"
                        }
                    );
                    ui.painter().text(
                        egui::pos2(screen.left() + 16.0, screen.bottom() - 28.0),
                        egui::Align2::LEFT_BOTTOM,
                        hud,
                        egui::FontId::proportional(14.0),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 200),
                    );

                    // Click zones: left prev, right next, center pause
                    let resp = ui.interact(screen, ui.id().with("slide_click"), Sense::click());
                    if resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let x = (pos.x - screen.left()) / screen.width();
                            if x < 0.28 {
                                self.slide_index = (self.slide_index + n - 1) % n;
                                self.slide_elapsed = 0.0;
                                self.begin_slide_change();
                            } else if x > 0.72 {
                                self.slide_index = (self.slide_index + 1) % n;
                                self.slide_elapsed = 0.0;
                                self.begin_slide_change();
                            } else {
                                self.slide_paused = !self.slide_paused;
                            }
                        }
                    }
                });
            });
    }

    pub(super) fn draw_wall_overlay(&mut self, ctx: &egui::Context, paths: &[PathBuf]) {
        let n = paths.len();
        if n == 0 {
            return;
        }
        // Ensure textures for wall
        self.ensure_thumbs(paths, ctx);

        egui::Area::new(egui::Id::new("wall_overlay"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.painter()
                    .rect_filled(screen, 0.0, Color32::from_rgb(12, 12, 14));

                // If one image focused (slide_tex set), show it large; click to back
                if let Some((ref path, ref tex)) = self.slide_tex.clone() {
                    let size = tex.size_vec2();
                    let fit = (screen.width() / size.x)
                        .min(screen.height() / size.y)
                        .min(1.5);
                    let draw = size * fit;
                    let rect = egui::Rect::from_center_size(screen.center(), draw);
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ui.painter().text(
                        egui::pos2(screen.center().x, screen.bottom() - 24.0),
                        egui::Align2::CENTER_BOTTOM,
                        format!("{name}  ·  点击返回平铺 · Esc 结束"),
                        egui::FontId::proportional(13.0),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 200),
                    );
                    let resp = ui.interact(screen, ui.id().with("wall_focus"), Sense::click());
                    if resp.clicked() {
                        self.slide_tex = None;
                    }
                    return;
                }

                let (rows, cols) = crate::tiler::rows_cols(n);
                let margin = 16.0;
                let gap = 8.0;
                let area = egui::Rect::from_min_max(
                    screen.min + egui::vec2(margin, margin + 8.0),
                    screen.max - egui::vec2(margin, margin + 32.0),
                );
                let cell_w = ((area.width() - gap * (cols as f32 - 1.0)) / cols as f32).max(40.0);
                let cell_h = ((area.height() - gap * (rows as f32 - 1.0)) / rows as f32).max(40.0);

                for (idx, path) in paths.iter().enumerate() {
                    let r = idx / cols;
                    let c = idx % cols;
                    let x = area.left() + c as f32 * (cell_w + gap);
                    let y = area.top() + r as f32 * (cell_h + gap);
                    let cell = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(cell_w, cell_h));
                    ui.painter()
                        .rect_filled(cell, 2.0, Color32::from_rgb(28, 28, 32));
                    let key = path.to_string_lossy().to_string();
                    if let Some(tex) = self.textures.get(&key) {
                        let size = tex.size_vec2();
                        let fit = (cell.width() / size.x).min(cell.height() / size.y);
                        let draw = size * fit;
                        let img_r = egui::Rect::from_center_size(cell.center(), draw);
                        ui.painter().image(
                            tex.id(),
                            img_r,
                            egui::Rect::from_min_max(
                                egui::pos2(0.0, 0.0),
                                egui::pos2(1.0, 1.0),
                            ),
                            Color32::WHITE,
                        );
                    }
                    let resp = ui.interact(cell, ui.id().with("wall_cell").with(idx), Sense::click());
                    if resp.clicked() {
                        let k = format!("slide:{}", path.display());
                        if let Some(tex) = load_texture(ctx, &k, path) {
                            self.slide_tex = Some((path.clone(), tex));
                        }
                    }
                }

                ui.painter().text(
                    egui::pos2(screen.center().x, screen.bottom() - 12.0),
                    egui::Align2::CENTER_BOTTOM,
                    format!("平铺墙 · {n} 张 · 点击放大 · Esc 结束"),
                    egui::FontId::proportional(13.0),
                    Color32::from_rgba_unmultiplied(255, 255, 255, 200),
                );
            });
    }
}

