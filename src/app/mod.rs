mod theme;

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, Vec2};
use std::path::Path;

use crate::session::{SessionHandle, SessionPhase};
use theme::{BG, BG_SOFT, FAINT, INK, LINE, LINE_STRONG, MUTED, ON_INK};

pub struct SuijiApp {
    session: SessionHandle,
    show_settings: bool,
    pot_path_edit: String,
    status_tick: f32,
}

impl SuijiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, session: SessionHandle) -> Self {
        theme::apply_magazine_style(&cc.egui_ctx);
        let pot_path_edit = session.config_clone().potplayer_path;
        Self {
            session,
            show_settings: false,
            pot_path_edit,
            status_tick: 0.0,
        }
    }
}

impl eframe::App for SuijiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.status_tick += ctx.input(|i| i.unstable_dt);
        // Poll session while busy
        let snap = self.session.snapshot();
        if matches!(
            snap.phase,
            SessionPhase::Starting | SessionPhase::Stopping
        ) {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }

        let cfg = self.session.config_clone();

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG).inner_margin(0.0))
            .show(ctx, |ui| {
                ui.set_min_size(ui.available_size());

                // Header
                egui::Frame::NONE
                    .fill(BG)
                    .stroke(Stroke::new(1.0, LINE))
                    .inner_margin(egui::Margin {
                        left: 22,
                        right: 22,
                        top: 20,
                        bottom: 12,
                    })
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("SUIJI  ·  POTPLAYER")
                                .size(11.0)
                                .color(FAINT)
                                .extra_letter_spacing(2.0),
                        );
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("今日片单")
                                    .size(26.0)
                                    .color(INK)
                                    .strong(),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                status_pill(ui, snap.phase, &snap.message);
                            });
                        });
                        ui.add_space(6.0);
                        let root_label = if snap.library_root.is_empty() {
                            "未设置片库".to_string()
                        } else {
                            truncate_path(&snap.library_root, 42)
                        };
                        ui.label(
                            RichText::new(format!(
                                "{root_label}  ·  已索引 {} 部",
                                snap.library_count
                            ))
                            .size(12.5)
                            .color(MUTED),
                        );
                    });

                ui.add_space(4.0);

                // Controls
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(22, 10))
                    .show(ui, |ui| {
                        // count
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("本轮数量").size(13.0).color(MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let count = self.session.ui_count();
                                if small_step_btn(ui, "+").clicked() {
                                    self.session.set_ui_count(count + 1);
                                }
                                ui.label(
                                    RichText::new(format!("{count}"))
                                        .size(20.0)
                                        .color(INK)
                                        .strong(),
                                );
                                if small_step_btn(ui, "−").clicked() {
                                    self.session
                                        .set_ui_count(count.saturating_sub(1).max(1));
                                }
                            });
                        });

                        ui.add_space(8.0);

                        // volume
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("统一音量").size(13.0).color(MUTED));
                            let mut vol = cfg.volume_percent as f32;
                            let slider = egui::Slider::new(&mut vol, 0.0..=100.0)
                                .show_value(false)
                                .trailing_fill(true);
                            if ui.add_sized(Vec2::new(160.0, 18.0), slider).changed() {
                                self.session.set_volume(vol as u8);
                            }
                            ui.label(
                                RichText::new(format!("{}%", cfg.volume_percent))
                                    .size(12.5)
                                    .color(MUTED),
                            );
                        });

                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("避开最近播放").size(13.0).color(MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let mut avoid = cfg.avoid_recent;
                                if toggle(ui, &mut avoid) {
                                    self.session.set_avoid_recent(avoid);
                                }
                            });
                        });
                    });

                // Preview grid
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(22, 6))
                    .show(ui, |ui| {
                        let n = self.session.ui_count();
                        let (rows, cols) = crate::tiler::rows_cols(n);
                        ui.label(
                            RichText::new(format!(
                                "本轮预览  ·  {rows}×{cols} 网格  ·  避开任务栏"
                            ))
                            .size(11.0)
                            .color(FAINT)
                            .extra_letter_spacing(0.5),
                        );
                        ui.add_space(6.0);

                        let files = &snap.current_files;
                        egui::Frame::NONE
                            .fill(BG_SOFT)
                            .stroke(Stroke::new(1.0, LINE))
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                let gap = 6.0;
                                let total_w = ui.available_width();
                                let cell_w = ((total_w - gap * (cols as f32 - 1.0))
                                    / cols as f32)
                                    .max(40.0);
                                let cell_h = cell_w * 10.0 / 16.0;

                                for r in 0..rows {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = gap;
                                        for c in 0..cols {
                                            let idx = r * cols + c;
                                            if idx >= n {
                                                ui.allocate_exact_size(
                                                    Vec2::new(cell_w, cell_h),
                                                    Sense::hover(),
                                                );
                                                continue;
                                            }
                                            let label = files
                                                .get(idx)
                                                .and_then(|p| {
                                                    p.file_name()
                                                        .map(|n| n.to_string_lossy().to_string())
                                                })
                                                .unwrap_or_default();
                                            preview_cell(ui, cell_w, cell_h, &label);
                                        }
                                    });
                                    ui.add_space(gap);
                                }
                            });

                        // fake taskbar hint
                        ui.add_space(2.0);
                        let (rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 8.0),
                            Sense::hover(),
                        );
                        ui.painter()
                            .rect_filled(rect, 0.0, Color32::from_rgb(0xD6, 0xD3, 0xD1));
                        ui.label(
                            RichText::new("任务栏区域（不覆盖）")
                                .size(10.0)
                                .color(FAINT),
                        );
                    });

                if !snap.last_errors.is_empty() {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(22, 4))
                        .show(ui, |ui| {
                            ui.colored_label(
                                Color32::from_rgb(0xB4, 0x53, 0x09),
                                RichText::new(format!(
                                    "注意：{} 项启动异常",
                                    snap.last_errors.len()
                                ))
                                .size(12.0),
                            );
                        });
                }

                ui.add_space(4.0);

                // Actions
                egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: 22,
                        right: 22,
                        top: 8,
                        bottom: 18,
                    })
                    .show(ui, |ui| {
                        let busy = matches!(
                            snap.phase,
                            SessionPhase::Starting | SessionPhase::Stopping
                        );
                        let playing = snap.phase == SessionPhase::Playing;

                        let primary = if playing { "关闭本轮" } else { "开启本轮" };
                        let primary_enabled = !busy
                            && (playing
                                || (!snap.library_root.is_empty() && snap.library_count > 0));

                        if primary_btn(ui, primary, primary_enabled).clicked() {
                            if playing {
                                self.session.stop();
                            } else {
                                self.session.start();
                            }
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let w = (ui.available_width() - 8.0) / 2.0;
                            let reroll_ok = !busy
                                && (playing || snap.phase == SessionPhase::Idle)
                                && snap.library_count > 0;
                            if secondary_btn(ui, w, "再来一轮", reroll_ok).clicked() {
                                self.session.reroll();
                            }
                            ui.add_space(8.0);
                            let stop_ok = !busy && playing;
                            if secondary_btn(ui, w, "关闭本轮", stop_ok).clicked() {
                                self.session.stop();
                            }
                        });

                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            ui.horizontal(|ui| {
                                if link_btn(ui, "片库设置").clicked() {
                                    self.show_settings = true;
                                    self.pot_path_edit =
                                        self.session.config_clone().potplayer_path;
                                }
                                ui.label(RichText::new("·").color(FAINT));
                                if link_btn(ui, "重新扫描").clicked() {
                                    self.session.rescan();
                                }
                                ui.label(RichText::new("·").color(FAINT));
                                ui.label(
                                    RichText::new("v0.1")
                                        .size(11.0)
                                        .color(FAINT),
                                );
                            });
                        });
                    });
            });

        if self.show_settings {
            self.settings_modal(ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.session.shutdown_if_needed();
    }
}

