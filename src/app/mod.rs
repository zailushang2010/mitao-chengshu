mod theme;
mod widgets;
mod media_view;
mod settings;

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, TextureHandle, Vec2};
use std::collections::{HashMap, HashSet};
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
    file_title, mode_chip, preview_cell, primary_btn_w, row_action_btn, secondary_btn,
    selection_bar, sidebar_list_row, small_step_btn, status_pill, truncate_path, IconKind,
    SelectionBarAction,
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
    /// Seconds since last automatic index freshness check.
    index_fresh_accum: f32,
    /// Second instance asked us to show (named event).
    show_from_second: Arc<AtomicBool>,
    /// Multi-select indices on idle preview slate (batch 换/剔除/拉黑).
    preview_sel: HashSet<usize>,
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

        let cfg0 = session.config_clone();
        let pot_path_edit = cfg0.potplayer_path.clone();
        let pin_while_playing = cfg0.pin_while_playing;
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
            last_media_mode,
            last_phase: SessionPhase::Idle,
            user_hid_to_tray: false,
            toast,
            pin_while_playing,
            pin_level_applied: false,
            window_was_minimized: false,
            slide_index: 0,
            slide_elapsed: 0.0,
            slide_paused: false,
            slide_tex: None,
            slide_prev: None,
            slide_fade: 1.0,
            reap_accum: 0.0,
            index_fresh_accum: 0.0,
            show_from_second,
            preview_sel: HashSet::new(),
        }
    }

    fn clear_preview_sel(&mut self) {
        self.preview_sel.clear();
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
        let snap = self.session.snapshot();
        if self.should_stay_above_players(snap.phase, snap.movie_in_background) {
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

    /// PotPlayers we launched are still on screen (foreground play or parked under 图片).
    fn should_stay_above_players(&self, phase: SessionPhase, movie_in_background: bool) -> bool {
        if self.user_hid_to_tray {
            return false;
        }
        // 并存：切到图片后电影仍在播时，面板必须可点（不依赖图钉开关）
        if movie_in_background {
            return true;
        }
        // 纯电影播放：尊重「播放时置顶」
        self.pin_while_playing && !self.is_image_mode() && phase == SessionPhase::Playing
    }

    /// Raise/pin above our PotPlayers; release on minimize / tray / pin-off / no pots.
    fn sync_play_pin_state(
        &mut self,
        ctx: &egui::Context,
        phase: SessionPhase,
        movie_in_background: bool,
    ) {
        let minimized = ctx.input(|i| i.viewport().minimized == Some(true));

        let want_pin =
            self.should_stay_above_players(phase, movie_in_background) && !minimized;

        if !want_pin {
            if self.pin_level_applied || crate::tray::pin_desired() {
                self.apply_window_level(ctx, false);
            }
            self.window_was_minimized = minimized;
            return;
        }

        // Pots running + pin enabled + not minimized
        if self.window_was_minimized {
            crate::tray::force_show_and_pin();
            self.window_was_minimized = false;
            self.pin_level_applied = false;
            self.apply_window_level(ctx, true);
        } else if !self.pin_level_applied || !crate::tray::pin_desired() {
            self.apply_window_level(ctx, true);
        } else {
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
                            self.clear_preview_sel();
                            self.show_toast("已生成预览，再点一次托盘可开播");
                        }
                    }
                    self.show_window(ctx);
                }
                TrayCommand::Reroll => {
                    self.session.reroll();
                    self.clear_preview_sel();
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

        // Auto-refresh library when nested folders change (no manual 重新扫描 needed).
        // Tree signature is cheap; full walk only for roots that actually changed.
        self.index_fresh_accum += dt;
        if self.index_fresh_accum >= 20.0 {
            self.index_fresh_accum = 0.0;
            if self.session.refresh_if_stale() {
                self.show_toast("片库更新中…");
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

        self.sync_play_pin_state(ctx, snap.phase, snap.movie_in_background);

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

        // Workbench is fixed-size (user may resize). Do NOT auto-InnerSize on
        // mode/count change — pin icon / ui_count / grid would creep the window
        // a few pixels every switch.
        if snap.media_mode != self.last_media_mode {
            self.last_media_mode = snap.media_mode;
            self.clear_preview_sel();
        }

        // Toast: bottom-center overlay — never covers top toolbar / primary actions.
        if let Some(ref toast) = self.toast {
            let alpha = Self::toast_alpha(toast);
            let a = (220.0 * alpha) as u8;
            let icon_a = (255.0 * alpha) as u8;
            let text_a = (255.0 * alpha) as u8;
            // Enter: rise from below; exit: sink slightly (ease via alpha).
            let rise = (1.0 - alpha) * 12.0;
            egui::Area::new(egui::Id::new("toast_overlay"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0 - rise))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    // Cap width so long messages wrap inside the window
                    let max_w = (ctx.screen_rect().width() - 48.0).clamp(200.0, 520.0);
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(28, 25, 23, a))
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .corner_radius(2.0)
                        .show(ui, |ui| {
                            ui.set_max_width(max_w);
                            ui.horizontal(|ui| {
                                let (icon_rect, _) = ui
                                    .allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                                ui.painter().circle_filled(
                                    icon_rect.center(),
                                    4.5,
                                    Color32::from_rgba_unmultiplied(0x86, 0xEF, 0xAC, icon_a),
                                );
                                ui.add_space(6.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(toast.msg.as_str()).size(14.0).color(
                                            Color32::from_rgba_unmultiplied(
                                                ON_INK.r(),
                                                ON_INK.g(),
                                                ON_INK.b(),
                                                text_a,
                                            ),
                                        ),
                                    )
                                    .wrap(),
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
                ui.vertical(|ui| {
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

                                // Mode switch — primary workbench context
                                mode_chip(
                                    ui,
                                    "电影",
                                    snap.media_mode == MediaMode::Movie,
                                    || {
                                        self.session.set_media_mode(MediaMode::Movie);
                                        self.clear_slides();
                                        // Bring panel back if pots still running
                                        let s = self.session.snapshot();
                                        if self.should_stay_above_players(
                                            s.phase,
                                            s.movie_in_background,
                                        ) {
                                            crate::tray::force_show_and_pin();
                                            self.pin_level_applied = false;
                                            self.apply_window_level(ctx, true);
                                        }
                                    },
                                );
                                mode_chip(
                                    ui,
                                    "图片",
                                    snap.media_mode == MediaMode::Image,
                                    || {
                                        self.session.set_media_mode(MediaMode::Image);
                                        self.clear_slides();
                                        // Parked PotPlayers steal z-order — keep panel usable
                                        // while user browses images (plan A 并存).
                                        let s = self.session.snapshot();
                                        if s.movie_in_background {
                                            // Must surface above parked PotPlayers or UI is unusable
                                            crate::tray::force_show_and_pin();
                                            self.pin_level_applied = false;
                                            self.apply_window_level(ctx, true);
                                        } else {
                                            self.apply_window_level(ctx, false);
                                        }
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
                                    RichText::new("数量")
                                        .size(12.0)
                                        .color(MUTED),
                                );
                                if small_step_btn(ui, "−").clicked() {
                                    let next = count.saturating_sub(1).max(cmin);
                                    self.session.set_ui_count(next);
                                }
                                ui.label(
                                    RichText::new(format!("{count}"))
                                        .size(17.0)
                                        .color(INK)
                                        .strong(),
                                );
                                if small_step_btn(ui, "+").clicked() {
                                    let next = (count + 1).min(cmax);
                                    self.session.set_ui_count(next);
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
                                    // Pin control when pots matter (movie play or parked under 图片)
                                    if snap.media_mode == MediaMode::Movie
                                        || snap.movie_in_background
                                    {
                                        let pin_forced = snap.movie_in_background;
                                        let pin_tip = if pin_forced {
                                            "电影后台播放中：面板保持置顶以便操作"
                                        } else if self.pin_while_playing {
                                            "播放时置顶：开（点此关闭）"
                                        } else {
                                            "播放时置顶：关（点此开启）"
                                        };
                                        let pin_visual = pin_forced || self.pin_while_playing;
                                        if icon_btn_toggle(
                                            ui,
                                            IconKind::Pin,
                                            pin_tip,
                                            pin_visual,
                                        )
                                        .clicked()
                                        {
                                            if pin_forced {
                                                self.show_toast(
                                                    "电影仍在后台时需保持置顶，否则无法点到本界面",
                                                );
                                            } else {
                                                self.pin_while_playing = !self.pin_while_playing;
                                                self.session
                                                    .set_pin_while_playing(self.pin_while_playing);
                                                if !self.pin_while_playing {
                                                    self.apply_window_level(ctx, false);
                                                    self.show_toast("已关闭播放时置顶");
                                                } else {
                                                    if self.playing_now() && !self.user_hid_to_tray
                                                    {
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
                                    }
                                    // 左键检查 · 右键全量
                                    let rescan_resp = icon_btn(ui, IconKind::Rescan, "检查更新");
                                    let mut want_force: Option<bool> = None;
                                    if rescan_resp.clicked() {
                                        want_force = Some(false);
                                    }
                                    rescan_resp.context_menu(|ui| {
                                        if ui.button("检查更新").clicked() {
                                            want_force = Some(false);
                                            ui.close_menu();
                                        }
                                        if ui.button("全量扫描").clicked() {
                                            want_force = Some(true);
                                            ui.close_menu();
                                        }
                                    });
                                    if let Some(force) = want_force {
                                        use crate::session::RescanOutcome;
                                        let outcome = if force {
                                            self.session.rescan_force()
                                        } else {
                                            self.session.rescan()
                                        };
                                        match outcome {
                                            RescanOutcome::AlreadyFresh { count } => {
                                                self.show_toast(format!("已是最新 · {count}"));
                                            }
                                            RescanOutcome::Started { force: true } => {
                                                self.show_toast("全量扫描…");
                                            }
                                            RescanOutcome::Started { force: false } => {
                                                self.show_toast("更新中…");
                                            }
                                            RescanOutcome::Busy => {
                                                self.show_toast("索引中");
                                            }
                                            RescanOutcome::NoRoots => {
                                                self.show_toast("请先添加片库");
                                            }
                                        }
                                    }
                                    if icon_btn(ui, IconKind::Settings, "片库设置").clicked() {
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

                    // ── Workbench body: fill remaining window only (never force taller) ──
                    let body_h = ui.available_height().max(1.0);
                    let body_w = ui.available_width().max(1.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(body_w, body_h),
                        Layout::top_down(Align::Min),
                        |ui| {
                            // Clip stage so dense chrome never paints outside the window.
                            ui.set_clip_rect(ui.max_rect());
                            egui::Frame::NONE
                                .inner_margin(egui::Margin {
                                    left: 12,
                                    right: 12,
                                    top: 8,
                                    bottom: 8,
                                })
                                .show(ui, |ui| {
                                    let stage_w = ui.available_width().max(80.0);
                                    ui.set_width(stage_w);
                                    // Remaining budget for the preview slate after chrome rows.
                                    // Keep a floor so the grid stays usable, but never exceed avail.

                                    let starting = snap.phase == SessionPhase::Starting;
                                    let stopping = snap.phase == SessionPhase::Stopping;
                                    let busy = starting || stopping;
                                    let playing = snap.phase == SessionPhase::Playing;
                                    let has_preview = snap.has_preview;
                                    let can_lib = !snap.library_roots.is_empty()
                                        && snap.library_count > 0;
                                    let is_img = snap.media_mode == MediaMode::Image;
                                    let is_wall = is_img
                                        && snap.image_play_style == ImagePlayStyle::Wall;
                                    let files = &snap.current_files;
                                    let n = if files.is_empty() {
                                        self.session.ui_count()
                                    } else {
                                        files.len()
                                    };
                                    let (rows, cols) = crate::tiler::rows_cols(n.max(1));
                                    let idle_preview = snap.phase == SessionPhase::Idle
                                        && has_preview
                                        && !files.is_empty();
                                    if idle_preview {
                                        self.preview_sel.retain(|&i| i < files.len());
                                    }
                                    let sel_n = if idle_preview {
                                        self.preview_sel.len()
                                    } else {
                                        0
                                    };

                                    // ── Action strip: primary + reroll + avoid ──
                                    let (primary, primary_ok) = if starting {
                                        ("取消开启", true)
                                    } else if playing {
                                        (
                                            if is_img {
                                                if is_wall {
                                                    "关闭平铺"
                                                } else {
                                                    "关闭幻灯"
                                                }
                                            } else {
                                                "关闭本轮"
                                            },
                                            !busy,
                                        )
                                    } else if has_preview {
                                        (
                                            if is_img {
                                                if is_wall {
                                                    "开启平铺墙"
                                                } else {
                                                    "开启幻灯"
                                                }
                                            } else if snap.pot_available {
                                                "开启播放"
                                            } else {
                                                // No PotPlayer: open with OS default
                                                "系统打开"
                                            },
                                            !busy && can_lib,
                                        )
                                    } else {
                                        ("随机预览", !busy && can_lib)
                                    };
                                    let reroll_ok = !busy
                                        && can_lib
                                        && (playing || snap.phase == SessionPhase::Idle);

                                    // ── Control strip: primary actions in one paper bar ──
                                    egui::Frame::NONE
                                        .fill(BG_SOFT)
                                        .stroke(Stroke::new(1.0, LINE))
                                        .inner_margin(egui::Margin::symmetric(10, 8))
                                        .corner_radius(2.0)
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.horizontal_wrapped(|ui| {
                                                ui.spacing_mut().item_spacing.x = 8.0;
                                                ui.spacing_mut().item_spacing.y = 6.0;
                                                if primary_btn_w(
                                                    ui,
                                                    112.0,
                                                    32.0,
                                                    primary,
                                                    primary_ok,
                                                )
                                                .clicked()
                                                {
                                                    if starting {
                                                        self.session.stop();
                                                        self.show_toast("已取消");
                                                    } else if playing {
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
                                                                    "幻灯 · Esc 结束"
                                                                }
                                                                ImagePlayStyle::Wall => {
                                                                    "平铺墙 · Esc 结束"
                                                                }
                                                            });
                                                        } else if snap.pot_available {
                                                            self.show_toast("开播中…");
                                                        } else {
                                                            self.show_toast(
                                                                "无 PotPlayer · 用系统默认打开",
                                                            );
                                                        }
                                                    } else {
                                                        self.session.roll_preview();
                                                        self.clear_preview_sel();
                                                        self.show_toast("已生成预览");
                                                    }
                                                }
                                                if secondary_btn(
                                                    ui,
                                                    96.0,
                                                    "再来一批",
                                                    reroll_ok,
                                                )
                                                .clicked()
                                                {
                                                    self.session.reroll();
                                                    self.clear_preview_sel();
                                                    self.show_toast(if playing {
                                                        "已换一批"
                                                    } else {
                                                        "已换一批"
                                                    });
                                                }
                                                ui.add_space(6.0);
                                                ui.label(
                                                    RichText::new("避开最近")
                                                        .size(12.0)
                                                        .color(MUTED),
                                                );
                                                let mut avoid = cfg.avoid_recent;
                                                if toggle(ui, &mut avoid) {
                                                    self.session.set_avoid_recent(avoid);
                                                }
                                                if !snap.last_errors.is_empty() {
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "{} 项异常",
                                                            snap.last_errors.len()
                                                        ))
                                                        .size(12.0)
                                                        .color(Color32::from_rgb(
                                                            0xB4, 0x53, 0x09,
                                                        )),
                                                    );
                                                }
                                            });
                                        });

                                    ui.add_space(6.0);

                                    // Meta + 全选
                                    ui.horizontal(|ui| {
                                        let preview_hint = if playing {
                                            format!("播放中 · {rows}×{cols}")
                                        } else if has_preview {
                                            if !is_img && !snap.pot_available {
                                                format!("预览 · {rows}×{cols} · 无 Pot")
                                            } else {
                                                format!("预览 · {rows}×{cols}")
                                            }
                                        } else if snap.library_roots.is_empty() {
                                            "未添加片库".to_string()
                                        } else if snap.indexing {
                                            "索引中…".to_string()
                                        } else if !can_lib {
                                            "片库为空".to_string()
                                        } else if !is_img && !snap.pot_available {
                                            "待预览 · 未检测到 PotPlayer".to_string()
                                        } else {
                                            format!(
                                                "待预览 · {} {}",
                                                self.session.ui_count(),
                                                if is_img { "张" } else { "部" }
                                            )
                                        };
                                        ui.label(
                                            RichText::new(preview_hint)
                                                .size(12.0)
                                                .color(MUTED),
                                        );
                                        if idle_preview && sel_n == 0 {
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    if mini_text_btn(ui, "全选").clicked() {
                                                        self.preview_sel =
                                                            (0..files.len()).collect();
                                                    }
                                                },
                                            );
                                        }
                                    });

                                    if idle_preview && sel_n > 0 {
                                        ui.add_space(4.0);
                                        if let Some(act) = selection_bar(ui, sel_n) {
                                            let idxs: Vec<usize> = self
                                                .preview_sel
                                                .iter()
                                                .copied()
                                                .collect();
                                            match act {
                                                SelectionBarAction::Replace => {
                                                    match self
                                                        .session
                                                        .replace_preview_items(&idxs)
                                                    {
                                                        Ok(n) => {
                                                            self.clear_preview_sel();
                                                            self.show_toast(format!("已换 {n}"));
                                                        }
                                                        Err(e) => {
                                                            self.clear_preview_sel();
                                                            self.show_toast(e);
                                                        }
                                                    }
                                                }
                                                SelectionBarAction::Remove => {
                                                    let n = self
                                                        .session
                                                        .remove_preview_items(&idxs);
                                                    self.clear_preview_sel();
                                                    self.show_toast(format!("已剔除 {n}"));
                                                }
                                                SelectionBarAction::Blacklist => {
                                                    let n = self
                                                        .session
                                                        .blacklist_preview_items(&idxs);
                                                    self.clear_preview_sel();
                                                    self.show_toast(format!("已拉黑 {n}"));
                                                }
                                                SelectionBarAction::Clear => {
                                                    self.clear_preview_sel();
                                                }
                                            }
                                        }
                                    }

                                    // Playing: compact scroll strip — leave most height for grid
                                    if playing && !snap.items.is_empty() {
                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new("本轮")
                                                    .size(12.0)
                                                    .color(MUTED),
                                            );
                                            if snap.media_mode == MediaMode::Movie {
                                                ui.with_layout(
                                                    Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        if mini_text_btn(ui, "重新平铺").clicked()
                                                        {
                                                            self.session.retile_now();
                                                            self.show_toast("重新平铺…");
                                                        }
                                                    },
                                                );
                                            }
                                        });
                                        // Cap list so short windows still keep a visible grid
                                        let room = ui.available_height();
                                        let list_h = (room * 0.22).clamp(48.0, 96.0).min(room * 0.35);
                                        egui::ScrollArea::vertical()
                                            .id_salt("playing_items")
                                            .max_height(list_h)
                                            .auto_shrink([false, true])
                                            .show(ui, |ui| {
                                                let mut close_idx: Option<usize> = None;
                                                let mut focus_idx: Option<usize> = None;
                                                let mut solo_idx: Option<usize> = None;
                                                for it in &snap.items {
                                                    let title = file_title(&it.name);
                                                    sidebar_list_row(
                                                        ui,
                                                        it.index + 1,
                                                        &title,
                                                        128.0,
                                                        |ui| {
                                                            if row_action_btn(ui, "关闭").clicked()
                                                            {
                                                                close_idx = Some(it.index);
                                                            }
                                                            if row_action_btn(ui, "独播").clicked()
                                                            {
                                                                solo_idx = Some(it.index);
                                                            }
                                                            if row_action_btn(ui, "置前").clicked()
                                                            {
                                                                focus_idx = Some(it.index);
                                                            }
                                                        },
                                                    );
                                                    ui.add_space(2.0);
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
                                    }

                                    ui.add_space(4.0);

                                    // ── Preview grid: use exact remaining space (no min-size push) ──
                                    let slate_h = ui.available_height().max(1.0);
                                    let slate_w = ui.available_width().max(1.0);
                                    let gap_c = 6.0;
                                    let margin = 6.0;

                                    egui::Frame::NONE
                                        .fill(BG_SOFT)
                                        .stroke(Stroke::new(1.0, LINE))
                                        .inner_margin(margin)
                                        .show(ui, |ui| {
                                            let avail_w = (slate_w - margin * 2.0).max(32.0);
                                            let avail_h = (slate_h - margin * 2.0).max(32.0);
                                            // Exact size — do not set_min larger than remaining,
                                            // which used to shove chrome off-screen.
                                            ui.set_min_size(Vec2::new(avail_w, avail_h));
                                            ui.set_max_size(Vec2::new(avail_w, avail_h));

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
                                                            let (rect, _) = ui
                                                                .allocate_exact_size(
                                                                    Vec2::new(cell_w, cell_h),
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
                                                                &p.to_string_lossy().to_string(),
                                                            )
                                                        });
                                                        let selected = idle_preview
                                                            && self.preview_sel.contains(&idx);
                                                        let cell = preview_cell(
                                                            ui,
                                                            cell_w,
                                                            cell_h,
                                                            &label,
                                                            tex,
                                                            selected,
                                                            idle_preview,
                                                        );
                                                        if idle_preview && cell.clicked() {
                                                            if self.preview_sel.contains(&idx) {
                                                                self.preview_sel.remove(&idx);
                                                            } else {
                                                                self.preview_sel.insert(idx);
                                                            }
                                                        }
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
                });

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


