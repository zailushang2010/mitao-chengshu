mod theme;

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, TextureHandle, Vec2};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::session::{SessionHandle, SessionPhase};
use crate::thumb::ThumbCache;
use crate::tray::{TrayCommand, TrayService};
use theme::{BG, BG_SOFT, FAINT, INK, LINE, LINE_STRONG, MUTED, ON_INK};

pub struct SuijiApp {
    session: SessionHandle,
    show_settings: bool,
    pot_path_edit: String,
    thumbs: ThumbCache,
    textures: HashMap<String, TextureHandle>,
    tray: Option<TrayService>,
    /// When true, next close request quits instead of tray-hide
    force_quit: bool,
    /// First frames: force visible + focus so user always sees the window
    boot_frames: u8,
    /// Shrink window height to content for a few frames (kill bottom dead space)
    fit_height_frames: u8,
    last_fit_count: usize,
    last_phase: SessionPhase,
    /// User hid to tray; don't auto-raise until they ask
    user_hid_to_tray: bool,
}

impl SuijiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, session: SessionHandle) -> Self {
        theme::apply_magazine_style(&cc.egui_ctx);
        // Always show on create
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Minimized(false));

        let pot_path_edit = session.config_clone().potplayer_path;
        let tray = match TrayService::try_new(cc.egui_ctx.clone()) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("tray unavailable: {e}");
                None
            }
        };
        // Open settings on first run if library empty after scan
        let show_settings = {
            let s = session.snapshot();
            s.library_roots.is_empty() || s.library_count == 0
        };
        let last_fit_count = session.ui_count();
        Self {
            session,
            show_settings,
            pot_path_edit,
            thumbs: ThumbCache::new(),
            textures: HashMap::new(),
            tray,
            force_quit: false,
            boot_frames: 10,
            fit_height_frames: 12,
            last_fit_count,
            last_phase: SessionPhase::Idle,
            user_hid_to_tray: false,
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.user_hid_to_tray = false;
        // Win32 first (cuts through PotPlayer wall), then sync eframe
        crate::tray::force_show_main_window();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        self.user_hid_to_tray = true;
        crate::tray::set_main_window_topmost(false);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = self.tray.as_ref() else {
            return;
        };

        let show_req = tray
            .show_requested
            .swap(false, std::sync::atomic::Ordering::SeqCst);
        let mut cmds = Vec::new();
        while let Ok(cmd) = tray.rx.try_recv() {
            cmds.push(cmd);
        }
        // drop tray borrow before &mut self methods
        let _ = tray;

        if show_req {
            self.show_window(ctx);
        }
        for cmd in cmds {
            match cmd {
                TrayCommand::Show => self.show_window(ctx),
                TrayCommand::StartOrStop => {
                    let phase = self.session.snapshot().phase;
                    if phase == SessionPhase::Playing {
                        self.session.stop();
                    } else if phase == SessionPhase::Idle {
                        self.session.start();
                    }
                    self.show_window(ctx);
                }
                TrayCommand::Reroll => {
                    self.session.reroll();
                    self.show_window(ctx);
                }
                TrayCommand::StopSession => {
                    self.session.stop();
                    self.show_window(ctx);
                }
                TrayCommand::Exit => {
                    self.force_quit = true;
                    self.session.shutdown_if_needed();
                    crate::tray::force_show_main_window();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn ensure_thumbs(&mut self, files: &[PathBuf], ctx: &egui::Context) {
        for f in files {
            self.thumbs.request(f);
            let key = f.to_string_lossy().to_string();
            if self.textures.contains_key(&key) {
                continue;
            }
            if let Some(path) = self.thumbs.path_if_ready(f) {
                if let Some(tex) = load_texture(ctx, &key, &path) {
                    self.textures.insert(key, tex);
                }
            }
        }
        // Drop textures for files no longer in session
        let live: std::collections::HashSet<String> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        self.textures.retain(|k, _| live.contains(k));
    }
}

impl eframe::App for SuijiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.boot_frames > 0 {
            self.boot_frames -= 1;
            self.show_window(ctx);
            ctx.request_repaint();
        }

        self.poll_tray(ctx);

        // While we may be hidden, keep a light repaint heartbeat so tray
        // channel is drained even if winit is quiet.
        if self.tray.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(400));
        }

        // Close → tray (unless force quit or setting off)
        if ctx.input(|i| i.viewport().close_requested()) {
            let to_tray = self.session.config_clone().minimize_to_tray
                && self.tray.is_some()
                && !self.force_quit;
            if to_tray {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_to_tray(ctx);
            }
        }

        let snap = self.session.snapshot();
        if matches!(
            snap.phase,
            SessionPhase::Starting | SessionPhase::Stopping | SessionPhase::Playing
        ) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        // After 开启本轮 finishes, PotPlayers steal focus — raise control panel
        if self.last_phase == SessionPhase::Starting && snap.phase == SessionPhase::Playing {
            if !self.user_hid_to_tray {
                self.show_window(ctx);
                crate::tray::set_main_window_topmost(true);
            }
        }
        // While playing, keep panel on top (unless user chose tray)
        if snap.phase == SessionPhase::Playing && !self.user_hid_to_tray {
            crate::tray::set_main_window_topmost(true);
        }
        if matches!(snap.phase, SessionPhase::Idle | SessionPhase::Stopping)
            && self.last_phase == SessionPhase::Playing
        {
            crate::tray::set_main_window_topmost(false);
        }
        self.last_phase = snap.phase;

        self.ensure_thumbs(&snap.current_files, ctx);
        let cfg = self.session.config_clone();

        // Count changes → preview rows change → re-fit window height
        let count_now = self.session.ui_count();
        if count_now != self.last_fit_count {
            self.last_fit_count = count_now;
            self.fit_height_frames = 6;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG).inner_margin(0.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);

                // Pack everything in one vertical column — no ScrollArea (it was leaving a tall empty band).
                let stack = ui.vertical(|ui| {
                    // ── Header ──
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 18,
                            right: 14,
                            top: 10,
                            bottom: 6,
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new("随机片库 · PotPlayer")
                                            .size(10.5)
                                            .color(FAINT)
                                            .extra_letter_spacing(1.5),
                                    );
                                    ui.add_space(1.0);
                                    ui.label(
                                        RichText::new("今日片单")
                                            .size(22.0)
                                            .color(INK)
                                            .strong(),
                                    );
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if self.tray.is_some() {
                                        if icon_btn(ui, IconKind::Tray, "最小化到托盘").clicked()
                                        {
                                            self.hide_to_tray(ctx);
                                        }
                                        ui.add_space(4.0);
                                    }
                                    if icon_btn(ui, IconKind::Rescan, "重新扫描片库").clicked() {
                                        self.session.rescan();
                                    }
                                    ui.add_space(4.0);
                                    if icon_btn(ui, IconKind::Settings, "片库与设置").clicked() {
                                        self.show_settings = true;
                                        self.pot_path_edit =
                                            self.session.config_clone().potplayer_path;
                                    }
                                });
                            });

                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                status_pill(ui, snap.phase, &snap.message);
                                ui.add_space(8.0);
                                let root_label = if snap.library_roots.is_empty() {
                                    "未设置片库".to_string()
                                } else if snap.library_roots.len() == 1 {
                                    truncate_path(&snap.library_roots[0], 28)
                                } else {
                                    format!(
                                        "{} 等 {} 个目录",
                                        truncate_path(&snap.library_roots[0], 16),
                                        snap.library_roots.len()
                                    )
                                };
                                ui.label(
                                    RichText::new(format!(
                                        "{root_label} · {} 部",
                                        snap.library_count
                                    ))
                                    .size(12.0)
                                    .color(MUTED),
                                );
                            });
                        });

                    ui.add(egui::Separator::default().spacing(4.0));

                    // ── Controls ──
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(18, 6))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "本轮数量（{}–{}）",
                                        cfg.count_min, cfg.count_max
                                    ))
                                    .size(13.0)
                                    .color(MUTED),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let count = self.session.ui_count();
                                    if small_step_btn(ui, "+").clicked() {
                                        self.session.set_ui_count(count + 1);
                                    }
                                    ui.label(
                                        RichText::new(format!("{count}"))
                                            .size(18.0)
                                            .color(INK)
                                            .strong(),
                                    );
                                    if small_step_btn(ui, "−").clicked() {
                                        self.session.set_ui_count(count.saturating_sub(1));
                                    }
                                });
                            });

                            ui.horizontal(|ui| {
                                ui.label(RichText::new("统一音量").size(13.0).color(MUTED));
                                let mut vol = cfg.volume_percent as f32;
                                let slider = egui::Slider::new(&mut vol, 0.0..=100.0)
                                    .show_value(false)
                                    .trailing_fill(true);
                                if ui.add_sized(Vec2::new(150.0, 16.0), slider).changed() {
                                    self.session.set_volume(vol as u8);
                                }
                                ui.label(
                                    RichText::new(format!("{}%", cfg.volume_percent))
                                        .size(12.0)
                                        .color(MUTED),
                                );
                            });

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("避开最近播放").size(13.0).color(MUTED),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let mut avoid = cfg.avoid_recent;
                                    if toggle(ui, &mut avoid) {
                                        self.session.set_avoid_recent(avoid);
                                    }
                                });
                            });
                        });

                    // ── Preview ──
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 18,
                            right: 18,
                            top: 2,
                            bottom: 4,
                        })
                        .show(ui, |ui| {
                            let n = self.session.ui_count();
                            let (rows, cols) = crate::tiler::rows_cols(n);
                            ui.label(
                                RichText::new(format!(
                                    "本轮预览 · {rows}×{cols} · 避开任务栏"
                                ))
                                .size(11.0)
                                .color(FAINT)
                                .extra_letter_spacing(0.3),
                            );
                            ui.add_space(3.0);

                            let files = &snap.current_files;
                            egui::Frame::NONE
                                .fill(BG_SOFT)
                                .stroke(Stroke::new(1.0, LINE))
                                .inner_margin(5.0)
                                .show(ui, |ui| {
                                    let gap = 3.0;
                                    let total_w = ui.available_width();
                                    let cell_w = ((total_w - gap * (cols as f32 - 1.0))
                                        / cols as f32)
                                        .max(32.0);
                                    let cell_h = (cell_w * 10.0 / 16.0).min(42.0);

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
                                                let path_opt = files.get(idx);
                                                let label = path_opt
                                                    .and_then(|p| {
                                                        p.file_name().map(|n| {
                                                            n.to_string_lossy().to_string()
                                                        })
                                                    })
                                                    .unwrap_or_default();
                                                let tex = path_opt.and_then(|p| {
                                                    self.textures
                                                        .get(&p.to_string_lossy().to_string())
                                                });
                                                preview_cell(ui, cell_w, cell_h, &label, tex);
                                            }
                                        });
                                        if r + 1 < rows {
                                            ui.add_space(gap);
                                        }
                                    }
                                });
                        });

                    if !snap.last_errors.is_empty() {
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
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

                    // ── Per-item ops while playing ──
                    if snap.phase == SessionPhase::Playing && !snap.items.is_empty() {
                        ui.add_space(4.0);
                        egui::Frame::NONE
                            .inner_margin(egui::Margin {
                                left: 18,
                                right: 18,
                                top: 0,
                                bottom: 2,
                            })
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("本轮影片 · 可单独操作")
                                        .size(11.0)
                                        .color(FAINT),
                                );
                                ui.add_space(3.0);
                                egui::ScrollArea::vertical()
                                    .max_height(140.0)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        let mut close_idx: Option<usize> = None;
                                        let mut focus_idx: Option<usize> = None;
                                        let mut solo_idx: Option<usize> = None;
                                        for it in &snap.items {
                                            egui::Frame::NONE
                                                .fill(BG_SOFT)
                                                .stroke(Stroke::new(1.0, LINE))
                                                .inner_margin(egui::Margin::symmetric(8, 5))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label(
                                                            RichText::new(format!(
                                                                "{}. {}",
                                                                it.index + 1,
                                                                truncate_path(&it.name, 22)
                                                            ))
                                                            .size(12.0)
                                                            .color(INK),
                                                        );
                                                        ui.with_layout(
                                                            Layout::right_to_left(Align::Center),
                                                            |ui| {
                                                                if mini_text_btn(ui, "关闭").clicked()
                                                                {
                                                                    close_idx = Some(it.index);
                                                                }
                                                                ui.add_space(4.0);
                                                                if mini_text_btn(ui, "独播").clicked()
                                                                {
                                                                    solo_idx = Some(it.index);
                                                                }
                                                                ui.add_space(4.0);
                                                                if mini_text_btn(ui, "置前").clicked()
                                                                {
                                                                    focus_idx = Some(it.index);
                                                                }
                                                            },
                                                        );
                                                    });
                                                });
                                            ui.add_space(3.0);
                                        }
                                        if let Some(i) = focus_idx {
                                            self.session.focus_item(i);
                                            self.show_window(ctx);
                                        }
                                        if let Some(i) = solo_idx {
                                            self.session.solo_item(i);
                                            self.show_window(ctx);
                                        }
                                        if let Some(i) = close_idx {
                                            self.session.close_item(i);
                                            self.fit_height_frames = 4;
                                        }
                                    });
                            });
                    }

                    ui.add_space(6.0);

                    // ── Actions (tight under preview) ──
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 18,
                            right: 18,
                            top: 0,
                            bottom: 10,
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
                                    || (!snap.library_roots.is_empty()
                                        && snap.library_count > 0));

                            if primary_btn(ui, primary, primary_enabled).clicked() {
                                if playing {
                                    self.session.stop();
                                } else {
                                    self.session.start();
                                }
                            }

                            ui.add_space(6.0);
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
                        });
                });

                // Resize window to the actual stacked content height (kills bottom dead space)
                if self.fit_height_frames > 0 && !self.show_settings {
                    let used_h = stack.response.rect.height().ceil() + 4.0;
                    let w = ctx.input(|i| {
                        i.viewport()
                            .inner_rect
                            .map(|r| r.width())
                            .unwrap_or(440.0)
                    });
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                        w.clamp(400.0, 520.0),
                        used_h.clamp(320.0, 900.0),
                    )));
                    self.fit_height_frames = self.fit_height_frames.saturating_sub(1);
                    ctx.request_repaint();
                }
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
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_size([360.0, 420.0])
            .open(&mut open)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(BG)
                    .stroke(Stroke::new(1.0, LINE_STRONG))
                    .corner_radius(2.0)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("片库目录（可添加多个，将合并扫描）")
                        .size(13.0)
                        .color(MUTED),
                );
                ui.add_space(6.0);

                let roots = self.session.snapshot().library_roots;
                if roots.is_empty() {
                    ui.label(
                        RichText::new("（尚未添加目录）")
                            .size(12.5)
                            .color(FAINT),
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(140.0)
                        .show(ui, |ui| {
                            let mut remove_idx: Option<usize> = None;
                            for (i, root) in roots.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("{}.  {}", i + 1, root))
                                            .size(12.0)
                                            .color(INK),
                                    );
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            if link_btn(ui, "移除").clicked() {
                                                remove_idx = Some(i);
                                            }
                                        },
                                    );
                                });
                                ui.add_space(4.0);
                            }
                            if let Some(i) = remove_idx {
                                self.session.remove_library_path(i);
                            }
                        });
                }

                ui.add_space(8.0);
                if ui
                    .add(sized_outline_button("添加文件夹…", ui.available_width()))
                    .clicked()
                {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.session
                            .add_library_path(folder.to_string_lossy().to_string());
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
                    RichText::new("本轮数量范围（可改，不再固定 5–10）")
                        .size(13.0)
                        .color(MUTED),
                );
                ui.add_space(4.0);
                {
                    let cfg_now = self.session.config_clone();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("下限").size(12.5).color(MUTED));
                        if small_step_btn(ui, "−").clicked() {
                            self.session.set_count_bounds(
                                cfg_now.count_min.saturating_sub(1),
                                cfg_now.count_max,
                            );
                        }
                        ui.label(
                            RichText::new(format!("{}", cfg_now.count_min))
                                .size(15.0)
                                .color(INK)
                                .strong(),
                        );
                        if small_step_btn(ui, "+").clicked() {
                            self.session
                                .set_count_bounds(cfg_now.count_min + 1, cfg_now.count_max);
                        }

                        ui.add_space(16.0);
                        ui.label(RichText::new("上限").size(12.5).color(MUTED));
                        if small_step_btn(ui, "−").clicked() {
                            self.session.set_count_bounds(
                                cfg_now.count_min,
                                cfg_now.count_max.saturating_sub(1),
                            );
                        }
                        ui.label(
                            RichText::new(format!("{}", cfg_now.count_max))
                                .size(15.0)
                                .color(INK)
                                .strong(),
                        );
                        if small_step_btn(ui, "+").clicked() {
                            self.session
                                .set_count_bounds(cfg_now.count_min, cfg_now.count_max + 1);
                        }
                    });
                    ui.label(
                        RichText::new(format!(
                            "绝对范围 {}–{}；改后主界面「本轮数量」按此限制",
                            crate::config::Config::ABS_COUNT_MIN,
                            crate::config::Config::ABS_COUNT_MAX
                        ))
                        .size(11.0)
                        .color(FAINT),
                    );
                }

                ui.add_space(14.0);
                ui.label(
                    RichText::new("PotPlayer 路径（可留空自动探测）")
                        .size(13.0)
                        .color(MUTED),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.pot_path_edit)
                        .desired_width(f32::INFINITY)
                        .text_color(INK),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
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
                ui.horizontal(|ui| {
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
                    RichText::new("缩略图：同目录海报图，或系统 PATH 中的 ffmpeg")
                        .size(11.0)
                        .color(FAINT),
                );

                ui.add_space(16.0);
                if primary_btn(ui, "完成", true).clicked() {
                    self.show_settings = false;
                }
            });
        self.show_settings = open;
    }
}

