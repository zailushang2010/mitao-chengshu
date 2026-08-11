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
use theme::{BG, BG_MAIN, BG_SOFT, FAINT, INK, LINE, MUTED, ON_INK, RAIL};
use widgets::{toggle, 
    count_stepper, ease_out_cubic, icon_btn, icon_btn_toggle, is_image_path, load_texture,
    mini_text_btn, file_title, nav_item, preview_card, primary_btn_w, row_action_btn,
    mode_chip, search_field, secondary_btn, selection_bar_ex, sidebar_list_row,
    status_pill, truncate_path, IconKind, NavIcon, SelectionBarAction,
};
use crate::config::WindowGeometry;

/// Main stage: library random browse vs favorites shelf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StageView {
    Browse,
    Favorites,
}

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
    /// Filter current slate titles (prototype search, local only).
    slate_query: String,
    /// Preferred card columns 2..=5 (prototype density slider).
    card_cols: u8,
    /// Browse library slate vs favorites shelf.
    stage_view: StageView,
    /// Which favorites shelf is open (independent of left-rail browse mode until play).
    fav_tab: MediaMode,
    /// Debounce timer for window geometry save (seconds).
    geom_dirty_age: Option<f32>,
    /// Last geometry we wrote or loaded (skip no-op saves).
    last_saved_geom: Option<WindowGeometry>,
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
        center_on_boot: bool,
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
        let last_saved_geom = cfg0.window_geometry.map(|g| g.clamp_size());
        let tray = match TrayService::try_new(cc.egui_ctx.clone()) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("tray unavailable: {e}");
                None
            }
        };
        // First-run only: open settings when no library paths are configured.
        // Do NOT key off library_count — boot scan is async, so count is often
        // still 0 even with valid roots (was forcing the modal every launch).
        let need_library = !cfg0.has_library();
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
            need_center: center_on_boot,
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
            slate_query: String::new(),
            card_cols: cfg0.card_cols.clamp(2, 5),
            stage_view: StageView::Browse,
            fav_tab: last_media_mode,
            geom_dirty_age: None,
            last_saved_geom,
        }
    }

    /// Open a favorites shelf without leaving the 收藏 stage.
    fn set_fav_tab(&mut self, tab: MediaMode) {
        if self.fav_tab == tab {
            return;
        }
        self.fav_tab = tab;
        self.clear_preview_sel();
        self.clear_slate_filter();
        // Align session mode so 播放 / 幻灯 / 平铺 走对路径
        if self.session.media_mode() != tab {
            self.session.set_media_mode(tab);
            self.clear_slides();
        }
    }

    fn poll_window_geometry(&mut self, ctx: &egui::Context, dt: f32) {
        // Skip until boot settle; ignore minimized / tray-hidden.
        if self.boot_frames > 0 || self.user_hid_to_tray {
            return;
        }
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if minimized || maximized || fullscreen {
            self.geom_dirty_age = None;
            return;
        }
        let Some((outer, inner)) = ctx.input(|i| {
            let v = i.viewport();
            match (v.outer_rect, v.inner_rect) {
                (Some(o), Some(inn)) => Some((o, inn)),
                _ => None,
            }
        }) else {
            return;
        };
        let geom = WindowGeometry {
            x: outer.min.x,
            y: outer.min.y,
            w: inner.width(),
            h: inner.height(),
        }
        .clamp_size();
        if self.last_saved_geom == Some(geom) {
            self.geom_dirty_age = None;
            return;
        }
        // Mark dirty; write after short idle so drag doesn't thrash disk.
        match self.geom_dirty_age {
            Some(age) => {
                let age = age + dt;
                if age >= 0.6 {
                    self.session.set_window_geometry(geom);
                    self.last_saved_geom = Some(geom);
                    self.geom_dirty_age = None;
                } else {
                    self.geom_dirty_age = Some(age);
                }
            }
            None => self.geom_dirty_age = Some(0.0),
        }
    }

    fn clear_preview_sel(&mut self) {
        self.preview_sel.clear();
    }

    fn clear_slate_filter(&mut self) {
        self.slate_query.clear();
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
            // Only re-center while settling when we intentionally start centered
            if self.need_center {
                // keep flag; apply below
            }
            ctx.request_repaint();
        }

        if self.need_center {
            if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
                ctx.send_viewport_cmd(cmd);
                self.need_center = false;
            }
        }

        self.poll_window_geometry(ctx, dt);
        if self.geom_dirty_age.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
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

        if self.stage_view == StageView::Favorites && snap.phase == SessionPhase::Idle {
            let favs = self.session.favorite_paths_existing();
            self.ensure_thumbs(&favs, ctx);
        } else {
            self.ensure_thumbs(&snap.current_files, ctx);
        }
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

        // Prototype shell: left nav · main (header + actions + cards + footer)
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG).inner_margin(0.0))
            .show(ctx, |ui| {
                let full = ui.available_rect_before_wrap();
                ui.allocate_ui_with_layout(
                    full.size(),
                    Layout::left_to_right(Align::Min),
                    |ui| {
                        let h = ui.available_height();
                        // ── Left nav rail (prototype) ──
                        let rail_w = 76.0;
                        ui.allocate_ui_with_layout(
                            Vec2::new(rail_w, h),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.painter().rect_filled(ui.max_rect(), 0.0, RAIL);
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(ui.max_rect().right() - 0.5, ui.max_rect().top()),
                                        egui::pos2(
                                            ui.max_rect().right() - 0.5,
                                            ui.max_rect().bottom(),
                                        ),
                                    ],
                                    Stroke::new(1.0, LINE),
                                );
                                ui.add_space(18.0);
                                if nav_item(
                                    ui,
                                    "电影",
                                    self.stage_view == StageView::Browse
                                        && snap.media_mode == MediaMode::Movie,
                                    NavIcon::Movie,
                                )
                                .clicked()
                                {
                                    self.stage_view = StageView::Browse;
                                    self.clear_preview_sel();
                                    self.session.set_media_mode(MediaMode::Movie);
                                    self.clear_slides();
                                    let s = self.session.snapshot();
                                    if self.should_stay_above_players(
                                        s.phase,
                                        s.movie_in_background,
                                    ) {
                                        crate::tray::force_show_and_pin();
                                        self.pin_level_applied = false;
                                        self.apply_window_level(ctx, true);
                                    }
                                }
                                ui.add_space(6.0);
                                if nav_item(
                                    ui,
                                    "图片",
                                    self.stage_view == StageView::Browse
                                        && snap.media_mode == MediaMode::Image,
                                    NavIcon::Image,
                                )
                                .clicked()
                                {
                                    self.stage_view = StageView::Browse;
                                    self.clear_preview_sel();
                                    self.session.set_media_mode(MediaMode::Image);
                                    self.clear_slides();
                                    let s = self.session.snapshot();
                                    if s.movie_in_background {
                                        crate::tray::force_show_and_pin();
                                        self.pin_level_applied = false;
                                        self.apply_window_level(ctx, true);
                                    } else {
                                        self.apply_window_level(ctx, false);
                                    }
                                }
                                ui.add_space(6.0);
                                if nav_item(
                                    ui,
                                    "收藏",
                                    self.stage_view == StageView::Favorites,
                                    NavIcon::Star,
                                )
                                .clicked()
                                {
                                    // Enter shelf matching where user came from; tabs stay inside 收藏
                                    self.fav_tab = self.session.media_mode();
                                    self.stage_view = StageView::Favorites;
                                    self.clear_preview_sel();
                                    self.clear_slate_filter();
                                }
                                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                                    ui.add_space(16.0);
                                    if nav_item(ui, "设置", false, NavIcon::Settings).clicked() {
                                        self.show_settings = true;
                                        self.pot_path_edit =
                                            self.session.config_clone().potplayer_path;
                                    }
                                });
                            },
                        );

                        // ── Main column ──
                        let main_w = (ui.available_width()).max(1.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(main_w, h),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_clip_rect(ui.max_rect());
                                ui.painter().rect_filled(ui.max_rect(), 0.0, BG_MAIN);
                                let starting = snap.phase == SessionPhase::Starting;
                                let stopping = snap.phase == SessionPhase::Stopping;
                                let busy = starting || stopping;
                                let playing = snap.phase == SessionPhase::Playing;
                                // Favorites shelf only while idle (playing uses shared player chrome).
                                let fav_stage = self.stage_view == StageView::Favorites
                                    && snap.phase == SessionPhase::Idle;
                                let fav_tab = self.fav_tab;
                                let fav_movie_n =
                                    self.session.favorite_count_for(MediaMode::Movie);
                                let fav_image_n =
                                    self.session.favorite_count_for(MediaMode::Image);
                                let fav_files_buf = if fav_stage {
                                    self.session.favorite_paths_existing_for(fav_tab)
                                } else {
                                    Vec::new()
                                };
                                let has_preview = if fav_stage {
                                    !fav_files_buf.is_empty()
                                } else {
                                    snap.has_preview
                                };
                                let can_lib = !snap.library_roots.is_empty()
                                    && snap.library_count > 0;
                                // In 收藏台, unit/play style follow the shelf tab — not "which rail you last used"
                                let is_img = if fav_stage {
                                    fav_tab == MediaMode::Image
                                } else {
                                    snap.media_mode == MediaMode::Image
                                };
                                let is_wall = is_img
                                    && snap.image_play_style == ImagePlayStyle::Wall;
                                let files: &[PathBuf] = if fav_stage {
                                    &fav_files_buf
                                } else {
                                    &snap.current_files
                                };
                                let n = if files.is_empty() {
                                    self.session.ui_count()
                                } else {
                                    files.len()
                                };
                                let (rows, cols) = crate::tiler::rows_cols(n.max(1));
                                let idle_preview = if fav_stage {
                                    !files.is_empty()
                                } else {
                                    snap.phase == SessionPhase::Idle
                                        && has_preview
                                        && !files.is_empty()
                                };
                                if idle_preview {
                                    self.preview_sel.retain(|&i| i < files.len());
                                }
                                let sel_n = if idle_preview {
                                    self.preview_sel.len()
                                } else {
                                    0
                                };
                                let unit = if is_img { "张" } else { "部" };
                                let title = if fav_stage {
                                    "收藏"
                                } else if is_img {
                                    "图片"
                                } else {
                                    "电影"
                                };

                                // Header (prototype top bar)
                                egui::Frame::NONE
                                    .inner_margin(egui::Margin {
                                        left: 24,
                                        right: 20,
                                        top: 16,
                                        bottom: 10,
                                    })
                                    .show(ui, |ui| {
                                        // One fixed-height row, all children vertically centered
                                        let header_h = 40.0_f32;
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(ui.available_width(), header_h),
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                            ui.set_min_height(header_h);
                                            ui.spacing_mut().item_spacing.x = 12.0;
                                            // Title: same row center as chips (not baseline-top)
                                            ui.label(
                                                RichText::new(title)
                                                    .size(22.0)
                                                    .color(INK)
                                                    .strong(),
                                            );
                                            if fav_stage {
                                                // Two shelves inside 收藏 — never silently swap the whole stage
                                                mode_chip(
                                                    ui,
                                                    &format!("电影 {fav_movie_n}"),
                                                    fav_tab == MediaMode::Movie,
                                                    || self.set_fav_tab(MediaMode::Movie),
                                                );
                                                mode_chip(
                                                    ui,
                                                    &format!("图片 {fav_image_n}"),
                                                    fav_tab == MediaMode::Image,
                                                    || self.set_fav_tab(MediaMode::Image),
                                                );
                                            }
                                            ui.label(
                                                RichText::new("数量").size(12.5).color(MUTED),
                                            );
                                            let bounds_mode = if fav_stage {
                                                fav_tab
                                            } else {
                                                snap.media_mode
                                            };
                                            let (cmin, cmax) = cfg.count_bounds_for(bounds_mode);
                                            let count = self.session.ui_count();
                                            count_stepper(
                                                ui,
                                                count,
                                                || {
                                                    self.session.set_ui_count(
                                                        count.saturating_sub(1).max(cmin),
                                                    );
                                                },
                                                || {
                                                    self.session
                                                        .set_ui_count((count + 1).min(cmax));
                                                },
                                            );
                                            status_pill(ui, snap.phase, &snap.message);
                                            let lib = if fav_stage {
                                                format!(
                                                    "电影 {} · 图片 {}",
                                                    fav_movie_n, fav_image_n
                                                )
                                            } else if snap.library_roots.is_empty() {
                                                "未设置片库".to_string()
                                            } else if snap.indexing {
                                                format!("索引中… {}", snap.indexing_found)
                                            } else {
                                                format!("{} · {} {}", 
                                                    if snap.library_roots.len() == 1 {
                                                        truncate_path(
                                                            &snap.library_roots[0],
                                                            10,
                                                        )
                                                    } else {
                                                        format!(
                                                            "{} 个库",
                                                            snap.library_roots.len()
                                                        )
                                                    },
                                                    snap.library_count,
                                                    unit
                                                )
                                            };
                                            ui.label(
                                                RichText::new(lib).size(13.0).color(MUTED),
                                            );

                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    ui.set_min_height(header_h);
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    // Icons first (RTL draws rightmost first)
                                                    if self.tray.is_some()
                                                        && icon_btn(
                                                            ui,
                                                            IconKind::Tray,
                                                            "托盘",
                                                        )
                                                        .clicked()
                                                    {
                                                        self.hide_to_tray(ctx);
                                                    }
                                                    if snap.media_mode == MediaMode::Movie
                                                        || snap.movie_in_background
                                                    {
                                                        let pin_forced =
                                                            snap.movie_in_background;
                                                        let pin_visual = pin_forced
                                                            || self.pin_while_playing;
                                                        if icon_btn_toggle(
                                                            ui,
                                                            IconKind::Pin,
                                                            "置顶",
                                                            pin_visual,
                                                        )
                                                        .clicked()
                                                        {
                                                            if pin_forced {
                                                                self.show_toast(
                                                                    "后台电影时需保持置顶",
                                                                );
                                                            } else {
                                                                self.pin_while_playing =
                                                                    !self.pin_while_playing;
                                                                self.session
                                                                    .set_pin_while_playing(
                                                                        self.pin_while_playing,
                                                                    );
                                                                if !self.pin_while_playing {
                                                                    self.apply_window_level(
                                                                        ctx, false,
                                                                    );
                                                                } else if self.playing_now()
                                                                    && !self.user_hid_to_tray
                                                                {
                                                                    crate::tray::force_show_and_pin(
                                                                    );
                                                                    self.pin_level_applied =
                                                                        false;
                                                                    self.apply_window_level(
                                                                        ctx, true,
                                                                    );
                                                                }
                                                            }
                                                        }
                                                    }
                                                    let rescan_resp = icon_btn(
                                                        ui,
                                                        IconKind::Rescan,
                                                        "检查更新",
                                                    );
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
                                                            RescanOutcome::AlreadyFresh {
                                                                count,
                                                            } => self.show_toast(format!(
                                                                "已是最新 · {count}"
                                                            )),
                                                            RescanOutcome::Started {
                                                                force: true,
                                                            } => self.show_toast("全量扫描…"),
                                                            RescanOutcome::Started {
                                                                force: false,
                                                            } => self.show_toast("更新中…"),
                                                            RescanOutcome::Busy => {
                                                                self.show_toast("索引中")
                                                            }
                                                            RescanOutcome::NoRoots => self
                                                                .show_toast(
                                                                    "请先添加片库",
                                                                ),
                                                        }
                                                    }
                                                    // Search pill (prototype top-right)
                                                    let hint = if is_img {
                                                        "搜索本轮图片…"
                                                    } else {
                                                        "搜索本轮片名…"
                                                    };
                                                    ui.add_space(6.0);
                                                    search_field(
                                                        ui,
                                                        &mut self.slate_query,
                                                        hint,
                                                        210.0,
                                                    );
                                                },
                                            );
                                            },
                                        );
                                    });

                                // Action bar (prototype: black 随机/开启 + 再来一批 + 避开最近 · 密度)
                                egui::Frame::NONE
                                    .inner_margin(egui::Margin {
                                        left: 24,
                                        right: 20,
                                        top: 2,
                                        bottom: 10,
                                    })
                                    .show(ui, |ui| {
                                        // 按钮不重复：
                                        // · 收藏台 → 从收藏随机预览（进浏览台）
                                        // · 无预览 → 随机预览
                                        // · 有预览 Idle → 开启 + 再来一批
                                        // · 播放中 → 关闭 + 再来一批（先停再抽，不开播）
                                        let fav_n = if fav_stage {
                                            files.len()
                                        } else {
                                            self.session.favorite_count()
                                        };
                                        let roll_ok = if fav_stage {
                                            !busy && fav_n > 0 && !playing
                                        } else {
                                            !busy && can_lib && !playing
                                        };
                                        let play_ok = !busy
                                            && !fav_stage
                                            && can_lib
                                            && has_preview
                                            && snap.phase == SessionPhase::Idle;
                                        let stop_ok = playing || starting;
                                        // 再来一批：有片单可换，或播放中要换一批
                                        let reroll_ok = !busy
                                            && !fav_stage
                                            && can_lib
                                            && (playing
                                                || (snap.phase == SessionPhase::Idle
                                                    && has_preview));

                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 8.0;

                                            if stop_ok {
                                                let stop_l = if starting {
                                                    "取消开启"
                                                } else if is_img {
                                                    if is_wall {
                                                        "关闭平铺"
                                                    } else {
                                                        "关闭幻灯"
                                                    }
                                                } else {
                                                    "关闭本轮"
                                                };
                                                if primary_btn_w(ui, 108.0, 36.0, stop_l, true)
                                                    .clicked()
                                                {
                                                    self.session.stop();
                                                    self.clear_slides();
                                                    if starting {
                                                        self.show_toast("已取消");
                                                    }
                                                }
                                                if secondary_btn(ui, 100.0, "再来一批", reroll_ok)
                                                    .clicked()
                                                {
                                                    self.session.reroll();
                                                    self.clear_preview_sel();
                                                    self.clear_slate_filter();
                                                    self.show_toast("已换一批");
                                                }
                                            } else if play_ok {
                                                let play_l = if is_img {
                                                    if is_wall {
                                                        "开启平铺墙"
                                                    } else {
                                                        "开启幻灯"
                                                    }
                                                } else if snap.pot_available {
                                                    "开启播放"
                                                } else {
                                                    "系统打开"
                                                };
                                                if primary_btn_w(ui, 108.0, 36.0, play_l, true)
                                                    .clicked()
                                                {
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
                                                            "无 PotPlayer · 系统默认打开",
                                                        );
                                                    }
                                                }
                                                if secondary_btn(ui, 100.0, "再来一批", reroll_ok)
                                                    .clicked()
                                                {
                                                    self.session.reroll();
                                                    self.clear_preview_sel();
                                                    self.clear_slate_filter();
                                                    self.show_toast("已换一批");
                                                }
                                            } else if fav_stage {
                                                if primary_btn_w(
                                                    ui,
                                                    128.0,
                                                    36.0,
                                                    "随机预览",
                                                    roll_ok,
                                                )
                                                .clicked()
                                                {
                                                    match self
                                                        .session
                                                        .roll_preview_from_favorites_for(fav_tab)
                                                    {
                                                        Ok(n) => {
                                                            self.stage_view = StageView::Browse;
                                                            self.clear_preview_sel();
                                                            self.clear_slate_filter();
                                                            self.show_toast(format!(
                                                                "从{}收藏抽出 {n} {unit}",
                                                                if fav_tab == MediaMode::Image {
                                                                    "图片"
                                                                } else {
                                                                    "电影"
                                                                }
                                                            ));
                                                        }
                                                        Err(e) => self.show_toast(e),
                                                    }
                                                }
                                                if secondary_btn(
                                                    ui,
                                                    100.0,
                                                    "全部播放",
                                                    !busy && has_preview && sel_n == 0,
                                                )
                                                .clicked()
                                                {
                                                    if self.session.media_mode() != fav_tab {
                                                        self.session.set_media_mode(fav_tab);
                                                    }
                                                    let paths = files.to_vec();
                                                    match self.session.start_paths(paths) {
                                                        Ok(n) => {
                                                            self.clear_preview_sel();
                                                            if is_img {
                                                                self.slide_index = 0;
                                                                self.slide_elapsed = 0.0;
                                                                self.slide_paused = false;
                                                                self.clear_slides();
                                                            }
                                                            self.show_toast(format!(
                                                                "播放收藏 {n} {unit}…"
                                                            ));
                                                        }
                                                        Err(e) => self.show_toast(e),
                                                    }
                                                }
                                            } else if primary_btn_w(
                                                ui,
                                                118.0,
                                                36.0,
                                                "随机预览",
                                                roll_ok,
                                            )
                                            .clicked()
                                            {
                                                self.session.roll_preview();
                                                self.clear_preview_sel();
                                                self.clear_slate_filter();
                                                self.show_toast("已生成预览");
                                            }

                                            ui.label(
                                                RichText::new("避开最近")
                                                    .size(12.5)
                                                    .color(MUTED),
                                            );
                                            let mut avoid = cfg.avoid_recent;
                                            if toggle(ui, &mut avoid) {
                                                self.session.set_avoid_recent(avoid);
                                            }

                                            if snap.movie_in_background {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "后台 {} 部",
                                                        snap.movie_background_count
                                                    ))
                                                    .size(12.0)
                                                    .color(MUTED),
                                                );
                                                if mini_text_btn(ui, "关掉").clicked() {
                                                    self.session.stop_background_movie();
                                                    self.show_toast("已关闭后台电影");
                                                }
                                            }
                                            if !snap.last_errors.is_empty() {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "{} 项异常",
                                                        snap.last_errors.len()
                                                    ))
                                                    .size(12.0)
                                                    .color(Color32::from_rgb(0xB4, 0x53, 0x09)),
                                                );
                                            }

                                            // Density slider — persisted in config.json
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    ui.spacing_mut().item_spacing.x = 8.0;
                                                    let mut cols_f = self.card_cols as f32;
                                                    let resp = ui.add(
                                                        egui::Slider::new(&mut cols_f, 2.0..=5.0)
                                                            .show_value(false)
                                                            .clamping(egui::SliderClamping::Always),
                                                    );
                                                    // Live preview while dragging; write config once on commit
                                                    self.card_cols =
                                                        (cols_f.round() as u8).clamp(2, 5);
                                                    if resp.drag_stopped()
                                                        || (resp.changed() && !resp.dragged())
                                                    {
                                                        self.session.set_card_cols(self.card_cols);
                                                    }
                                                    resp.on_hover_text("卡片列数（下次启动记住）");
                                                    ui.label(
                                                        RichText::new("列")
                                                            .size(11.0)
                                                            .color(FAINT),
                                                    );
                                                },
                                            );
                                        });
                                    });

                                // Meta line
                                egui::Frame::NONE
                                    .inner_margin(egui::Margin {
                                        left: 20,
                                        right: 16,
                                        top: 0,
                                        bottom: 6,
                                    })
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let meta = if playing {
                                                format!("播放中 · {n} {unit}")
                                            } else if fav_stage {
                                                if files.is_empty() {
                                                    format!(
                                                        "本架暂无{} · 在对应模式预览右键可收藏",
                                                        if fav_tab == MediaMode::Image {
                                                            "图片"
                                                        } else {
                                                            "电影"
                                                        }
                                                    )
                                                } else {
                                                    format!(
                                                        "{}架 {} {} · 单击多选",
                                                        if fav_tab == MediaMode::Image {
                                                            "图片"
                                                        } else {
                                                            "电影"
                                                        },
                                                        n,
                                                        unit
                                                    )
                                                }
                                            } else if has_preview {
                                                format!("待预览 · {n} {unit}")
                                            } else if snap.library_roots.is_empty() {
                                                "未添加片库".into()
                                            } else if snap.indexing {
                                                "索引中…".into()
                                            } else if !can_lib {
                                                "片库为空".into()
                                            } else {
                                                format!(
                                                    "待预览 · 将抽 {} {unit}",
                                                    self.session.ui_count()
                                                )
                                            };
                                            ui.label(
                                                RichText::new(meta).size(12.5).color(FAINT),
                                            );
                                            if idle_preview && sel_n == 0 {
                                                ui.with_layout(
                                                    Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        if mini_text_btn(ui, "全选").clicked()
                                                        {
                                                            self.preview_sel =
                                                                (0..files.len()).collect();
                                                        }
                                                    },
                                                );
                                            }
                                        });
                                        if idle_preview && sel_n > 0 {
                                            ui.add_space(4.0);
                                            if let Some(act) =
                                                selection_bar_ex(ui, sel_n, fav_stage)
                                            {
                                                let idxs: Vec<usize> = self
                                                    .preview_sel
                                                    .iter()
                                                    .copied()
                                                    .collect();
                                                match act {
                                                    SelectionBarAction::Play => {
                                                        if fav_stage {
                                                            if self.session.media_mode() != fav_tab
                                                            {
                                                                self.session
                                                                    .set_media_mode(fav_tab);
                                                            }
                                                            let paths: Vec<PathBuf> = idxs
                                                                .iter()
                                                                .filter_map(|&i| {
                                                                    files.get(i).cloned()
                                                                })
                                                                .collect();
                                                            match self.session.start_paths(paths)
                                                            {
                                                                Ok(n) => {
                                                                    self.clear_preview_sel();
                                                                    if is_img {
                                                                        self.slide_index = 0;
                                                                        self.slide_elapsed = 0.0;
                                                                        self.slide_paused = false;
                                                                        self.clear_slides();
                                                                    }
                                                                    self.show_toast(if n == 1 {
                                                                        "单部播放…".into()
                                                                    } else {
                                                                        format!(
                                                                            "播放所选 {n} {unit}…"
                                                                        )
                                                                    });
                                                                }
                                                                Err(e) => self.show_toast(e),
                                                            }
                                                        } else {
                                                            match self
                                                                .session
                                                                .start_selected(&idxs)
                                                            {
                                                                Ok(n) => {
                                                                    self.clear_preview_sel();
                                                                    if is_img {
                                                                        self.slide_index = 0;
                                                                        self.slide_elapsed = 0.0;
                                                                        self.slide_paused = false;
                                                                        self.clear_slides();
                                                                    }
                                                                    self.show_toast(if n == 1 {
                                                                        "单部播放…".into()
                                                                    } else {
                                                                        format!(
                                                                            "播放所选 {n} 部…"
                                                                        )
                                                                    });
                                                                }
                                                                Err(e) => self.show_toast(e),
                                                            }
                                                        }
                                                    }
                                                    SelectionBarAction::Replace => {
                                                        match self
                                                            .session
                                                            .replace_preview_items(&idxs)
                                                        {
                                                            Ok(n) => {
                                                                self.clear_preview_sel();
                                                                self.show_toast(format!(
                                                                    "已换片 {n}"
                                                                ));
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
                                                        self.show_toast(format!(
                                                            "已移出本轮 {n}"
                                                        ));
                                                    }
                                                    SelectionBarAction::Favorite => {
                                                        let n = self
                                                            .session
                                                            .favorite_preview_items(&idxs);
                                                        self.clear_preview_sel();
                                                        self.show_toast(if n == 0 {
                                                            "所选已在收藏中".into()
                                                        } else {
                                                            format!("已收藏 {n}")
                                                        });
                                                    }
                                                    SelectionBarAction::Blacklist => {
                                                        let n = self
                                                            .session
                                                            .blacklist_preview_items(&idxs);
                                                        self.clear_preview_sel();
                                                        self.show_toast(format!(
                                                            "已不再抽到 {n}"
                                                        ));
                                                    }
                                                    SelectionBarAction::Unfavorite => {
                                                        let paths: Vec<PathBuf> = idxs
                                                            .iter()
                                                            .filter_map(|&i| {
                                                                files.get(i).cloned()
                                                            })
                                                            .collect();
                                                        let n = self
                                                            .session
                                                            .unfavorite_paths_for(fav_tab, &paths);
                                                        self.clear_preview_sel();
                                                        self.show_toast(format!(
                                                            "已移出收藏 {n}"
                                                        ));
                                                    }
                                                    SelectionBarAction::Clear => {
                                                        self.clear_preview_sel();
                                                    }
                                                }
                                            }
                                        }
                                        if playing && !snap.items.is_empty() {
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new("本轮")
                                                        .size(12.0)
                                                        .color(MUTED),
                                                );
                                                if snap.media_mode == MediaMode::Movie
                                                    && mini_text_btn(ui, "重新平铺").clicked()
                                                {
                                                    self.session.retile_now();
                                                    self.show_toast("重新平铺…");
                                                }
                                            });
                                            let list_h = 72.0_f32;
                                            egui::ScrollArea::vertical()
                                                .id_salt("playing_items")
                                                .max_height(list_h)
                                                .show(ui, |ui| {
                                                    let mut close_idx = None;
                                                    let mut focus_idx = None;
                                                    let mut solo_idx = None;
                                                    for it in &snap.items {
                                                        let t = file_title(&it.name);
                                                        sidebar_list_row(
                                                            ui,
                                                            it.index + 1,
                                                            &t,
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
                                    });

                                // Visible indices (search filters current slate only)
                                let q = self.slate_query.trim().to_ascii_lowercase();
                                let shown: Vec<usize> = if files.is_empty() {
                                    Vec::new()
                                } else if q.is_empty() {
                                    (0..files.len()).collect()
                                } else {
                                    files
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, p)| {
                                            let name = p
                                                .file_name()
                                                .map(|n| n.to_string_lossy().to_string())
                                                .unwrap_or_default();
                                            file_title(&name)
                                                .to_ascii_lowercase()
                                                .contains(&q)
                                        })
                                        .map(|(i, _)| i)
                                        .collect()
                                };
                                let show_n = if files.is_empty() {
                                    // empty chrome: keep count-shaped placeholders
                                    n.max(1)
                                } else {
                                    shown.len().max(1)
                                };
                                // Density from slider (prefer cols), not only tiler
                                let prefer = (self.card_cols as usize).clamp(2, 5);
                                let cols_v = prefer.min(show_n.max(1));
                                let rows_v = (show_n + cols_v - 1) / cols_v;
                                let _ = (rows, cols); // tiler dims unused when density set

                                // Card grid fills remaining space above a reserved footer strip
                                // (footer ~44px content + margins; keep grid from shoving it off-screen)
                                let footer_h = 52.0;
                                let slate_h =
                                    (ui.available_height() - footer_h).max(80.0);
                                let slate_w = ui.available_width().max(1.0);
                                let gap = 14.0;
                                let pad = 20.0;

                                let mut card_play: Option<usize> = None;
                                let mut card_replace: Option<usize> = None;
                                let mut card_remove: Option<usize> = None;
                                let mut card_ban: Option<usize> = None;
                                let mut card_fav: Option<usize> = None;
                                let mut card_unfav: Option<usize> = None;
                                // One lock: star marks on browse cards
                                let fav_keys: HashSet<String> = self
                                    .session
                                    .favorite_paths()
                                    .iter()
                                    .map(|p| p.to_string_lossy().to_ascii_lowercase())
                                    .collect();

                                egui::Frame::NONE
                                    .fill(BG_MAIN)
                                    .inner_margin(egui::Margin {
                                        left: pad as i8,
                                        right: pad as i8,
                                        top: 4,
                                        bottom: 4,
                                    })
                                    .show(ui, |ui| {
                                        let avail_w = (slate_w - pad * 2.0).max(40.0);
                                        let avail_h = (slate_h - 8.0).max(40.0);
                                        // Cap only — never force min taller than reserved slot
                                        // (was pushing footer out of the window / clipping 刷新)
                                        ui.set_max_size(Vec2::new(avail_w, avail_h));
                                        ui.set_min_width(avail_w);
                                        ui.set_min_height(avail_h.min(
                                            ui.available_height().max(40.0),
                                        ));

                                        if fav_stage && files.is_empty() {
                                            let shelf = if fav_tab == MediaMode::Image {
                                                "图片"
                                            } else {
                                                "电影"
                                            };
                                            ui.centered_and_justified(|ui| {
                                                ui.vertical_centered(|ui| {
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "「{shelf}」架还是空的"
                                                        ))
                                                        .size(16.0)
                                                        .color(MUTED),
                                                    );
                                                    ui.add_space(6.0);
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "在{shelf}模式预览里右键「收藏」；上方可切换电影 / 图片架"
                                                        ))
                                                        .size(13.0)
                                                        .color(FAINT),
                                                    );
                                                });
                                            });
                                        } else if !files.is_empty() && shown.is_empty() {
                                            ui.centered_and_justified(|ui| {
                                                ui.label(
                                                    RichText::new("无匹配片名")
                                                        .size(14.0)
                                                        .color(FAINT),
                                                );
                                            });
                                        } else {
                                            let cell_w = ((avail_w
                                                - gap * (cols_v.saturating_sub(1) as f32))
                                                / cols_v as f32)
                                                .max(48.0);
                                            // Fill reserved slate height — do not cap short and leave
                                            // a dead empty band above the footer (user 右下角空白).
                                            let cell_h = ((avail_h
                                                - gap * (rows_v.saturating_sub(1) as f32))
                                                / rows_v as f32)
                                                .max(72.0);

                                            for r in 0..rows_v {
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = gap;
                                                    for c in 0..cols_v {
                                                        let slot = r * cols_v + c;
                                                        if files.is_empty() {
                                                            if slot >= n {
                                                                continue;
                                                            }
                                                            let (rect, _) =
                                                                ui.allocate_exact_size(
                                                                    Vec2::new(cell_w, cell_h),
                                                                    Sense::hover(),
                                                                );
                                                            ui.painter().rect_filled(
                                                                rect, 12.0, BG_SOFT,
                                                            );
                                                            continue;
                                                        }
                                                        if slot >= shown.len() {
                                                            ui.allocate_exact_size(
                                                                Vec2::new(cell_w, cell_h),
                                                                Sense::hover(),
                                                            );
                                                            continue;
                                                        }
                                                        let idx = shown[slot];
                                                        let path_opt = files.get(idx);
                                                        let label = path_opt
                                                            .and_then(|p| {
                                                                p.file_name().map(|n| {
                                                                    n.to_string_lossy()
                                                                        .to_string()
                                                                })
                                                            })
                                                            .unwrap_or_default();
                                                        let folder = path_opt
                                                            .and_then(|p| p.parent())
                                                            .and_then(|p| p.file_name())
                                                            .map(|n| {
                                                                n.to_string_lossy().to_string()
                                                            });
                                                        let badge = format!("{:02}", idx + 1);
                                                        let tex = path_opt.and_then(|p| {
                                                            self.textures.get(
                                                                &p.to_string_lossy()
                                                                    .to_string(),
                                                            )
                                                        });
                                                        let selected = idle_preview
                                                            && self.preview_sel.contains(&idx);
                                                        let favorited = fav_stage
                                                            || path_opt
                                                                .map(|p| {
                                                                    fav_keys.contains(
                                                                        &p.to_string_lossy()
                                                                            .to_ascii_lowercase(),
                                                                    )
                                                                })
                                                                .unwrap_or(false);
                                                        let cell = preview_card(
                                                            ui,
                                                            cell_w,
                                                            cell_h,
                                                            &label,
                                                            folder.as_deref(),
                                                            Some(&badge),
                                                            tex,
                                                            selected,
                                                            idle_preview,
                                                            favorited,
                                                        );
                                                        if idle_preview {
                                                            cell.context_menu(|ui| {
                                                                if ui.button("播放").clicked()
                                                                {
                                                                    card_play = Some(idx);
                                                                    ui.close_menu();
                                                                }
                                                                if fav_stage {
                                                                    if ui
                                                                        .button("移出收藏")
                                                                        .clicked()
                                                                    {
                                                                        card_unfav = Some(idx);
                                                                        ui.close_menu();
                                                                    }
                                                                } else {
                                                                    if ui.button("换片").clicked()
                                                                    {
                                                                        card_replace = Some(idx);
                                                                        ui.close_menu();
                                                                    }
                                                                    if ui.button("移出本轮").clicked()
                                                                    {
                                                                        card_remove = Some(idx);
                                                                        ui.close_menu();
                                                                    }
                                                                    let fav_l = if favorited {
                                                                        "移出收藏"
                                                                    } else {
                                                                        "收藏"
                                                                    };
                                                                    if ui.button(fav_l).clicked() {
                                                                        if favorited {
                                                                            card_unfav = Some(idx);
                                                                        } else {
                                                                            card_fav = Some(idx);
                                                                        }
                                                                        ui.close_menu();
                                                                    }
                                                                    if ui
                                                                        .button("不再抽到")
                                                                        .clicked()
                                                                    {
                                                                        card_ban = Some(idx);
                                                                        ui.close_menu();
                                                                    }
                                                                }
                                                            });
                                                        }
                                                        if idle_preview && cell.double_clicked()
                                                        {
                                                            card_play = Some(idx);
                                                        } else if idle_preview && cell.clicked()
                                                        {
                                                            if self.preview_sel.contains(&idx)
                                                            {
                                                                self.preview_sel.remove(&idx);
                                                            } else {
                                                                self.preview_sel.insert(idx);
                                                            }
                                                        }
                                                    }
                                                });
                                                if r + 1 < rows_v {
                                                    ui.add_space(gap);
                                                }
                                            }
                                        }
                                    });

                                if let Some(i) = card_play {
                                    if fav_stage {
                                        if self.session.media_mode() != fav_tab {
                                            self.session.set_media_mode(fav_tab);
                                        }
                                        if let Some(p) = files.get(i).cloned() {
                                            match self.session.start_paths(vec![p]) {
                                                Ok(_) => {
                                                    self.clear_preview_sel();
                                                    if is_img {
                                                        self.slide_index = 0;
                                                        self.slide_elapsed = 0.0;
                                                        self.slide_paused = false;
                                                        self.clear_slides();
                                                    }
                                                    self.show_toast("单部播放…");
                                                }
                                                Err(e) => self.show_toast(e),
                                            }
                                        }
                                    } else {
                                        match self.session.start_selected(&[i]) {
                                            Ok(_) => {
                                                self.clear_preview_sel();
                                                if is_img {
                                                    self.slide_index = 0;
                                                    self.slide_elapsed = 0.0;
                                                    self.slide_paused = false;
                                                    self.clear_slides();
                                                }
                                                self.show_toast("单部播放…");
                                            }
                                            Err(e) => self.show_toast(e),
                                        }
                                    }
                                } else if let Some(i) = card_replace {
                                    match self.session.replace_preview_item(i) {
                                        Ok(_) => self.show_toast("已换片"),
                                        Err(e) => self.show_toast(e),
                                    }
                                } else if let Some(i) = card_remove {
                                    self.session.remove_preview_item(i);
                                    self.preview_sel.remove(&i);
                                    self.show_toast("已移出本轮");
                                } else if let Some(i) = card_fav {
                                    if let Some(p) = files.get(i).cloned() {
                                        if self.session.toggle_favorite(&p) {
                                            self.show_toast("已收藏");
                                        } else {
                                            self.show_toast("已移出收藏");
                                        }
                                    }
                                } else if let Some(i) = card_unfav {
                                    if let Some(p) = files.get(i).cloned() {
                                        if fav_stage {
                                            self.session.unfavorite_path_for(fav_tab, &p);
                                        } else {
                                            self.session.unfavorite_path(&p);
                                        }
                                        self.preview_sel.remove(&i);
                                        self.show_toast("已移出收藏");
                                    }
                                } else if let Some(i) = card_ban {
                                    if self.session.blacklist_preview_item(i).is_some() {
                                        self.preview_sel.remove(&i);
                                        self.show_toast("已不再抽到");
                                    }
                                }

                                // Footer: left texts grouped · 刷新 flush right (no empty thirds)
                                let shown_n = if files.is_empty() {
                                    0
                                } else {
                                    shown.len()
                                };
                                let mid = if fav_stage {
                                    let shelf = if fav_tab == MediaMode::Image {
                                        "图片架"
                                    } else {
                                        "电影架"
                                    };
                                    if files.is_empty() {
                                        format!("{shelf}为空")
                                    } else if q.is_empty() {
                                        format!("{shelf} {} {}", files.len(), unit)
                                    } else {
                                        format!(
                                            "筛选 {} / {} {}",
                                            shown_n,
                                            files.len(),
                                            unit
                                        )
                                    }
                                } else if has_preview {
                                    if q.is_empty() {
                                        format!("当前显示 {} {}", files.len(), unit)
                                    } else {
                                        format!(
                                            "筛选 {} / {} {}",
                                            shown_n,
                                            files.len(),
                                            unit
                                        )
                                    }
                                } else {
                                    format!("将抽 {} {}", self.session.ui_count(), unit)
                                };
                                let left = if fav_stage {
                                    format!("电影 {} · 图片 {}", fav_movie_n, fav_image_n)
                                } else {
                                    format!("共 {} {}", snap.library_count, unit)
                                };

                                egui::Frame::NONE
                                    .fill(RAIL)
                                    .inner_margin(egui::Margin::symmetric(20, 10))
                                    .show(ui, |ui| {
                                        ui.with_layout(
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                ui.set_min_height(28.0);
                                                ui.spacing_mut().item_spacing.x = 10.0;
                                                ui.label(
                                                    RichText::new(left)
                                                        .size(12.0)
                                                        .color(MUTED),
                                                );
                                                ui.label(
                                                    RichText::new("·").size(12.0).color(FAINT),
                                                );
                                                ui.label(
                                                    RichText::new(mid)
                                                        .size(12.0)
                                                        .color(MUTED),
                                                );
                                                ui.with_layout(
                                                    Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        if mini_text_btn(ui, "刷新").clicked()
                                                        {
                                                            let _ = self.session.rescan();
                                                            self.show_toast("检查更新…");
                                                        }
                                                    },
                                                );
                                            },
                                        );
                                    });
                            },
                        );
                    },
                );
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


