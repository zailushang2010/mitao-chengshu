mod theme;
mod widgets;
mod media_view;
mod settings;

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, TextureHandle, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::config::{ImagePlayStyle, MediaMode};
use crate::session::{SessionHandle, SessionPhase};
use crate::thumb::ThumbCache;
use crate::tray::{TrayCommand, TrayService};
use theme::{BG, BG_SOFT, FAINT, INK, LINE, MUTED, ON_INK};
use widgets::{toggle, 
    ease_out_cubic, icon_btn, icon_btn_toggle, is_image_path, load_texture, mini_text_btn,
    file_title, mode_chip, preview_cell, primary_btn, row_action_btn, secondary_btn,
    sidebar_list_row, small_step_btn, status_pill, truncate_path, IconKind,
};

pub struct SuijiApp {
    session: SessionHandle,
    /// Desired settings open state (true = opening/open, false = closing/closed).
    show_settings: bool,
    /// 0..=1 visual progress for settings (asymmetric open/close).
    settings_vis: f32,
    pot_path_edit: String,
    thumbs: ThumbCache,
    textures: HashMap<String, TextureHandle>,
    tray: Option<TrayService>,
    /// When true, next close request quits instead of tray-hide
    force_quit: bool,
    /// First frames: force visible + focus so user always sees the window
    boot_frames: u8,
    /// After size settle, center once on the monitor (boot / first show).
    need_center: bool,
    /// Nudge window height to content for a few frames (rare; landscape is mostly fixed).
    fit_height_frames: u8,
    last_fit_count: usize,
    last_media_mode: MediaMode,
    last_phase: SessionPhase,
    /// User hid to tray; don't auto-raise until they ask
    user_hid_to_tray: bool,
    /// Transient success / info banner
    toast: Option<ToastState>,
    /// While playing movies, keep panel above PotPlayer
    pin_while_playing: bool,
    /// Last WindowLevel we applied (avoid redundant viewport cmds; detect HWND drift)
    pin_level_applied: bool,
    window_was_minimized: bool,
    /// Workbench: ops rail desired open state.
    sidebar_open: bool,
    /// 0..=1 visual width of ops rail (animated).
    sidebar_vis: f32,
    /// In-app image slideshow
    slide_index: usize,
    slide_elapsed: f32,
    slide_paused: bool,
    slide_tex: Option<(PathBuf, TextureHandle)>,
    /// Previous slide during short crossfade (opacity blend)
    slide_prev: Option<(PathBuf, TextureHandle)>,
    /// 0 = fully previous, 1 = fully current
    slide_fade: f32,
    /// Seconds since last PotPlayer liveness check.
    reap_accum: f32,
    /// Second instance asked us to show (named event).
    show_from_second: Arc<AtomicBool>,
}

/// Toast life: enter 160ms → hold → exit 180ms (ease-out feel via alpha).
struct ToastState {
    msg: String,
    age: f32,
    hold: f32,
}

const TOAST_ENTER: f32 = 0.16;
const TOAST_EXIT: f32 = 0.18;
/// Settings panel: open slower than close (Emil asymmetric enter/exit).
const SETTINGS_OPEN_SECS: f32 = 0.20;
const SETTINGS_CLOSE_SECS: f32 = 0.12;
/// Workbench ops rail width when fully open.
const SIDEBAR_W: f32 = 268.0;
/// Sidebar width animation (asymmetric).
const SIDEBAR_OPEN_SECS: f32 = 0.18;
const SIDEBAR_CLOSE_SECS: f32 = 0.12;