fn load_texture(ctx: &egui::Context, id: &str, path: &Path) -> Option<TextureHandle> {
    let img = image::open(path).ok()?.into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = img.into_raw();
    let color = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture(id, color, egui::TextureOptions::LINEAR))
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
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
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
        ui.interact(rect, ui.id().with("disabled_primary"), Sense::hover())
    }
}

fn mini_text_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let pad = Vec2::new(8.0, 3.0);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(11.0),
        INK,
    );
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let stroke = if resp.hovered() {
        Stroke::new(1.0, INK)
    } else {
        Stroke::new(1.0, FAINT)
    };
    ui.painter()
        .rect_stroke(rect, 1.0, stroke, egui::StrokeKind::Inside);
    if resp.hovered() {
        ui.painter().rect_filled(rect, 1.0, BG_SOFT);
    }
    ui.painter().galley(
        egui::pos2(rect.left() + pad.x, rect.top() + pad.y),
        galley,
        INK,
    );
    resp
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
        egui::Label::new(RichText::new(text).size(11.5).color(MUTED)).sense(Sense::click()),
    )
}

#[derive(Clone, Copy)]
enum IconKind {
    Settings,
    Rescan,
    Tray,
}

/// Magazine-style square icon button with hover + tooltip.
fn icon_btn(ui: &mut egui::Ui, kind: IconKind, tip: &str) -> egui::Response {
    let size = Vec2::new(34.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let stroke = if hovered {
        Stroke::new(1.2, INK)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    let fill = if hovered { BG_SOFT } else { BG };
    ui.painter().rect_filled(rect, 2.0, fill);
    ui.painter()
        .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);

    let c = rect.center();
    let ink = if hovered { INK } else { MUTED };
    let s = Stroke::new(1.4, ink);

    match kind {
        IconKind::Settings => {
            // Gear: circle + teeth ticks
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
            // Circular arrows (refresh)
            let r = 8.0;
            // arc approximation with polyline
            let mut pts = Vec::new();
            for i in 0..=14 {
                let t = i as f32 / 14.0;
                let a = -0.2 + t * (std::f32::consts::TAU * 0.72);
                pts.push(c + Vec2::new(a.cos() * r, a.sin() * r));
            }
            for w in pts.windows(2) {
                ui.painter().line_segment([w[0], w[1]], s);
            }
            // arrow head at end
            if let Some(&p) = pts.last() {
                let a = -0.2 + 0.72 * std::f32::consts::TAU;
                let dir = Vec2::new(a.cos(), a.sin());
                let n = Vec2::new(-dir.y, dir.x);
                ui.painter().line_segment([p, p - dir * 4.0 + n * 3.0], s);
                ui.painter().line_segment([p, p - dir * 4.0 - n * 3.0], s);
            }
        }
        IconKind::Tray => {
            // Window minimize into tray: rectangle + down chevron
            let box_r = egui::Rect::from_center_size(c + Vec2::new(0.0, -2.0), Vec2::new(14.0, 10.0));
            ui.painter()
                .rect_stroke(box_r, 1.0, s, egui::StrokeKind::Outside);
            // bottom bar
            ui.painter().line_segment(
                [
                    egui::pos2(box_r.left() + 2.0, box_r.bottom() - 2.5),
                    egui::pos2(box_r.right() - 2.0, box_r.bottom() - 2.5),
                ],
                s,
            );
            // down arrow
            let tip = egui::pos2(c.x, c.y + 11.0);
            ui.painter().line_segment(
                [egui::pos2(c.x - 4.0, c.y + 6.0), tip],
                s,
            );
            ui.painter().line_segment(
                [egui::pos2(c.x + 4.0, c.y + 6.0), tip],
                s,
            );
        }
    }

    resp.on_hover_text(tip)
}

fn sized_outline_button(text: &str, width: f32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(INK))
        .fill(BG)
        .stroke(Stroke::new(1.0, FAINT))
        .min_size(Vec2::new(width, 32.0))
}

fn preview_cell(
    ui: &mut egui::Ui,
    w: f32,
    h: f32,
    label: &str,
    texture: Option<&TextureHandle>,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
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
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, LINE_STRONG),
        egui::StrokeKind::Inside,
    );
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
            egui::FontId::proportional(9.0),
            ON_INK,
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
