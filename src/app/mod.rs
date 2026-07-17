mod theme;

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, TextureHandle, Vec2};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::{ImagePlayStyle, MediaMode};
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
    /// Transient success / info banner: (text, seconds left)
    toast: Option<(String, f32)>,
    /// While playing movies, keep panel above PotPlayer
    pin_while_playing: bool,
    window_was_minimized: bool,
    /// In-app image slideshow
    slide_index: usize,
    slide_elapsed: f32,
    slide_paused: bool,
    slide_tex: Option<(PathBuf, TextureHandle)>,
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
        // Guide user to pick libraries — never invent a disk path for them
        let need_library = {
            let s = session.snapshot();
            s.library_roots.is_empty() || s.library_count == 0
        };
        let last_fit_count = session.ui_count();
        let toast = if need_library {
            Some((
                "请先添加片库目录（右上角齿轮 · 片库设置）".to_string(),
                4.5,
            ))
        } else {
            None
        };
        Self {
            session,
            show_settings: need_library,
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
            toast,
            pin_while_playing: true,
            window_was_minimized: false,
            slide_index: 0,
            slide_elapsed: 0.0,
            slide_paused: false,
            slide_tex: None,
        }
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), 2.4));
    }

    fn playing_now(&self) -> bool {
        self.session.snapshot().phase == SessionPhase::Playing
    }

    fn is_image_mode(&self) -> bool {
        self.session.media_mode() == MediaMode::Image
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.user_hid_to_tray = false;
        self.window_was_minimized = false;
        // Pin only matters for movie + PotPlayer
        if self.playing_now() && self.pin_while_playing && !self.is_image_mode() {
            crate::tray::force_show_and_pin();
        } else {
            crate::tray::force_show_main_window();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        self.user_hid_to_tray = true;
        self.window_was_minimized = false;
        crate::tray::set_main_window_topmost(false);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
    }

    /// Pin above players while Playing movies, but release on minimize so taskbar works.
    fn sync_play_pin_state(&mut self, ctx: &egui::Context, phase: SessionPhase) {
        let minimized = ctx.input(|i| i.viewport().minimized == Some(true));

        // Image slideshow is in-app — no sticky topmost needed
        if self.is_image_mode()
            || phase != SessionPhase::Playing
            || self.user_hid_to_tray
            || !self.pin_while_playing
        {
            if self.last_phase == SessionPhase::Playing || !self.pin_while_playing {
                crate::tray::set_main_window_topmost(false);
            }
            self.window_was_minimized = minimized;
            return;
        }

        // Playing + pin enabled
        if minimized {
            if !self.window_was_minimized {
                // User clicked minimize — must drop TOPMOST or it fights the system
                crate::tray::set_main_window_topmost(false);
            }
            self.window_was_minimized = true;
        } else {
            if self.window_was_minimized {
                // Restored from taskbar — raise above PotPlayers and pin again
                crate::tray::force_show_and_pin();
                self.window_was_minimized = false;
            } else if self.last_phase == SessionPhase::Starting {
                crate::tray::set_main_window_topmost(true);
            } else {
                // Stay above players without restoring (won't undo minimize)
                crate::tray::set_main_window_topmost(true);
            }
        }
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
                    let s = self.session.snapshot();
                    if s.phase == SessionPhase::Playing {
                        self.session.stop();
                    } else if s.phase == SessionPhase::Idle {
                        if s.has_preview {
                            self.session.start();
                        } else {
                            self.session.roll_preview();
                            self.show_toast("已生成预览，再点一次托盘可开播");
                        }
                    }
                    self.show_window(ctx);
                }
                TrayCommand::Reroll => {
                    self.session.reroll();
                    self.show_toast("已再来一批");
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
            let key = f.to_string_lossy().to_string();
            if self.textures.contains_key(&key) {
                continue;
            }
            // Images: load file itself as preview texture
            if is_image_path(f) {
                if let Some(tex) = load_texture(ctx, &key, f) {
                    self.textures.insert(key, tex);
                }
                continue;
            }
            self.thumbs.request(f);
            if let Some(path) = self.thumbs.path_if_ready(f) {
                if let Some(tex) = load_texture(ctx, &key, &path) {
                    self.textures.insert(key, tex);
                }
            }
        }
        let live: std::collections::HashSet<String> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        self.textures.retain(|k, _| live.contains(k));
    }

    fn tick_slideshow(&mut self, ctx: &egui::Context, dt: f32, paths: &[PathBuf], interval: f32) {
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
            self.slide_tex = None;
            self.show_toast("已结束幻灯");
            return;
        }
        if step != 0 {
            let n = paths.len() as i32;
            self.slide_index = ((self.slide_index as i32 + step).rem_euclid(n)) as usize;
            self.slide_elapsed = 0.0;
            self.slide_tex = None;
        } else if !self.slide_paused {
            self.slide_elapsed += dt;
            if self.slide_elapsed >= interval {
                self.slide_elapsed = 0.0;
                self.slide_index = (self.slide_index + 1) % paths.len();
                self.slide_tex = None;
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

    fn draw_slideshow_overlay(&mut self, ctx: &egui::Context, paths: &[PathBuf], interval: u8) {
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

                    if let Some((_, tex)) = &self.slide_tex {
                        let size = tex.size_vec2();
                        let fit = (screen.width() / size.x)
                            .min(screen.height() / size.y)
                            .min(1.5);
                        let draw = size * fit;
                        let rect = egui::Rect::from_center_size(screen.center(), draw);
                        ui.painter().image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(
                                egui::pos2(0.0, 0.0),
                                egui::pos2(1.0, 1.0),
                            ),
                            Color32::WHITE,
                        );
                    } else {
                        ui.painter().text(
                            screen.center(),
                            egui::Align2::CENTER_CENTER,
                            "加载图片…",
                            egui::FontId::proportional(18.0),
                            Color32::from_gray(180),
                        );
                    }

                    // HUD
                    let hud = format!(
                        "{}/{}  ·  {}s  ·  {}  ·  空格暂停  ←/→ 切换  Esc 结束",
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
                                self.slide_tex = None;
                            } else if x > 0.72 {
                                self.slide_index = (self.slide_index + 1) % n;
                                self.slide_elapsed = 0.0;
                                self.slide_tex = None;
                            } else {
                                self.slide_paused = !self.slide_paused;
                            }
                        }
                    }
                });
            });
    }

    fn draw_wall_overlay(&mut self, ctx: &egui::Context, paths: &[PathBuf]) {
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

impl eframe::App for SuijiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.unstable_dt);
        if let Some((_, ref mut left)) = self.toast {
            *left -= dt;
            if *left <= 0.0 {
                self.toast = None;
            } else {
                ctx.request_repaint();
            }
        }

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
        ) || snap.indexing
        {
            // Keep UI live while PotPlayer starts/stops or background index runs.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        self.sync_play_pin_state(ctx, snap.phase);

        // Image play: slideshow timer / wall keys
        let image_playing = snap.media_mode == MediaMode::Image
            && snap.phase == SessionPhase::Playing
            && !snap.current_files.is_empty();
        let image_slideshow =
            image_playing && snap.image_play_style == ImagePlayStyle::Slideshow;
        let image_wall = image_playing && snap.image_play_style == ImagePlayStyle::Wall;

        if image_slideshow {
            if self.last_phase != SessionPhase::Playing {
                self.slide_index = 0;
                self.slide_elapsed = 0.0;
                self.slide_paused = false;
                self.slide_tex = None;
            }
            self.tick_slideshow(
                ctx,
                dt,
                &snap.current_files,
                snap.slideshow_interval_secs as f32,
            );
        } else if image_wall {
            if self.last_phase != SessionPhase::Playing {
                self.slide_index = 0;
                self.slide_tex = None;
            }
            // Esc ends wall
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.session.stop();
                self.slide_tex = None;
                self.show_toast("已关闭平铺墙");
            }
            ctx.request_repaint();
        } else if self.last_phase == SessionPhase::Playing && snap.media_mode == MediaMode::Image
        {
            self.slide_tex = None;
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

                // Success / info toast under title bar
                // Note: do not use "✓" as text — CJK primary font often lacks it and
                // renders a green tofu box on the left (see mode-switch toasts).
                if let Some((ref msg, left)) = self.toast {
                    let alpha = (left / 0.35).clamp(0.0, 1.0).min(1.0);
                    let a = (220.0 * alpha) as u8;
                    let icon_a = (255.0 * alpha) as u8;
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(28, 25, 23, a))
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(2.0);
                                let (icon_rect, _) =
                                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                                ui.painter().circle_filled(
                                    icon_rect.center(),
                                    4.5,
                                    Color32::from_rgba_unmultiplied(0x86, 0xEF, 0xAC, icon_a),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(msg.as_str())
                                        .size(14.0)
                                        .color(ON_INK),
                                );
                            });
                        });
                }

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
                            // Mode switch
                            ui.horizontal(|ui| {
                                mode_chip(
                                    ui,
                                    "电影",
                                    snap.media_mode == MediaMode::Movie,
                                    || {
                                        self.session.set_media_mode(MediaMode::Movie);
                                        self.slide_tex = None;
                                        self.textures.clear();
                                        self.fit_height_frames = 6;
                                        self.show_toast("已切换到电影模式");
                                    },
                                );
                                ui.add_space(6.0);
                                mode_chip(
                                    ui,
                                    "图片",
                                    snap.media_mode == MediaMode::Image,
                                    || {
                                        self.session.set_media_mode(MediaMode::Image);
                                        self.slide_tex = None;
                                        self.textures.clear();
                                        self.fit_height_frames = 6;
                                        self.show_toast("已切换到图片模式 · 预览后开启幻灯");
                                    },
                                );
                            });
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(match snap.media_mode {
                                            MediaMode::Movie => "随机片库 · PotPlayer",
                                            MediaMode::Image => "随机图库 · 内置幻灯",
                                        })
                                        .size(10.5)
                                        .color(FAINT)
                                        .extra_letter_spacing(1.5),
                                    );
                                    ui.add_space(1.0);
                                    ui.label(
                                        RichText::new(match snap.media_mode {
                                            MediaMode::Movie => "今日片单",
                                            MediaMode::Image => "今日图集",
                                        })
                                        .size(28.0)
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
                                    // Pin only for movie + PotPlayer
                                    if snap.media_mode == MediaMode::Movie {
                                        let pin_tip = if self.pin_while_playing {
                                            "播放时置顶：开（点此关闭）"
                                        } else {
                                            "播放时置顶：关（点此开启）"
                                        };
                                        if icon_btn_toggle(
                                            ui,
                                            IconKind::Pin,
                                            pin_tip,
                                            self.pin_while_playing,
                                        )
                                        .clicked()
                                        {
                                            self.pin_while_playing = !self.pin_while_playing;
                                            if !self.pin_while_playing {
                                                crate::tray::set_main_window_topmost(false);
                                                self.show_toast("已关闭播放时置顶");
                                            } else {
                                                if self.playing_now() && !self.user_hid_to_tray {
                                                    crate::tray::force_show_and_pin();
                                                }
                                                self.show_toast(
                                                    "已开启播放时置顶（最小化时会自动取消）",
                                                );
                                            }
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
                                let (cmin, cmax) = cfg.count_bounds_for(snap.media_mode);
                                ui.label(
                                    RichText::new(format!(
                                        "本轮数量（{}–{}）",
                                        cmin, cmax
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

                    // ── Preview (slate before play / playing set) ──
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 18,
                            right: 18,
                            top: 2,
                            bottom: 4,
                        })
                        .show(ui, |ui| {
                            let files = &snap.current_files;
                            let n = if files.is_empty() {
                                self.session.ui_count()
                            } else {
                                files.len()
                            };
                            let (rows, cols) = crate::tiler::rows_cols(n.max(1));
                            let preview_hint = if snap.phase == SessionPhase::Playing {
                                format!("正在播放 · {rows}×{cols}")
                            } else if snap.has_preview {
                                format!("预览片单 · {rows}×{cols} · 未开播")
                            } else {
                                format!("本轮预览 · 空 · 点「随机预览」生成 {n} 部")
                            };
                            ui.label(
                                RichText::new(preview_hint)
                                    .size(11.0)
                                    .color(FAINT)
                                    .extra_letter_spacing(0.3),
                            );
                            ui.add_space(3.0);

                            egui::Frame::NONE
                                .fill(BG_SOFT)
                                .stroke(Stroke::new(1.0, LINE))
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    let gap = 6.0;
                                    let total_w = ui.available_width();
                                    let cell_w = ((total_w - gap * (cols as f32 - 1.0))
                                        / cols as f32)
                                        .max(48.0);
                                    let cell_h = (cell_w * 10.0 / 16.0).clamp(78.0, 118.0);

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

                    // ── List ops: preview remove OR playing controls ──
                    if snap.phase == SessionPhase::Idle && snap.has_preview {
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
                                    RichText::new("预览片单 · 可剔除后再开播")
                                        .size(11.0)
                                        .color(FAINT),
                                );
                                ui.add_space(3.0);
                                egui::ScrollArea::vertical()
                                    .max_height(120.0)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        let mut remove_idx: Option<usize> = None;
                                        for (i, path) in snap.current_files.iter().enumerate() {
                                            let name = path
                                                .file_name()
                                                .map(|n| n.to_string_lossy().to_string())
                                                .unwrap_or_else(|| path.display().to_string());
                                            egui::Frame::NONE
                                                .fill(BG_SOFT)
                                                .stroke(Stroke::new(1.0, LINE))
                                                .inner_margin(egui::Margin::symmetric(8, 5))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label(
                                                            RichText::new(format!(
                                                                "{}. {}",
                                                                i + 1,
                                                                truncate_path(&name, 26)
                                                            ))
                                                            .size(12.0)
                                                            .color(INK),
                                                        );
                                                        ui.with_layout(
                                                            Layout::right_to_left(Align::Center),
                                                            |ui| {
                                                                if mini_text_btn(ui, "剔除")
                                                                    .clicked()
                                                                {
                                                                    remove_idx = Some(i);
                                                                }
                                                            },
                                                        );
                                                    });
                                                });
                                            ui.add_space(3.0);
                                        }
                                        if let Some(i) = remove_idx {
                                            self.session.remove_preview_item(i);
                                            self.fit_height_frames = 4;
                                            self.show_toast("已从预览中剔除");
                                        }
                                    });
                            });
                    }

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
                                                                if mini_text_btn(ui, "关闭")
                                                                    .clicked()
                                                                {
                                                                    close_idx = Some(it.index);
                                                                }
                                                                ui.add_space(4.0);
                                                                if mini_text_btn(ui, "独播")
                                                                    .clicked()
                                                                {
                                                                    solo_idx = Some(it.index);
                                                                }
                                                                ui.add_space(4.0);
                                                                if mini_text_btn(ui, "置前")
                                                                    .clicked()
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

                    // ── Actions: 预览 → 确认播放 ──
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
                            let has_preview = snap.has_preview;
                            let can_lib =
                                !snap.library_roots.is_empty() && snap.library_count > 0;

                            // Primary
                            let is_img = snap.media_mode == MediaMode::Image;
                            let (primary, primary_ok) = if playing {
                                (
                                    if is_img {
                                        "关闭幻灯"
                                    } else {
                                        "关闭本轮"
                                    },
                                    !busy,
                                )
                            } else if has_preview {
                                (
                                    if is_img {
                                        "开启幻灯"
                                    } else {
                                        "开启播放"
                                    },
                                    !busy && can_lib,
                                )
                            } else {
                                ("随机预览", !busy && can_lib)
                            };

                            if primary_btn(ui, primary, primary_ok).clicked() {
                                if playing {
                                    self.session.stop();
                                    self.slide_tex = None;
                                } else if has_preview {
                                    self.session.start();
                                    if is_img {
                                        self.slide_index = 0;
                                        self.slide_elapsed = 0.0;
                                        self.slide_paused = false;
                                        self.slide_tex = None;
                                        let style = self.session.config_clone().image_play_style;
                                        self.show_toast(match style {
                                            ImagePlayStyle::Slideshow => "幻灯开始 · Esc 结束",
                                            ImagePlayStyle::Wall => "平铺墙 · 点击放大 · Esc 结束",
                                        });
                                    } else {
                                        self.show_toast("正在按预览片单开播…");
                                    }
                                } else {
                                    self.session.roll_preview();
                                    self.fit_height_frames = 6;
                                    self.show_toast(if is_img {
                                        "已生成图集预览，确认后点「开启幻灯」"
                                    } else {
                                        "已生成预览，确认后点「开启播放」"
                                    });
                                }
                            }

                            ui.add_space(6.0);
                            // Secondary matches 随机预览: 4 chars
                            let reroll_ok = !busy
                                && can_lib
                                && (playing || snap.phase == SessionPhase::Idle);
                            if secondary_btn(ui, ui.available_width(), "再来一批", reroll_ok)
                                .clicked()
                            {
                                self.session.reroll();
                                self.fit_height_frames = 6;
                                if playing {
                                    self.show_toast("已停播并再来一批（未自动播放）");
                                } else {
                                    self.show_toast("已再来一批");
                                }
                            }
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
                    // Fit height to content (larger preview already fills more space).
                    // Floor keeps room for settings modal without huge empty strip.
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                        w.clamp(500.0, 680.0),
                        used_h.clamp(640.0, 1200.0),
                    )));
                    self.fit_height_frames = self.fit_height_frames.saturating_sub(1);
                    ctx.request_repaint();
                }
            });

        if self.show_settings {
            self.settings_modal(ctx);
        }

        // Full-window image play overlays
        if image_slideshow {
            self.draw_slideshow_overlay(
                ctx,
                &snap.current_files,
                snap.slideshow_interval_secs,
            );
        } else if image_wall {
            self.draw_wall_overlay(ctx, &snap.current_files);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.session.shutdown_if_needed();
    }
}