impl SuijiApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        session: SessionHandle,
        show_from_second: Arc<AtomicBool>,
    ) -> Self {
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
        let last_media_mode = session.media_mode();
        let toast = if need_library {
            Some(ToastState {
                msg: "请先添加片库目录（右上角齿轮 · 片库设置）".to_string(),
                age: 0.0,
                hold: 4.2,
            })
        } else {
            None
        };
        Self {
            session,
            show_settings: need_library,
            settings_vis: 0.0,
            pot_path_edit,
            thumbs: ThumbCache::new(),
            textures: HashMap::new(),
            tray,
            force_quit: false,
            boot_frames: 8,
            need_center: true,
            fit_height_frames: 0,
            last_fit_count,
            last_media_mode,
            last_phase: SessionPhase::Idle,
            user_hid_to_tray: false,
            toast,
            pin_while_playing: true,
            pin_level_applied: false,
            window_was_minimized: false,
            sidebar_open: true,
            sidebar_vis: 1.0,
            slide_index: 0,
            slide_elapsed: 0.0,
            slide_paused: false,
            slide_tex: None,
            slide_prev: None,
            slide_fade: 1.0,
            reap_accum: 0.0,
            show_from_second,
        }
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        // Interruptible: replace mid-flight; re-enter from low alpha via age reset.
        self.toast = Some(ToastState {
            msg: msg.into(),
            age: 0.0,
            hold: 2.0,
        });
    }

    fn toast_alpha(t: &ToastState) -> f32 {
        let enter = TOAST_ENTER;
        let hold = t.hold;
        let exit = TOAST_EXIT;
        if t.age < enter {
            ease_out_cubic((t.age / enter).clamp(0.0, 1.0))
        } else if t.age < enter + hold {
            1.0
        } else {
            let u = ((t.age - enter - hold) / exit).clamp(0.0, 1.0);
            1.0 - ease_out_cubic(u)
        }
    }

    fn toast_alive(t: &ToastState) -> bool {
        t.age < TOAST_ENTER + t.hold + TOAST_EXIT
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
            self.apply_window_level(ctx, true);
        } else {
            crate::tray::force_show_main_window();
            self.apply_window_level(ctx, false);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        self.user_hid_to_tray = true;
        self.window_was_minimized = false;
        self.apply_window_level(ctx, false);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
    }

    /// Keep winit ALWAYS_ON_TOP and Win32 HWND in sync. Bare SetWindowPos alone is
    /// wiped the next time winit reapplies window flags.
    fn apply_window_level(&mut self, ctx: &egui::Context, on: bool) {
        if self.pin_level_applied != on {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            self.pin_level_applied = on;
        }
        crate::tray::set_main_window_topmost(on);
    }

    /// Pin above players while Playing movies, but release on minimize so taskbar works.
    fn sync_play_pin_state(&mut self, ctx: &egui::Context, phase: SessionPhase) {
        let minimized = ctx.input(|i| i.viewport().minimized == Some(true));

        // Image slideshow is in-app — no sticky topmost needed
        let want_pin = !self.is_image_mode()
            && phase == SessionPhase::Playing
            && !self.user_hid_to_tray
            && self.pin_while_playing
            && !minimized;

        if !want_pin {
            // Drop pin when idle / image / tray / user turned off / minimized
            if self.pin_level_applied
                || crate::tray::pin_desired()
                || self.last_phase == SessionPhase::Playing
                || !self.pin_while_playing
            {
                self.apply_window_level(ctx, false);
            }
            self.window_was_minimized = minimized;
            return;
        }

        // Playing + pin enabled + visible
        if self.window_was_minimized {
            // Restored from taskbar — raise above PotPlayers and pin again
            crate::tray::force_show_and_pin();
            self.window_was_minimized = false;
            // Force winit re-apply even if it already thought we were topmost
            self.pin_level_applied = false;
            self.apply_window_level(ctx, true);
        } else if !self.pin_level_applied || !crate::tray::pin_desired() {
            self.apply_window_level(ctx, true);
        } else {
            // HWND may have been cleared by a flash-raise; re-assert z-order cheaply
            crate::tray::set_main_window_topmost(true);
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
}

impl eframe::App for SuijiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.unstable_dt);
        if let Some(ref mut t) = self.toast {
            t.age += dt;
            if !Self::toast_alive(t) {
                self.toast = None;
            } else {
                ctx.request_repaint();
            }
        }

        if self.boot_frames > 0 {
            self.boot_frames -= 1;
            self.show_window(ctx);
            // Re-assert center while monitor metrics become available
            self.need_center = true;
            ctx.request_repaint();
        }

        if self.need_center {
            if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
                ctx.send_viewport_cmd(cmd);
                self.need_center = false;
            }
        }

        // Workbench ops rail width (ease-out open, faster close)
        if self.sidebar_open {
            if self.sidebar_vis < 1.0 {
                self.sidebar_vis =
                    (self.sidebar_vis + dt / SIDEBAR_OPEN_SECS).min(1.0);
                ctx.request_repaint();
            }
        } else if self.sidebar_vis > 0.0 {
            self.sidebar_vis =
                (self.sidebar_vis - dt / SIDEBAR_CLOSE_SECS).max(0.0);
            ctx.request_repaint();
        }

        self.poll_tray(ctx);

        // Second instance double-clicked the exe → raise this window (incl. tray-hidden).
        if self
            .show_from_second
            .swap(false, Ordering::SeqCst)
        {
            self.show_window(ctx);
            self.show_toast("已切换到当前窗口");
            ctx.request_repaint();
        }

        // While we may be hidden, keep a light repaint heartbeat so tray
        // channel is drained even if winit is quiet.
        if self.tray.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(400));
        }
        // Always poll second-instance wake reasonably often
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

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

        // Reap closed PotPlayers ~1s (playing or parked background).
        self.reap_accum += dt;
        if self.reap_accum >= 1.0 {
            self.reap_accum = 0.0;
            if self.session.reap_dead_players() {
                ctx.request_repaint();
            }
        }

        let snap = self.session.snapshot();
        if matches!(
            snap.phase,
            SessionPhase::Starting | SessionPhase::Stopping | SessionPhase::Playing
        ) || snap.indexing
            || snap.movie_in_background
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
                self.clear_slides();
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
                self.clear_slides();
            }
            // Esc ends wall
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.session.stop();
                self.clear_slides();
                self.show_toast("已关闭平铺墙");
            }
            ctx.request_repaint();
        } else if self.last_phase == SessionPhase::Playing && snap.media_mode == MediaMode::Image
        {
            self.clear_slides();
        }
        self.last_phase = snap.phase;

        self.ensure_thumbs(&snap.current_files, ctx);
        let cfg = self.session.config_clone();

        // Count / mode changes and window height:
        // Mode switch often changes ui_count (movie vs image defaults) — must NOT
        // thrash InnerSize for 6 frames, or the window looks like it "reloads".
        let count_now = self.session.ui_count();
        if snap.media_mode != self.last_media_mode {
            self.last_media_mode = snap.media_mode;
            self.last_fit_count = count_now;
            // One gentle remeasure after layout settles (pin row may appear/disappear).
            self.fit_height_frames = 1;
        } else if count_now != self.last_fit_count {
            self.last_fit_count = count_now;
            self.fit_height_frames = 3;
        }

        // Toast as overlay — does not push content or resize the window.
        if let Some(ref toast) = self.toast {
            let alpha = Self::toast_alpha(toast);
            let a = (220.0 * alpha) as u8;
            let icon_a = (255.0 * alpha) as u8;
            let text_a = (255.0 * alpha) as u8;
            let y = 8.0 + (1.0 - alpha) * 8.0;
            egui::Area::new(egui::Id::new("toast_overlay"))
                .fixed_pos(egui::pos2(12.0, y))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(28, 25, 23, a))
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .corner_radius(2.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (icon_rect, _) = ui
                                    .allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                                ui.painter().circle_filled(
                                    icon_rect.center(),
                                    4.5,
                                    Color32::from_rgba_unmultiplied(0x86, 0xEF, 0xAC, icon_a),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(toast.msg.as_str()).size(14.0).color(
                                        Color32::from_rgba_unmultiplied(
                                            ON_INK.r(),
                                            ON_INK.g(),
                                            ON_INK.b(),
                                            text_a,
                                        ),
                                    ),
                                );
                            });
                        });
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG).inner_margin(0.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);

                // Workbench chrome: compact single-row toolbar (not magazine title stack).
                let stack = ui.vertical(|ui| {
                    // ── Toolbar ──
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(0xF7, 0xF3, 0xEC))
                        .inner_margin(egui::Margin {
                            left: 12,
                            right: 12,
                            top: 8,
                            bottom: 8,
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                ui.set_min_height(40.0);

                                // Ops rail toggle
                                let rail_tip = if self.sidebar_open {
                                    "隐藏操作栏"
                                } else {
                                    "显示操作栏"
                                };
                                if icon_btn_toggle(
                                    ui,
                                    IconKind::Sidebar,
                                    rail_tip,
                                    self.sidebar_open,
                                )
                                .clicked()
                                {
                                    self.sidebar_open = !self.sidebar_open;
                                }

                                // Mode switch — primary workbench context
                                mode_chip(
                                    ui,
                                    "电影",
                                    snap.media_mode == MediaMode::Movie,
                                    || {
                                        self.session.set_media_mode(MediaMode::Movie);
                                        self.clear_slides();
                                    },
                                );
                                mode_chip(
                                    ui,
                                    "图片",
                                    snap.media_mode == MediaMode::Image,
                                    || {
                                        self.session.set_media_mode(MediaMode::Image);
                                        self.clear_slides();
                                        self.apply_window_level(ctx, false);
                                    },
                                );

                                // Hairline
                                ui.add_space(4.0);
                                let sep_h = 22.0;
                                let (sep_rect, _) =
                                    ui.allocate_exact_size(Vec2::new(1.0, sep_h), Sense::hover());
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(sep_rect.center().x, sep_rect.center().y - 9.0),
                                        egui::pos2(sep_rect.center().x, sep_rect.center().y + 9.0),
                                    ],
                                    Stroke::new(1.0, LINE),
                                );
                                ui.add_space(4.0);

                                // 本轮数量 — primary workbench control, always in toolbar
                                let (cmin, cmax) = cfg.count_bounds_for(snap.media_mode);
                                let count = self.session.ui_count();
                                ui.label(
                                    RichText::new("本轮数量")
                                        .size(12.0)
                                        .color(MUTED),
                                );
                                ui.label(
                                    RichText::new(format!("({cmin}–{cmax})"))
                                        .size(11.0)
                                        .color(FAINT),
                                );
                                if small_step_btn(ui, "−").clicked() {
                                    self.session.set_ui_count(count.saturating_sub(1));
                                }
                                ui.label(
                                    RichText::new(format!("{count}"))
                                        .size(18.0)
                                        .color(INK)
                                        .strong(),
                                );
                                if small_step_btn(ui, "+").clicked() {
                                    self.session.set_ui_count(count + 1);
                                }

                                // Hairline
                                ui.add_space(4.0);
                                let (sep2, _) =
                                    ui.allocate_exact_size(Vec2::new(1.0, sep_h), Sense::hover());
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(sep2.center().x, sep2.center().y - 9.0),
                                        egui::pos2(sep2.center().x, sep2.center().y + 9.0),
                                    ],
                                    Stroke::new(1.0, LINE),
                                );
                                ui.add_space(4.0);

                                // Status + library meta
                                status_pill(ui, snap.phase, &snap.message);

                                let unit = match snap.media_mode {
                                    MediaMode::Movie => "部",
                                    MediaMode::Image => "张",
                                };
                                let root_label = if snap.library_roots.is_empty() {
                                    "未设置片库".to_string()
                                } else if snap.library_roots.len() == 1 {
                                    truncate_path(&snap.library_roots[0], 18)
                                } else {
                                    format!(
                                        "{} 等{}个",
                                        truncate_path(&snap.library_roots[0], 12),
                                        snap.library_roots.len()
                                    )
                                };
                                let lib_line = if snap.indexing {
                                    if snap.indexing_found > 0 {
                                        format!(
                                            "{root_label} · 索引中 {} {unit}",
                                            snap.indexing_found
                                        )
                                    } else {
                                        format!("{root_label} · 索引中…")
                                    }
                                } else {
                                    format!("{root_label} · {} {unit}", snap.library_count)
                                };
                                // Cap meta width so icon cluster never gets shoved off
                                let meta_w = (ui.available_width() - 200.0).clamp(80.0, 360.0);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(meta_w, 20.0),
                                    Layout::left_to_right(Align::Center),
                                    |ui| {
                                        ui.set_max_width(meta_w);
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(lib_line).size(12.0).color(MUTED),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );

                                if snap.indexing {
                                    if mini_text_btn(ui, "取消").clicked() {
                                        self.session.cancel_scan();
                                        self.show_toast("已取消索引");
                                    }
                                }

                                // Right: tools
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    if self.tray.is_some() {
                                        if icon_btn(ui, IconKind::Tray, "最小化到托盘").clicked()
                                        {
                                            self.hide_to_tray(ctx);
                                        }
                                    }
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
                                                self.apply_window_level(ctx, false);
                                                self.show_toast("已关闭播放时置顶");
                                            } else {
                                                if self.playing_now() && !self.user_hid_to_tray {
                                                    crate::tray::force_show_and_pin();
                                                    self.pin_level_applied = false;
                                                    self.apply_window_level(ctx, true);
                                                }
                                                self.show_toast(
                                                    "已开启播放时置顶（最小化时会自动取消）",
                                                );
                                            }
                                        }
                                    }
                                    if icon_btn(ui, IconKind::Rescan, "重新扫描片库").clicked() {
                                        self.session.rescan();
                                    }
                                    if icon_btn(ui, IconKind::Settings, "片库与设置").clicked() {
                                        self.show_settings = true;
                                        self.pot_path_edit =
                                            self.session.config_clone().potplayer_path;
                                    }
                                });
                            });

                            // Secondary banner only when needed (not every frame chrome)
                            if snap.movie_in_background {
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "电影仍在后台 · {} 部 · 切回「电影」可关",
                                            snap.movie_background_count
                                        ))
                                        .size(12.0)
                                        .color(MUTED),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if mini_text_btn(ui, "关掉电影").clicked() {
                                            self.session.stop_background_movie();
                                            self.show_toast("已关闭后台电影");
                                        }
                                    });
                                });
                            }
                        });

                    ui.add(egui::Separator::default().spacing(0.0));

                    // ── Workbench body: collapsible ops rail · main preview stage ──
                    let body_h = ui.available_height().max(360.0);
                    let t_side = ease_out_cubic(self.sidebar_vis.clamp(0.0, 1.0));
                    let left_w = SIDEBAR_W * t_side;
                    let show_rail = left_w >= 8.0;

                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), body_h),
                        Layout::left_to_right(Align::Min),
                        |ui| {
                            let total_w = ui.available_width();
                            let gap = if show_rail { 0.0 } else { 0.0 };
                            let right_w = (total_w - left_w - gap).max(200.0);

                            // ── LEFT ops rail (narrow fixed width; can hide) ──
                            if show_rail {
                            ui.allocate_ui_with_layout(
                                Vec2::new(left_w, body_h),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    // Soft rail background so it reads as a dock, not empty space
                                    let rail_rect = ui.max_rect();
                                    ui.painter().rect_filled(
                                        rail_rect,
                                        0.0,
                                        Color32::from_rgb(0xF3, 0xEF, 0xE8),
                                    );
                                    ui.painter().line_segment(
                                        [
                                            egui::pos2(rail_rect.right() - 0.5, rail_rect.top()),
                                            egui::pos2(rail_rect.right() - 0.5, rail_rect.bottom()),
                                        ],
                                        Stroke::new(1.0, LINE),
                                    );

                                    egui::Frame::NONE
                                        .inner_margin(egui::Margin {
                                            left: 14,
                                            right: 12,
                                            top: 10,
                                            bottom: 10,
                                        })
                                        .show(ui, |ui| {
                                            ui.set_width((left_w - 26.0).max(40.0));
                                            ui.set_clip_rect(ui.max_rect());
                                            ui.spacing_mut().item_spacing.y = 6.0;

                                            // Avoid recent (count lives in the top toolbar)
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new("避开最近播放")
                                                        .size(13.0)
                                                        .color(MUTED),
                                                );
                                                ui.with_layout(
                                                    Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        let mut avoid = cfg.avoid_recent;
                                                        if toggle(ui, &mut avoid) {
                                                            self.session.set_avoid_recent(avoid);
                                                        }
                                                    },
                                                );
                                            });

                                            if !snap.last_errors.is_empty() {
                                                ui.colored_label(
                                                    Color32::from_rgb(0xB4, 0x53, 0x09),
                                                    RichText::new(format!(
                                                        "注意：{} 项启动异常",
                                                        snap.last_errors.len()
                                                    ))
                                                    .size(12.0),
                                                );
                                            }

                                            ui.add_space(4.0);

                                            // Actions up front — left column is the control rail
                                            let busy = matches!(
                                                snap.phase,
                                                SessionPhase::Starting | SessionPhase::Stopping
                                            );
                                            let playing = snap.phase == SessionPhase::Playing;
                                            let has_preview = snap.has_preview;
                                            let can_lib = !snap.library_roots.is_empty()
                                                && snap.library_count > 0;
                                            let is_img = snap.media_mode == MediaMode::Image;
                                            let (primary, primary_ok) = if playing {
                                                (
                                                    if is_img { "关闭幻灯" } else { "关闭本轮" },
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
                                                    self.clear_slides();
                                                } else if has_preview {
                                                    self.session.start();
                                                    if is_img {
                                                        self.slide_index = 0;
                                                        self.slide_elapsed = 0.0;
                                                        self.slide_paused = false;
                                                        self.clear_slides();
                                                        let style = self
                                                            .session
                                                            .config_clone()
                                                            .image_play_style;
                                                        self.show_toast(match style {
                                                            ImagePlayStyle::Slideshow => {
                                                                "幻灯开始 · Esc 结束"
                                                            }
                                                            ImagePlayStyle::Wall => {
                                                                "平铺墙 · 点击放大 · Esc 结束"
                                                            }
                                                        });
                                                    } else {
                                                        self.show_toast("正在按预览片单开播…");
                                                    }
                                                } else {
                                                    self.session.roll_preview();
                                                    self.show_toast(if is_img {
                                                        "已生成图集预览，确认后点「开启幻灯」"
                                                    } else {
                                                        "已生成预览，确认后点「开启播放」"
                                                    });
                                                }
                                            }

                                            ui.add_space(6.0);
                                            let reroll_ok = !busy
                                                && can_lib
                                                && (playing
                                                    || snap.phase == SessionPhase::Idle);
                                            if secondary_btn(
                                                ui,
                                                ui.available_width(),
                                                "再来一批",
                                                reroll_ok,
                                            )
                                            .clicked()
                                            {
                                                self.session.reroll();
                                                if playing {
                                                    self.show_toast(
                                                        "已停播并再来一批（未自动播放）",
                                                    );
                                                } else {
                                                    self.show_toast("已再来一批");
                                                }
                                            }

                                            ui.add_space(10.0);

                                            // List fills all remaining left height
                                            let list_h = ui.available_height().max(120.0);

                                            if snap.phase == SessionPhase::Idle && snap.has_preview
                                            {
                                                ui.label(
                                                    RichText::new("预览片单 · 悬停看全名")
                                                        .size(11.0)
                                                        .color(FAINT),
                                                );
                                                ui.add_space(3.0);
                                                let scroll_h = (list_h - 22.0).max(80.0);
                                                egui::ScrollArea::vertical()
                                                    .max_height(scroll_h)
                                                    .auto_shrink([false, false])
                                                    .show(ui, |ui| {
                                                        let mut remove_idx: Option<usize> = None;
                                                        let mut ban_idx: Option<usize> = None;
                                                        for (i, path) in
                                                            snap.current_files.iter().enumerate()
                                                        {
                                                            let title = path
                                                                .file_name()
                                                                .map(|n| {
                                                                    file_title(
                                                                        &n.to_string_lossy(),
                                                                    )
                                                                })
                                                                .unwrap_or_else(|| {
                                                                    file_title(
                                                                        &path.display().to_string(),
                                                                    )
                                                                });
                                                            // right-to-left: 拉黑 (stronger) on right
                                                            sidebar_list_row(
                                                                ui,
                                                                i + 1,
                                                                &title,
                                                                96.0,
                                                                |ui| {
                                                                    if row_action_btn(ui, "拉黑")
                                                                        .clicked()
                                                                    {
                                                                        ban_idx = Some(i);
                                                                    }
                                                                    if row_action_btn(ui, "剔除")
                                                                        .clicked()
                                                                    {
                                                                        remove_idx = Some(i);
                                                                    }
                                                                },
                                                            );
                                                            ui.add_space(3.0);
                                                        }
                                                        if let Some(i) = ban_idx {
                                                            if let Some(p) = self
                                                                .session
                                                                .blacklist_preview_item(i)
                                                            {
                                                                let short = truncate_path(
                                                                    &p.file_name()
                                                                        .map(|n| {
                                                                            n.to_string_lossy()
                                                                                .to_string()
                                                                        })
                                                                        .unwrap_or_default(),
                                                                    20,
                                                                );
                                                                self.show_toast(format!(
                                                                    "已拉黑：{short}"
                                                                ));
                                                            }
                                                        } else if let Some(i) = remove_idx {
                                                            self.session.remove_preview_item(i);
                                                            self.show_toast("已从预览中剔除");
                                                        }
                                                    });
                                            } else if snap.phase == SessionPhase::Playing
                                                && !snap.items.is_empty()
                                            {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new("本轮影片 · 可单独操作")
                                                            .size(11.0)
                                                            .color(FAINT),
                                                    );
                                                    ui.with_layout(
                                                        Layout::right_to_left(Align::Center),
                                                        |ui| {
                                                            if snap.media_mode == MediaMode::Movie
                                                                && mini_text_btn(ui, "重新平铺")
                                                                    .clicked()
                                                            {
                                                                self.session.retile_now();
                                                                self.show_toast("正在重新平铺…");
                                                            }
                                                        },
                                                    );
                                                });
                                                ui.add_space(3.0);
                                                let scroll_h = (list_h - 22.0).max(80.0);
                                                egui::ScrollArea::vertical()
                                                    .max_height(scroll_h)
                                                    .auto_shrink([false, false])
                                                    .show(ui, |ui| {
                                                        let mut close_idx: Option<usize> = None;
                                                        let mut focus_idx: Option<usize> = None;
                                                        let mut solo_idx: Option<usize> = None;
                                                        for it in &snap.items {
                                                            let title = file_title(&it.name);
                                                            // right-to-left: 关闭 | 独播 | 置前
                                                            sidebar_list_row(
                                                                ui,
                                                                it.index + 1,
                                                                &title,
                                                                128.0,
                                                                |ui| {
                                                                    if row_action_btn(ui, "关闭")
                                                                        .clicked()
                                                                    {
                                                                        close_idx = Some(it.index);
                                                                    }
                                                                    if row_action_btn(ui, "独播")
                                                                        .clicked()
                                                                    {
                                                                        solo_idx = Some(it.index);
                                                                    }
                                                                    if row_action_btn(ui, "置前")
                                                                        .clicked()
                                                                    {
                                                                        focus_idx = Some(it.index);
                                                                    }
                                                                },
                                                            );
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
                                                        }
                                                    });
                                            } else {
                                                // Full-height empty card — left rail stays solid, not hollow
                                                let count = self.session.ui_count();
                                                let unit = if is_img { "张" } else { "部" };
                                                let roots_n = snap.library_roots.len();
                                                egui::Frame::NONE
                                                    .fill(BG_SOFT)
                                                    .stroke(Stroke::new(1.0, LINE))
                                                    .inner_margin(egui::Margin::symmetric(14, 16))
                                                    .show(ui, |ui| {
                                                        ui.set_min_height(list_h - 4.0);
                                                        ui.set_width(ui.available_width());
                                                        ui.spacing_mut().item_spacing.y = 8.0;

                                                        ui.label(
                                                            RichText::new("本轮片单")
                                                                .size(13.0)
                                                                .color(INK)
                                                                .strong(),
                                                        );
                                                        ui.label(
                                                            RichText::new(if roots_n == 0 {
                                                                "尚未添加片库目录".to_string()
                                                            } else if snap.indexing {
                                                                format!(
                                                                    "索引中… 片库 {} 个目录",
                                                                    roots_n
                                                                )
                                                            } else {
                                                                format!(
                                                                    "片库 {} {unit} · 将抽 {} {unit}",
                                                                    snap.library_count, count
                                                                )
                                                            })
                                                            .size(12.0)
                                                            .color(MUTED),
                                                        );

                                                        ui.add_space(6.0);
                                                        ui.separator();
                                                        ui.add_space(6.0);

                                                        let steps: &[&str] = if roots_n == 0 {
                                                            &[
                                                                "1. 右上角齿轮 → 添加片库",
                                                                "2. 调整本轮数量",
                                                                "3. 点「随机预览」生成片单",
                                                            ]
                                                        } else {
                                                            &[
                                                                "1. 点上方「随机预览」",
                                                                "2. 主区查看封面网格",
                                                                "3. 确认后点「开启播放」",
                                                            ]
                                                        };
                                                        for line in steps {
                                                            ui.label(
                                                                RichText::new(*line)
                                                                    .size(12.5)
                                                                    .color(MUTED),
                                                            );
                                                        }

                                                        if !can_lib && roots_n > 0 && !snap.indexing
                                                        {
                                                            ui.add_space(8.0);
                                                            ui.label(
                                                                RichText::new(
                                                                    "当前片库为空，换个目录或重新扫描",
                                                                )
                                                                .size(12.0)
                                                                .color(Color32::from_rgb(
                                                                    0xB4, 0x53, 0x09,
                                                                )),
                                                            );
                                                        }
                                                    });
                                            }
                                        });
                                },
                            );
                            } // end show_rail

                            // ── MAIN STAGE: preview grid (takes remaining width) ──
                            ui.allocate_ui_with_layout(
                                Vec2::new(right_w, body_h),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    egui::Frame::NONE
                                        .inner_margin(egui::Margin {
                                            left: if show_rail { 12 } else { 16 },
                                            right: 16,
                                            top: 10,
                                            bottom: 12,
                                        })
                                        .show(ui, |ui| {
                                            let stage_w = (right_w
                                                - if show_rail { 28.0 } else { 32.0 })
                                            .max(160.0);
                                            ui.set_width(stage_w);
                                            let files = &snap.current_files;
                                            let n = if files.is_empty() {
                                                self.session.ui_count()
                                            } else {
                                                files.len()
                                            };
                                            let (rows, cols) = crate::tiler::rows_cols(n.max(1));
                                            ui.horizontal(|ui| {
                                                let preview_hint =
                                                    if snap.phase == SessionPhase::Playing {
                                                        format!("正在播放 · {rows}×{cols}")
                                                    } else if snap.has_preview {
                                                        format!(
                                                            "本轮预览 · {rows}×{cols} · 未开播"
                                                        )
                                                    } else {
                                                        format!(
                                                            "本轮预览 · 空 · 打开操作栏生成片单"
                                                        )
                                                    };
                                                ui.label(
                                                    RichText::new(preview_hint)
                                                        .size(11.0)
                                                        .color(FAINT)
                                                        .extra_letter_spacing(0.3),
                                                );
                                                if !self.sidebar_open {
                                                    ui.with_layout(
                                                        Layout::right_to_left(Align::Center),
                                                        |ui| {
                                                            if mini_text_btn(ui, "操作栏")
                                                                .clicked()
                                                            {
                                                                self.sidebar_open = true;
                                                            }
                                                        },
                                                    );
                                                }
                                            });
                                            ui.add_space(6.0);

                                            // Remaining height is the slate — grid fills it edge-to-edge.
                                            let slate_h = ui.available_height().max(120.0);
                                            let slate_w = ui.available_width().max(80.0);
                                            let gap_c = 6.0;
                                            let margin = 8.0;

                                            egui::Frame::NONE
                                                .fill(BG_SOFT)
                                                .stroke(Stroke::new(1.0, LINE))
                                                .inner_margin(margin)
                                                .show(ui, |ui| {
                                                    // Exact content area inside frame margins
                                                    let avail_w = (slate_w - margin * 2.0).max(48.0);
                                                    let avail_h = (slate_h - margin * 2.0).max(48.0);
                                                    ui.set_min_size(Vec2::new(avail_w, avail_h));

                                                    // Evenly tile — same idea as PotPlayer grid_layout.
                                                    // No fixed aspect on cells: empty chrome around the grid
                                                    // looked sparse; letterbox only inside each cell.
                                                    let cell_w = ((avail_w
                                                        - gap_c * (cols.saturating_sub(1) as f32))
                                                        / cols as f32)
                                                        .max(32.0);
                                                    let cell_h = ((avail_h
                                                        - gap_c * (rows.saturating_sub(1) as f32))
                                                        / rows as f32)
                                                        .max(32.0);

                                                    for r in 0..rows {
                                                        ui.horizontal(|ui| {
                                                            ui.spacing_mut().item_spacing.x = gap_c;
                                                            for c in 0..cols {
                                                                let idx = r * cols + c;
                                                                if idx >= n {
                                                                    // Empty slot still holds space so grid stays full
                                                                    let (rect, _) = ui
                                                                        .allocate_exact_size(
                                                                            Vec2::new(
                                                                                cell_w, cell_h,
                                                                            ),
                                                                            Sense::hover(),
                                                                        );
                                                                    ui.painter().rect_filled(
                                                                        rect,
                                                                        0.0,
                                                                        Color32::from_rgb(
                                                                            0xE7, 0xE5, 0xE4,
                                                                        ),
                                                                    );
                                                                    ui.painter().rect_stroke(
                                                                        rect,
                                                                        0.0,
                                                                        Stroke::new(1.0, LINE),
                                                                        egui::StrokeKind::Inside,
                                                                    );
                                                                    continue;
                                                                }
                                                                let path_opt = files.get(idx);
                                                                let label = path_opt
                                                                    .and_then(|p| {
                                                                        p.file_name().map(|n| {
                                                                            n.to_string_lossy()
                                                                                .to_string()
                                                                        })
                                                                    })
                                                                    .unwrap_or_default();
                                                                let tex = path_opt.and_then(|p| {
                                                                    self.textures.get(
                                                                        &p.to_string_lossy()
                                                                            .to_string(),
                                                                    )
                                                                });
                                                                preview_cell(
                                                                    ui, cell_w, cell_h, &label,
                                                                    tex,
                                                                );
                                                            }
                                                        });
                                                        if r + 1 < rows {
                                                            ui.add_space(gap_c);
                                                        }
                                                    }
                                                });
                                        });
                                },
                            );
                        },
                    );
                });

                // Landscape is fixed-size by default; rare fit only (settings close etc.).
                // After resize, re-center so the window does not stick to a corner.
                if self.fit_height_frames > 0 && !self.show_settings {
                    let used_h = stack.response.rect.height().ceil() + 4.0;
                    let w = ctx.input(|i| {
                        i.viewport()
                            .inner_rect
                            .map(|r| r.width())
                            .unwrap_or(1024.0)
                    });
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                        w.clamp(720.0, 1440.0),
                        used_h.clamp(480.0, 960.0),
                    )));
                    self.fit_height_frames = self.fit_height_frames.saturating_sub(1);
                    if self.fit_height_frames == 0 {
                        self.need_center = true;
                    }
                    ctx.request_repaint();
                }
            });

        // Settings open/close animation (ease-out open 200ms, faster close 120ms)
        if self.show_settings {
            if self.settings_vis < 1.0 {
                self.settings_vis =
                    (self.settings_vis + dt / SETTINGS_OPEN_SECS).min(1.0);
                ctx.request_repaint();
            }
        } else if self.settings_vis > 0.0 {
            self.settings_vis =
                (self.settings_vis - dt / SETTINGS_CLOSE_SECS).max(0.0);
            ctx.request_repaint();
        }
        if self.settings_vis > 0.001 {
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


