//! Settings modal UI.
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};

use crate::config::{ImagePlayStyle, MediaMode};
use super::theme::{BG, BG_SOFT, FAINT, INK, LINE, LINE_STRONG, MUTED};
use super::widgets::{
    bound_stepper, ease_out_cubic, mini_text_btn, mode_chip, primary_btn, sized_outline_button,
    small_step_btn, toggle, truncate_path,
};
use super::SuijiApp;

impl SuijiApp {
    pub(super) fn settings_modal(&mut self, ctx: &egui::Context) {
        let t = ease_out_cubic(self.settings_vis.clamp(0.0, 1.0));
        let a = (t * 255.0) as u8;
        let scrim_a = (t * 88.0) as u8;
        let screen = ctx.screen_rect();

        // Dim scrim — click does not close (avoid accidental dismiss while choosing folders)
        egui::Area::new(egui::Id::new("settings_scrim"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Middle)
            .interactable(true)
            .show(ctx, |ui| {
                ui.painter().rect_filled(
                    screen,
                    0.0,
                    Color32::from_rgba_unmultiplied(28, 25, 23, scrim_a),
                );
                let _ = ui.interact(
                    screen,
                    ui.id().with("settings_scrim_block"),
                    Sense::click(),
                );
            });

        let mut open = true; // keep window alive during exit fade
        let mut finish_clicked = false;
        // Slight rise on enter (spatial: from below → center)
        let y_off = (1.0 - t) * 14.0;

        // Fit inside the main window; never larger than ~90% so chrome stays visible.
        let max_w = (screen.width() * 0.92).clamp(360.0, 560.0);
        let max_h = (screen.height() * 0.90).clamp(360.0, 820.0);
        let def_w = max_w.min(480.0);
        let def_h = max_h.min(620.0);

        egui::Window::new("片库设置")
            .collapsible(false)
            .resizable(true)
            .constrain(true)
            .constrain_to(screen.shrink(8.0))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, y_off])
            .default_size([def_w, def_h])
            .min_size([340.0, 320.0])
            .max_size([max_w, max_h])
            .open(&mut open)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(Color32::from_rgba_unmultiplied(BG.r(), BG.g(), BG.b(), a))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(
                            LINE_STRONG.r(),
                            LINE_STRONG.g(),
                            LINE_STRONG.b(),
                            a,
                        ),
                    ))
                    .corner_radius(2.0)
                    .inner_margin(14.0)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, (t * 40.0) as u8),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_opacity(t.max(0.05));
                // Fill window so scroll region knows remaining height
                let panel_h = ui.available_height().max(200.0);
                let panel_w = ui.available_width().max(280.0);
                ui.set_min_size(Vec2::new(panel_w, panel_h));

                // Sticky footer height (primary button + gap)
                let footer_h = 58.0;
                let scroll_h = (panel_h - footer_h).max(120.0);

                egui::ScrollArea::vertical()
                    .id_salt("settings_body_scroll")
                    .max_height(scroll_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(panel_w - 4.0);
                        self.draw_settings_body(ui);
                    });

                ui.add_space(10.0);
                if primary_btn(ui, "完成", true).clicked() {
                    finish_clicked = true;
                }
            });

        if finish_clicked {
            // Persist path field even if user skipped「保存路径」
            let path = self.pot_path_edit.trim().to_string();
            self.session.set_potplayer_path(path);
            let cfg = self.session.config_clone();
            let save_ok = crate::config::save(&cfg).is_ok();
            self.show_settings = false; // begin close anim
            if save_ok {
                let roots = cfg.roots_for(cfg.media_mode).len();
                let (cmin, cmax) = cfg.count_bounds_for(cfg.media_mode);
                let kind = match cfg.media_mode {
                    MediaMode::Movie => "电影库",
                    MediaMode::Image => "图库",
                };
                self.show_toast(format!(
                    "设置已保存 · {kind} {roots} 个 · 本模式数量 {cmin}–{cmax}"
                ));
            } else {
                self.show_toast("设置未能写入文件，请检查目录权限");
            }
        } else if !open && self.show_settings {
            // Closed via window X — also commit PotPlayer path (other fields live-save)
            let path = self.pot_path_edit.trim().to_string();
            self.session.set_potplayer_path(path);
            self.show_settings = false;
            self.show_toast("已关闭设置（修改已即时生效）");
        }
    }

    /// Scrollable settings content (footer「完成」 stays outside).
    fn draw_settings_body(&mut self, ui: &mut egui::Ui) {
        let mode = self.session.media_mode();
        ui.label(
            RichText::new(format!(
                "{}库目录（可添加多个，将合并扫描）· 当前：{}",
                mode.label(),
                mode.label()
            ))
            .size(13.0)
            .color(MUTED),
        );
        ui.add_space(6.0);

        // Always read live roots so remove reflects immediately next paint
        let roots = self.session.snapshot().library_roots;
        if roots.is_empty() {
            egui::Frame::NONE
                .fill(BG_SOFT)
                .stroke(Stroke::new(1.0, LINE))
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("（尚未添加目录，请点下方「添加文件夹」）")
                            .size(13.0)
                            .color(MUTED),
                    );
                });
        } else {
            // Nested list: grow with items but cap so one section never eats the whole modal
            let list_h = ((roots.len() as f32) * 44.0 + 8.0).clamp(48.0, 200.0);
            egui::ScrollArea::vertical()
                .max_height(list_h)
                .id_salt("library_roots_list")
                .show(ui, |ui| {
                    let mut remove_idx: Option<usize> = None;
                    for (i, root) in roots.iter().enumerate() {
                        egui::Frame::NONE
                            .fill(BG_SOFT)
                            .stroke(Stroke::new(1.0, LINE))
                            .inner_margin(egui::Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.set_min_height(28.0);
                                    ui.label(
                                        RichText::new(format!("{}.", i + 1))
                                            .size(13.0)
                                            .color(FAINT),
                                    );
                                    let path_w = (ui.available_width() - 60.0).max(48.0);
                                    ui.add_sized(
                                        Vec2::new(path_w, 20.0),
                                        egui::Label::new(
                                            RichText::new(root.as_str()).size(13.0).color(INK),
                                        )
                                        .truncate(),
                                    );
                                    if mini_text_btn(ui, "移除").clicked() {
                                        remove_idx = Some(i);
                                    }
                                });
                            });
                        ui.add_space(4.0);
                    }
                    if let Some(i) = remove_idx {
                        let name = roots.get(i).cloned().unwrap_or_default();
                        if let Some(removed) = self.session.remove_library_path(i) {
                            let short = truncate_path(&removed, 28);
                            self.show_toast(format!("已移除片库：{short}"));
                        } else if !name.is_empty() {
                            self.show_toast(format!(
                                "移除失败：{}",
                                truncate_path(&name, 24)
                            ));
                        }
                    }
                });
        }

        ui.add_space(8.0);
        if ui
            .add(sized_outline_button("添加文件夹…", ui.available_width()))
            .clicked()
        {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                let p = folder.to_string_lossy().to_string();
                self.session.add_library_path(p.clone());
                self.show_toast(format!("已添加片库：{}", truncate_path(&p, 28)));
            }
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new("每个目录都会递归扫描子文件夹；重复路径会自动去重")
                .size(11.0)
                .color(FAINT),
        );

        ui.add_space(12.0);
        ui.label(
            RichText::new(match mode {
                MediaMode::Movie => "电影 · 本轮数量范围",
                MediaMode::Image => "图片 · 本轮数量范围",
            })
            .size(13.0)
            .color(MUTED),
        );
        ui.add_space(6.0);
        {
            let cfg_now = self.session.config_clone();
            let (cmin, cmax) = cfg_now.count_bounds_for(mode);
            ui.columns(2, |cols| {
                bound_stepper(
                    &mut cols[0],
                    "下限",
                    cmin,
                    || {
                        self.session
                            .set_count_bounds(cmin.saturating_sub(1), cmax);
                    },
                    || {
                        self.session.set_count_bounds(cmin + 1, cmax);
                    },
                );
                bound_stepper(
                    &mut cols[1],
                    "上限",
                    cmax,
                    || {
                        self.session
                            .set_count_bounds(cmin, cmax.saturating_sub(1));
                    },
                    || {
                        self.session.set_count_bounds(cmin, cmax + 1);
                    },
                );
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "与另一模式独立记忆；绝对范围 {}–{}",
                    crate::config::Config::ABS_COUNT_MIN,
                    crate::config::Config::ABS_COUNT_MAX
                ))
                .size(11.0)
                .color(FAINT),
            );
        }

        if mode == MediaMode::Image {
            ui.add_space(12.0);
            ui.label(
                RichText::new("图片开启方式")
                    .size(13.0)
                    .color(MUTED),
            );
            ui.add_space(4.0);
            let style = self.session.config_clone().image_play_style;
            ui.horizontal_wrapped(|ui| {
                mode_chip(
                    ui,
                    "幻灯片",
                    style == ImagePlayStyle::Slideshow,
                    || {
                        self.session
                            .set_image_play_style(ImagePlayStyle::Slideshow);
                        self.show_toast("开启后将全屏幻灯播放");
                    },
                );
                ui.add_space(6.0);
                mode_chip(ui, "平铺墙", style == ImagePlayStyle::Wall, || {
                    self.session.set_image_play_style(ImagePlayStyle::Wall);
                    self.show_toast("开启后将平铺展示本轮图片");
                });
            });

            if style == ImagePlayStyle::Slideshow {
                ui.add_space(10.0);
                ui.label(
                    RichText::new("幻灯片间隔（秒）")
                        .size(13.0)
                        .color(MUTED),
                );
                ui.add_space(4.0);
                let iv = self.session.config_clone().slideshow_interval_secs;
                ui.horizontal(|ui| {
                    if small_step_btn(ui, "−").clicked() {
                        self.session
                            .set_slideshow_interval(iv.saturating_sub(1).max(1));
                    }
                    ui.label(
                        RichText::new(format!("{iv}"))
                            .size(16.0)
                            .color(INK)
                            .strong(),
                    );
                    if small_step_btn(ui, "+").clicked() {
                        self.session
                            .set_slideshow_interval(iv.saturating_add(1).min(60));
                    }
                    ui.label(RichText::new("秒 / 张").size(12.5).color(FAINT));
                });
            }
        }

        ui.add_space(14.0);
        ui.label(
            RichText::new("平铺工作区（电影多开落在哪块屏）")
                .size(13.0)
                .color(MUTED),
        );
        ui.add_space(4.0);
        {
            let cur = self.session.config_clone().tile_monitor_index;
            let monitors = crate::tiler::list_monitors();
            let sel_label = if cur < 0 {
                "系统主工作区（默认）".to_string()
            } else if let Some(m) = monitors.get(cur as usize) {
                m.label()
            } else {
                format!("显示器 #{}（当前不可用，将回退）", cur + 1)
            };
            egui::ComboBox::from_id_salt("tile_monitor")
                .selected_text(sel_label)
                .width((ui.available_width() - 8.0).max(160.0))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(cur < 0, "系统主工作区（默认）")
                        .clicked()
                    {
                        self.session.set_tile_monitor_index(-1);
                    }
                    for m in &monitors {
                        let lab = m.label();
                        if ui
                            .selectable_label(cur == m.index as i32, &lab)
                            .clicked()
                        {
                            self.session.set_tile_monitor_index(m.index as i32);
                        }
                    }
                });
        }
        ui.label(
            RichText::new("仅影响 PotPlayer 网格；控制面板窗口仍可拖到任意屏")
                .size(11.0)
                .color(FAINT),
        );

        ui.add_space(14.0);
        ui.label(
            RichText::new("PotPlayer（电影完整体验）")
                .size(13.0)
                .color(MUTED),
        );
        ui.add_space(4.0);
        {
            // Prefer the field being edited; empty = auto-detect like runtime.
            let probe = if self.pot_path_edit.trim().is_empty() {
                ""
            } else {
                self.pot_path_edit.trim()
            };
            let pot_ok = crate::potplayer::resolve_potplayer_path(probe).is_some();
            ui.label(
                RichText::new(if pot_ok {
                    "已检测到 · 可多开平铺"
                } else {
                    "未检测到 · 开启将用系统默认播放器（无平铺）"
                })
                .size(12.0)
                .color(if pot_ok {
                    MUTED
                } else {
                    Color32::from_rgb(0xB4, 0x53, 0x09)
                }),
            );
        }
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.pot_path_edit)
                .desired_width(f32::INFINITY)
                .text_color(INK)
                .hint_text("可留空自动探测，或浏览指定 exe"),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.add(sized_outline_button("浏览…", 90.0)).clicked() {
                if let Some(f) = rfd::FileDialog::new()
                    .add_filter("Executable", &["exe"])
                    .pick_file()
                {
                    self.pot_path_edit = f.to_string_lossy().to_string();
                }
            }
            if ui.add(sized_outline_button("保存路径", 90.0)).clicked() {
                self.session
                    .set_potplayer_path(self.pot_path_edit.clone());
                self.show_toast(if crate::potplayer::resolve_potplayer_path(
                    self.pot_path_edit.trim(),
                )
                .is_some()
                {
                    "PotPlayer 路径已保存"
                } else if self.pot_path_edit.trim().is_empty() {
                    "已清空 · 将自动探测"
                } else {
                    "路径无效，请重新选择"
                });
            }
        });

        ui.add_space(14.0);
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

        ui.add_space(8.0);
        let mut to_tray = self.session.config_clone().minimize_to_tray;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("点关闭(X)时进托盘（默认关=退出程序）")
                    .size(12.5)
                    .color(MUTED),
            );
            if toggle(ui, &mut to_tray) {
                self.session.set_minimize_to_tray(to_tray);
            }
        });
        ui.label(
            RichText::new("未开启时：X 退出程序；右上角托盘图标仍可手动收起")
                .size(11.0)
                .color(FAINT),
        );

        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "缩略图优先用片源旁已有图片（同名.jpg / poster / cover / 封面 等），否则用 ffmpeg 抽帧",
            )
            .size(11.0)
            .color(FAINT),
        );

        ui.add_space(12.0);
        let bl_count = self.session.blacklist_count();
        ui.label(
            RichText::new(format!(
                "黑名单（当前{}模式 · {} 条 · 不再被随机抽到）",
                mode.label(),
                bl_count
            ))
            .size(13.0)
            .color(MUTED),
        );
        ui.add_space(4.0);
        if bl_count == 0 {
            ui.label(
                RichText::new("预览片单上点「屏蔽」可加入")
                    .size(11.0)
                    .color(FAINT),
            );
        } else {
            let bl_h = ((bl_count as f32) * 28.0 + 8.0).clamp(48.0, 140.0);
            egui::ScrollArea::vertical()
                .max_height(bl_h)
                .id_salt("blacklist_list")
                .show(ui, |ui| {
                    let paths = self.session.blacklist_paths();
                    let mut unban: Option<std::path::PathBuf> = None;
                    for p in &paths {
                        let name = p
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| p.display().to_string());
                        ui.horizontal(|ui| {
                            let name_w = (ui.available_width() - 56.0).max(48.0);
                            ui.add_sized(
                                Vec2::new(name_w, 18.0),
                                egui::Label::new(
                                    RichText::new(truncate_path(&name, 36))
                                        .size(12.5)
                                        .color(INK),
                                )
                                .truncate(),
                            );
                            if mini_text_btn(ui, "移出").clicked() {
                                unban = Some(p.clone());
                            }
                        });
                    }
                    if let Some(p) = unban {
                        self.session.unblacklist_path(&p);
                        self.show_toast("已从黑名单移出");
                    }
                });
        }

        // Bottom padding so last rows aren't tight against the scroll edge
        ui.add_space(8.0);
    }
}