impl SuijiApp {
    fn settings_modal(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut finish_clicked = false;
        egui::Window::new("片库设置")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            // Smaller than main shell so margins remain around the modal
            .default_size([440.0, 520.0])
            .min_size([400.0, 420.0])
            .max_size([500.0, 640.0])
            .open(&mut open)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(BG)
                    .stroke(Stroke::new(1.0, LINE_STRONG))
                    .corner_radius(2.0)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
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
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
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
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(format!("{}.", i + 1))
                                                        .size(12.0)
                                                        .color(FAINT),
                                                );
                                                ui.label(
                                                    RichText::new(root.as_str())
                                                        .size(13.0)
                                                        .color(INK),
                                                );
                                            });
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    if mini_text_btn(ui, "移除").clicked() {
                                                        remove_idx = Some(i);
                                                    }
                                                },
                                            );
                                        });
                                    });
                                ui.add_space(4.0);
                            }
                            if let Some(i) = remove_idx {
                                let name = roots.get(i).cloned().unwrap_or_default();
                                if let Some(removed) = self.session.remove_library_path(i) {
                                    let short = truncate_path(&removed, 28);
                                    self.show_toast(format!("已移除片库：{short}"));
                                    self.fit_height_frames = 3;
                                } else if !name.is_empty() {
                                    self.show_toast(format!("移除失败：{}", truncate_path(&name, 24)));
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
                        self.fit_height_frames = 3;
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
                    // Two equal columns so 下限 / 上限 stay on one level row
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
                    ui.horizontal(|ui| {
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
                    RichText::new("PotPlayer 路径（电影模式，可留空自动探测）")
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
                    RichText::new(
                        "缩略图优先用片源旁已有图片（同名.jpg / poster / cover / 封面 等），否则用 ffmpeg 抽帧",
                    )
                    .size(11.0)
                    .color(FAINT),
                );

                ui.add_space(16.0);
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
            open = false;
            self.fit_height_frames = 4;
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
        } else if self.show_settings && !open {
            // Closed via window X — changes were already live-saved
            self.show_toast("已关闭设置（修改已即时生效）");
        }
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

fn is_image_path(p: &Path) -> bool {
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

fn mode_chip(ui: &mut egui::Ui, label: &str, active: bool, on_click: impl FnOnce()) {
    let fill = if active { INK } else { BG };
    let fg = if active { ON_INK } else { MUTED };
    let stroke = if active {
        Stroke::new(1.0, INK)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    let btn = egui::Button::new(RichText::new(label).size(13.0).color(fg))
        .fill(fill)
        .stroke(stroke)
        .min_size(Vec2::new(72.0, 30.0));
    if ui.add(btn).clicked() {
        on_click();
    }
}

fn status_pill(ui: &mut egui::Ui, phase: SessionPhase, message: &str) {
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
fn bound_stepper(
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

fn small_step_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let size = Vec2::new(34.0, 34.0);
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
    let height = 48.0;
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
        egui::FontId::proportional(16.0),
        ON_INK,
    );
    if enabled {
        resp
    } else {
        ui.interact(rect, ui.id().with("disabled_primary"), Sense::hover())
    }
}

fn mini_text_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let pad = Vec2::new(10.0, 5.0);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(12.5),
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
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 42.0), Sense::click());
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
        egui::FontId::proportional(14.0),
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
        egui::Label::new(RichText::new(text).size(13.0).color(MUTED)).sense(Sense::click()),
    )
}

#[derive(Clone, Copy)]
enum IconKind {
    Settings,
    Rescan,
    Tray,
    Pin,
}

fn icon_btn(ui: &mut egui::Ui, kind: IconKind, tip: &str) -> egui::Response {
    icon_btn_toggle(ui, kind, tip, false)
}

/// Magazine-style square icon button; `active` draws stronger (for pin on).
fn icon_btn_toggle(ui: &mut egui::Ui, kind: IconKind, tip: &str, active: bool) -> egui::Response {
    let size = Vec2::new(40.0, 40.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let stroke = if active || hovered {
        Stroke::new(1.2, INK)
    } else {
        Stroke::new(1.0, LINE_STRONG)
    };
    let fill = if active {
        Color32::from_rgb(0xE7, 0xE0, 0xD6)
    } else if hovered {
        BG_SOFT
    } else {
        BG
    };
    ui.painter().rect_filled(rect, 2.0, fill);
    ui.painter()
        .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);

    let c = rect.center();
    let ink = if active || hovered { INK } else { MUTED };
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
            egui::FontId::proportional(11.0),
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