impl SuijiApp {
    fn settings_modal(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        egui::Window::new("片库设置")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([340.0, 280.0])
            .open(&mut open)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(BG)
                    .stroke(Stroke::new(1.0, LINE_STRONG))
                    .corner_radius(2.0)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new("电影主目录").size(13.0).color(MUTED));
                ui.add_space(4.0);
                let root = self.session.snapshot().library_root;
                ui.label(
                    RichText::new(if root.is_empty() {
                        "（尚未选择）".into()
                    } else {
                        root
                    })
                    .size(12.5)
                    .color(INK),
                );
                ui.add_space(8.0);
                if ui
                    .add(sized_outline_button("选择文件夹…", ui.available_width()))
                    .clicked()
                {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.session
                            .update_library_path(folder.to_string_lossy().to_string());
                    }
                }

                ui.add_space(16.0);
                ui.label(RichText::new("PotPlayer 路径（可留空自动探测）").size(13.0).color(MUTED));
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.pot_path_edit)
                        .desired_width(f32::INFINITY)
                        .text_color(INK),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(sized_outline_button("浏览…", 90.0))
                        .clicked()
                    {
                        if let Some(f) = rfd::FileDialog::new()
                            .add_filter("Executable", &["exe"])
                            .pick_file()
                        {
                            self.pot_path_edit = f.to_string_lossy().to_string();
                        }
                    }
                    if ui
                        .add(sized_outline_button("保存路径", 90.0))
                        .clicked()
                    {
                        self.session
                            .set_potplayer_path(self.pot_path_edit.clone());
                    }
                });

                ui.add_space(16.0);
                let mut close_on_exit = self.session.config_clone().close_session_on_exit;
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("退出时关闭本轮播放")
                            .size(13.0)
                            .color(MUTED),
                    );
                    if toggle(ui, &mut close_on_exit) {
                        self.session.set_close_on_exit(close_on_exit);
                    }
                });

                ui.add_space(18.0);
                if primary_btn(ui, "完成", true).clicked() {
                    self.show_settings = false;
                }
            });
        self.show_settings = open;
    }
}

fn status_pill(ui: &mut egui::Ui, phase: SessionPhase, message: &str) {
    let (text, bg, fg) = match phase {
        SessionPhase::Idle => ("就绪", BG_SOFT, MUTED),
        SessionPhase::Starting => ("启动中", Color32::from_rgb(0xE7, 0xE0, 0xD6), INK),
        SessionPhase::Playing => ("播放中", INK, ON_INK),
        SessionPhase::Stopping => ("关闭中", Color32::from_rgb(0xE7, 0xE0, 0xD6), INK),
    };
    let label = if message.chars().count() < 16 && phase == SessionPhase::Idle {
        message
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
    ui.painter()
        .rect_stroke(rect, 20.0, Stroke::new(1.0, LINE_STRONG), egui::StrokeKind::Inside);
    ui.painter().rect_filled(rect, 20.0, bg);
    ui.painter().galley(
        egui::pos2(rect.left() + pad.x, rect.top() + pad.y),
        galley,
        fg,
    );
}

fn small_step_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let size = Vec2::new(28.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let stroke = if resp.hovered() {
        Stroke::new(1.0, MUTED)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    ui.painter()
        .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(16.0),
        INK,
    );
    resp
}

fn toggle(ui: &mut egui::Ui, on: &mut bool) -> bool {
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

fn primary_btn(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let width = ui.available_width();
    let height = 42.0;
    let (rect, mut resp) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if !enabled {
        resp = resp.on_disabled_hover_text("请先完成片库设置并确保有视频");
    }
    let bg = if !enabled {
        Color32::from_rgb(0xA8, 0xA2, 0x9E)
    } else if resp.hovered() {
        Color32::from_rgb(0x29, 0x25, 0x24)
    } else {
        INK
    };
    ui.painter().rect_filled(rect, 0.0, bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(14.0),
        ON_INK,
    );
    if enabled {
        resp
    } else {
        resp.surrender_focus();
        // swallow clicks
        ui.interact(rect, ui.id().with("disabled_primary"), Sense::hover())
    }
}

fn secondary_btn(ui: &mut egui::Ui, width: f32, text: &str, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::click());
    let stroke = if !enabled {
        Stroke::new(1.0, LINE)
    } else if resp.hovered() {
        Stroke::new(1.0, MUTED)
    } else {
        Stroke::new(1.0, FAINT)
    };
    let fg = if enabled { INK } else { FAINT };
    ui.painter()
        .rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(12.5),
        fg,
    );
    if enabled {
        resp
    } else {
        ui.interact(rect, ui.id().with(text).with("dis"), Sense::hover())
    }
}

fn link_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Label::new(RichText::new(text).size(11.5).color(MUTED))
            .sense(Sense::click()),
    )
}

fn sized_outline_button(text: &str, width: f32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(INK))
        .fill(BG)
        .stroke(Stroke::new(1.0, FAINT))
        .min_size(Vec2::new(width, 32.0))
}

fn preview_cell(ui: &mut egui::Ui, w: f32, h: f32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgb(0xE7, 0xE5, 0xE4));
    ui.painter()
        .rect_stroke(rect, 0.0, Stroke::new(1.0, LINE_STRONG), egui::StrokeKind::Inside);
    if !label.is_empty() {
        let short = truncate_path(label, 14);
        ui.painter().text(
            egui::pos2(rect.left() + 4.0, rect.bottom() - 14.0),
            egui::Align2::LEFT_TOP,
            short,
            egui::FontId::proportional(9.0),
            MUTED,
        );
    }
}

fn truncate_path(s: &str, max_chars: usize) -> String {
    let p = Path::new(s);
    let display = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| s.to_string());
    let count = display.chars().count();
    if count <= max_chars {
        display
    } else {
        let take = max_chars.saturating_sub(1);
        let t: String = display.chars().take(take).collect();
        format!("{t}…")
    }
}
