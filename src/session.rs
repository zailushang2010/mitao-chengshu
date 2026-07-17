use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::{Config, ImagePlayStyle, MediaMode};
use crate::history::History;
use crate::library::Library;
use crate::picker;
use crate::potplayer::{self, LaunchedItem};
use crate::tiler;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    Idle,
    Starting,
    Playing,
    Stopping,
}

#[derive(Debug, Clone)]
pub struct SessionItemView {
    pub index: usize,
    pub path: PathBuf,
    pub pid: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub phase: SessionPhase,
    pub message: String,
    /// Paths shown in grid: preview list, or playing list
    pub current_files: Vec<PathBuf>,
    /// True when a preview is ready but not playing yet
    pub has_preview: bool,
    pub items: Vec<SessionItemView>,
    pub library_count: usize,
    #[allow(dead_code)]
    pub library_root: String,
    pub library_roots: Vec<String>,
    pub last_errors: Vec<String>,
    pub media_mode: MediaMode,
    pub slideshow_interval_secs: u8,
    pub image_play_style: ImagePlayStyle,
    /// Background library scan in progress for the active mode.
    pub indexing: bool,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            phase: SessionPhase::Idle,
            message: "就绪".into(),
            current_files: Vec::new(),
            has_preview: false,
            items: Vec::new(),
            library_count: 0,
            library_root: String::new(),
            library_roots: Vec::new(),
            last_errors: Vec::new(),
            media_mode: MediaMode::Movie,
            slideshow_interval_secs: 5,
            image_play_style: ImagePlayStyle::Slideshow,
            indexing: false,
        }
    }
}

struct Inner {
    config: Config,
    /// Per-mode library cache so 电影↔图片 switch does not re-walk disks.
    movie_library: Library,
    image_library: Library,
    movie_indexed: bool,
    image_indexed: bool,
    movie_scan_epoch: u64,
    image_scan_epoch: u64,
    movie_scan_busy: bool,
    image_scan_busy: bool,
    movie_history: History,
    image_history: History,
    phase: SessionPhase,
    /// Selected for play (not launched yet)
    preview_files: Vec<PathBuf>,
    items: Vec<LaunchedItem>,
    message: String,
    last_errors: Vec<String>,
    ui_count: usize,
}

impl Inner {
    fn library(&self) -> &Library {
        match self.config.media_mode {
            MediaMode::Movie => &self.movie_library,
            MediaMode::Image => &self.image_library,
        }
    }

    fn history(&self) -> &History {
        match self.config.media_mode {
            MediaMode::Movie => &self.movie_history,
            MediaMode::Image => &self.image_history,
        }
    }

    fn history_mut(&mut self) -> &mut History {
        match self.config.media_mode {
            MediaMode::Movie => &mut self.movie_history,
            MediaMode::Image => &mut self.image_history,
        }
    }

    fn is_indexed(&self, mode: MediaMode) -> bool {
        match mode {
            MediaMode::Movie => self.movie_indexed,
            MediaMode::Image => self.image_indexed,
        }
    }

    fn is_scan_busy(&self, mode: MediaMode) -> bool {
        match mode {
            MediaMode::Movie => self.movie_scan_busy,
            MediaMode::Image => self.image_scan_busy,
        }
    }

    fn set_scan_busy(&mut self, mode: MediaMode, busy: bool) {
        match mode {
            MediaMode::Movie => self.movie_scan_busy = busy,
            MediaMode::Image => self.image_scan_busy = busy,
        }
    }

    fn set_library(&mut self, mode: MediaMode, lib: Library) {
        match mode {
            MediaMode::Movie => {
                self.movie_library = lib;
                self.movie_indexed = true;
                self.movie_scan_busy = false;
            }
            MediaMode::Image => {
                self.image_library = lib;
                self.image_indexed = true;
                self.image_scan_busy = false;
            }
        }
    }

    fn invalidate_mode(&mut self, mode: MediaMode) {
        match mode {
            MediaMode::Movie => {
                self.movie_library = Library::empty();
                self.movie_indexed = false;
            }
            MediaMode::Image => {
                self.image_library = Library::empty();
                self.image_indexed = false;
            }
        }
    }

    fn bump_scan_epoch(&mut self, mode: MediaMode) -> u64 {
        match mode {
            MediaMode::Movie => {
                self.movie_scan_epoch = self.movie_scan_epoch.wrapping_add(1);
                self.movie_scan_epoch
            }
            MediaMode::Image => {
                self.image_scan_epoch = self.image_scan_epoch.wrapping_add(1);
                self.image_scan_epoch
            }
        }
    }

    fn scan_epoch(&self, mode: MediaMode) -> u64 {
        match mode {
            MediaMode::Movie => self.movie_scan_epoch,
            MediaMode::Image => self.image_scan_epoch,
        }
    }
}

pub struct SessionHandle {
    inner: Arc<Mutex<Inner>>,
}

impl SessionHandle {
    pub fn new(config: Config) -> Self {
        let mode = config.media_mode;
        let ui_count = config.default_count_for(mode);
        // Load both histories once; only scan the active mode at boot (async via rescan).
        let movie_history = History::load();
        let image_history = History::load_images();
        let mut inner = Inner {
            config,
            movie_library: Library::empty(),
            image_library: Library::empty(),
            movie_indexed: false,
            image_indexed: false,
            movie_scan_epoch: 0,
            image_scan_epoch: 0,
            movie_scan_busy: false,
            image_scan_busy: false,
            movie_history,
            image_history,
            phase: SessionPhase::Idle,
            preview_files: Vec::new(),
            items: Vec::new(),
            message: "就绪 · 先「随机预览」再「开启播放」".into(),
            last_errors: Vec::new(),
            ui_count,
        };
        // Initial index for active mode only (keeps other mode for first switch).
        let roots = inner.config.roots_for(mode);
        if roots.is_empty() {
            inner.set_library(mode, Library::empty());
            inner.message = idle_message(&inner);
        } else {
            inner.message = "索引中…".into();
        }
        let handle = Self {
            inner: Arc::new(Mutex::new(inner)),
        };
        if !roots.is_empty() {
            handle.begin_scan(mode);
        }
        handle
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        let g = self.inner.lock().unwrap();
        let items: Vec<SessionItemView> = g
            .items
            .iter()
            .enumerate()
            .map(|(index, i)| SessionItemView {
                index,
                path: i.path.clone(),
                pid: i.pid,
                name: i
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| i.path.display().to_string()),
            })
            .collect();
        let current_files = if g.phase == SessionPhase::Playing
            || g.phase == SessionPhase::Starting
            || g.phase == SessionPhase::Stopping
        {
            if items.is_empty() {
                g.preview_files.clone()
            } else {
                items.iter().map(|i| i.path.clone()).collect()
            }
        } else {
            g.preview_files.clone()
        };
        let mode = g.config.media_mode;
        SessionSnapshot {
            phase: g.phase,
            message: g.message.clone(),
            current_files,
            has_preview: !g.preview_files.is_empty(),
            items,
            library_count: g.library().len(),
            library_root: g.config.library_label_for(mode),
            library_roots: g.config.roots_for(mode),
            last_errors: g.last_errors.clone(),
            media_mode: mode,
            slideshow_interval_secs: g.config.slideshow_interval_secs,
            image_play_style: g.config.image_play_style,
            indexing: g.is_scan_busy(mode),
        }
    }

    pub fn media_mode(&self) -> MediaMode {
        self.inner.lock().unwrap().config.media_mode
    }

    pub fn set_media_mode(&self, mode: MediaMode) {
        let (pids, need_scan, cfg_to_save) = {
            let mut g = self.inner.lock().unwrap();
            if g.config.media_mode == mode {
                return;
            }
            let pids: Vec<u32> = if g.phase == SessionPhase::Playing
                || g.phase == SessionPhase::Starting
            {
                let p: Vec<u32> = g
                    .items
                    .iter()
                    .map(|i| i.pid)
                    .filter(|p| *p != 0)
                    .collect();
                g.items.clear();
                g.phase = SessionPhase::Idle;
                p
            } else {
                Vec::new()
            };
            g.config.media_mode = mode;
            // Each mode remembers its own pick count.
            g.ui_count = g.config.default_count_for(mode);
            g.preview_files.clear();
            g.items.clear();
            g.phase = SessionPhase::Idle;
            let cfg_to_save = g.config.clone();
            let need_scan = !g.is_indexed(mode) && !g.is_scan_busy(mode);
            if g.is_scan_busy(mode) {
                g.message = "索引中…".into();
            } else if g.is_indexed(mode) {
                g.message = idle_message(&g);
            } else {
                g.message = "索引中…".into();
            }
            (pids, need_scan, cfg_to_save)
        };
        // Disk I/O off the UI thread
        thread::spawn(move || {
            let _ = crate::config::save(&cfg_to_save);
        });
        if !pids.is_empty() {
            thread::spawn(move || {
                potplayer::kill_pids(&pids);
            });
        }
        if need_scan {
            self.begin_scan(mode);
        }
    }

    /// Background scan for a media mode; results fill that mode's cache.
    fn begin_scan(&self, mode: MediaMode) {
        let (roots, exts, epoch) = {
            let mut g = self.inner.lock().unwrap();
            let roots = g.config.roots_for(mode);
            let exts = g.config.extensions_for(mode).to_vec();
            if roots.is_empty() {
                g.set_library(mode, Library::empty());
                if g.config.media_mode == mode && g.phase == SessionPhase::Idle {
                    g.message = idle_message(&g);
                }
                return;
            }
            let epoch = g.bump_scan_epoch(mode);
            g.set_scan_busy(mode, true);
            if g.config.media_mode == mode {
                g.message = "索引中…".into();
            }
            (roots, exts, epoch)
        };

        let handle = self.inner.clone();
        thread::spawn(move || {
            let lib = scan_config_roots(&roots, &exts);
            let mut g = handle.lock().unwrap();
            if g.scan_epoch(mode) != epoch {
                return; // superseded by a newer scan for this mode
            }
            g.set_library(mode, lib);
            if g.config.media_mode == mode && g.phase == SessionPhase::Idle {
                g.message = idle_message(&g);
            }
        });
    }

    pub fn set_slideshow_interval(&self, secs: u8) {
        let mut g = self.inner.lock().unwrap();
        g.config.slideshow_interval_secs = secs.clamp(1, 60);
        let _ = crate::config::save(&g.config);
    }

    pub fn set_image_play_style(&self, style: ImagePlayStyle) {
        let mut g = self.inner.lock().unwrap();
        g.config.image_play_style = style;
        let _ = crate::config::save(&g.config);
    }

    pub fn config_clone(&self) -> Config {
        self.inner.lock().unwrap().config.clone()
    }

    pub fn ui_count(&self) -> usize {
        self.inner.lock().unwrap().ui_count
    }

    pub fn set_ui_count(&self, n: usize) {
        let mut g = self.inner.lock().unwrap();
        let mode = g.config.media_mode;
        let n = g.config.clamp_count_for(mode, n);
        g.ui_count = n;
        g.config.set_default_count_for(mode, n);
        let _ = crate::config::save(&g.config);
    }

    pub fn set_count_bounds(&self, min: usize, max: usize) {
        let mut g = self.inner.lock().unwrap();
        let mode = g.config.media_mode;
        g.config.set_count_min_for(mode, min);
        g.config.set_count_max_for(mode, max);
        let n = g.config.clamp_count_for(mode, g.ui_count);
        g.ui_count = n;
        g.config.set_default_count_for(mode, n);
        let _ = crate::config::save(&g.config);
    }

    /// Bring one playing item to the front.
    pub fn focus_item(&self, index: usize) {
        let pid = {
            let g = self.inner.lock().unwrap();
            g.items.get(index).map(|i| i.pid)
        };
        if let Some(pid) = pid {
            potplayer::focus_pid(pid);
        }
    }

    /// Close all others and maximize this one (独播).
    pub fn solo_item(&self, index: usize) {
        let (keep, kill) = {
            let g = self.inner.lock().unwrap();
            if index >= g.items.len() {
                return;
            }
            let keep = g.items[index].clone();
            let kill: Vec<u32> = g
                .items
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, it)| it.pid)
                .collect();
            (keep, kill)
        };
        if !kill.is_empty() {
            potplayer::kill_pids(&kill);
        }
        {
            let mut g = self.inner.lock().unwrap();
            g.items = vec![keep.clone()];
            g.phase = SessionPhase::Playing;
            g.message = format!("独播 · {}", file_stem(&keep.path));
        }
        potplayer::maximize_pid(keep.pid);
        potplayer::focus_pid(keep.pid);
    }

    /// Close one playing item; if none left → Idle.
    pub fn close_item(&self, index: usize) {
        let pid = {
            let mut g = self.inner.lock().unwrap();
            if index >= g.items.len() {
                return;
            }
            let item = g.items.remove(index);
            if g.items.is_empty() {
                g.phase = SessionPhase::Idle;
                g.message = format!("就绪 · 已索引 {} 部", g.library().len());
            } else {
                g.message = format!("播放中 · {} 部", g.items.len());
            }
            item.pid
        };
        let _ = potplayer::kill_pid(pid);
    }

    pub fn set_volume(&self, v: u8) {
        let mut g = self.inner.lock().unwrap();
        g.config.volume_percent = v.min(100);
        let _ = crate::config::save(&g.config);
    }

    pub fn set_avoid_recent(&self, v: bool) {
        let mut g = self.inner.lock().unwrap();
        g.config.avoid_recent = v;
        let _ = crate::config::save(&g.config);
    }

    pub fn set_close_on_exit(&self, v: bool) {
        let mut g = self.inner.lock().unwrap();
        g.config.close_session_on_exit = v;
        let _ = crate::config::save(&g.config);
    }

    pub fn set_minimize_to_tray(&self, v: bool) {
        let mut g = self.inner.lock().unwrap();
        g.config.minimize_to_tray = v;
        let _ = crate::config::save(&g.config);
    }

    /// Replace all roots with a single path (legacy helper).
    #[allow(dead_code)]
    pub fn update_library_path(&self, path: String) {
        let mode = {
            let mut g = self.inner.lock().unwrap();
            g.config.library_paths = if path.trim().is_empty() {
                Vec::new()
            } else {
                vec![path]
            };
            g.config = g.config.clone().normalize();
            let mode = g.config.media_mode;
            g.invalidate_mode(mode);
            g.message = "索引中…".into();
            let cfg = g.config.clone();
            drop(g);
            thread::spawn(move || {
                let _ = crate::config::save(&cfg);
            });
            mode
        };
        self.begin_scan(mode);
    }

    pub fn add_library_path(&self, path: String) {
        let mode = {
            let mut g = self.inner.lock().unwrap();
            let mode = g.config.media_mode;
            g.config.add_path_for(mode, path);
            g.invalidate_mode(mode);
            g.message = "索引中…".into();
            let cfg = g.config.clone();
            drop(g);
            thread::spawn(move || {
                let _ = crate::config::save(&cfg);
            });
            mode
        };
        self.begin_scan(mode);
    }

    /// Remove a library root. Returns the removed path for UI feedback.
    pub fn remove_library_path(&self, index: usize) -> Option<String> {
        let (removed, mode) = {
            let mut g = self.inner.lock().unwrap();
            let mode = g.config.media_mode;
            let removed = g.config.remove_path_for(mode, index);
            if removed.is_some() {
                g.invalidate_mode(mode);
                g.message = "索引中…".into();
                let cfg = g.config.clone();
                thread::spawn(move || {
                    let _ = crate::config::save(&cfg);
                });
            }
            (removed, mode)
        };
        if removed.is_some() {
            self.begin_scan(mode);
        }
        removed
    }

    pub fn set_potplayer_path(&self, path: String) {
        let mut g = self.inner.lock().unwrap();
        g.config.potplayer_path = path;
        let _ = crate::config::save(&g.config);
    }

    pub fn rescan(&self) {
        let mode = {
            let mut g = self.inner.lock().unwrap();
            let mode = g.config.media_mode;
            // Keep old cache visible until new scan lands; just mark busy.
            g.message = "索引中…".into();
            mode
        };
        self.begin_scan(mode);
    }

    /// Randomly pick a slate into preview only — does not launch PotPlayer.
    pub fn roll_preview(&self) {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle {
            return;
        }
        if g.is_scan_busy(g.config.media_mode) {
            g.message = "片库索引中，请稍候…".into();
            return;
        }
        if g.library().is_empty() {
            g.message = "片库为空，无法预览".into();
            return;
        }
        let n = g.config.clamp_count_for(g.config.media_mode, g.ui_count);
        let avoid = g.history().as_path_set();
        let chosen = picker::pick(
            &g.library().files.clone(),
            n,
            g.config.avoid_recent,
            &avoid,
        );
        if chosen.is_empty() {
            g.message = "未能选出影片".into();
            g.preview_files.clear();
            return;
        }
        g.preview_files = chosen;
        g.last_errors.clear();
        g.message = format!(
            "预览就绪 · {} 部 · 确认后点「开启播放」",
            g.preview_files.len()
        );
    }

    /// Remove one title from the preview slate (before play).
    pub fn remove_preview_item(&self, index: usize) {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle || index >= g.preview_files.len() {
            return;
        }
        g.preview_files.remove(index);
        if g.preview_files.is_empty() {
            g.message = idle_message(&g);
        } else {
            g.message = format!(
                "预览就绪 · {} 部 · 确认后点「开启播放」",
                g.preview_files.len()
            );
        }
    }

    /// Launch current preview: movies → PotPlayer; images → in-app slideshow.
    pub fn start(&self) {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle {
            return;
        }
        if g.preview_files.is_empty() {
            g.message = "请先「随机预览」生成片单".into();
            return;
        }
        if g.config.media_mode == MediaMode::Image {
            let n = g.preview_files.len();
            g.phase = SessionPhase::Playing;
            g.items.clear();
            g.last_errors.clear();
            g.message = match g.config.image_play_style {
                ImagePlayStyle::Slideshow => {
                    format!("幻灯中 · {n} 张 · 空格暂停 · ←/→ 切换 · Esc 结束")
                }
                ImagePlayStyle::Wall => {
                    format!("平铺墙 · {n} 张 · 点击放大 · Esc 结束")
                }
            };
            return;
        }

        g.phase = SessionPhase::Starting;
        g.message = "正在开启播放…".into();
        g.last_errors.clear();
        drop(g);

        let handle = self.inner.clone();
        thread::spawn(move || {
            run_start(handle);
        });
    }

    pub fn stop(&self) {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Playing {
            return;
        }
        if g.config.media_mode == MediaMode::Image {
            g.phase = SessionPhase::Idle;
            g.items.clear();
            if g.preview_files.is_empty() {
                g.message = idle_message(&g);
            } else {
                g.message = format!(
                    "已停止幻灯 · 预览仍保留 {} 张",
                    g.preview_files.len()
                );
            }
            return;
        }

        g.phase = SessionPhase::Stopping;
        g.message = "正在关闭本轮…".into();
        let pids: Vec<u32> = g.items.iter().map(|i| i.pid).collect();
        g.items.clear();
        drop(g);

        let handle = self.inner.clone();
        thread::spawn(move || {
            potplayer::kill_pids(&pids);
            let mut g = handle.lock().unwrap();
            g.phase = SessionPhase::Idle;
            if g.preview_files.is_empty() {
                g.message = idle_message(&g);
            } else {
                g.message = format!(
                    "已停止 · 预览仍保留 {} 部，可再「开启播放」或「再来一批」",
                    g.preview_files.len()
                );
            }
        });
    }

    /// Re-roll preview only. If playing, stop players first then new preview (no auto-play).
    pub fn reroll(&self) {
        let mut g = self.inner.lock().unwrap();
        if g.phase == SessionPhase::Starting || g.phase == SessionPhase::Stopping {
            return;
        }
        if g.phase == SessionPhase::Idle {
            drop(g);
            self.roll_preview();
            return;
        }
        // Playing → stop then preview
        let pids: Vec<u32> = g.items.iter().map(|i| i.pid).collect();
        g.phase = SessionPhase::Stopping;
        g.message = "再来一批…".into();
        g.items.clear();
        drop(g);

        let handle = self.inner.clone();
        thread::spawn(move || {
            if !pids.is_empty() {
                potplayer::kill_pids(&pids);
                thread::sleep(std::time::Duration::from_millis(250));
            }
            {
                let mut g = handle.lock().unwrap();
                g.phase = SessionPhase::Idle;
            }
            // roll_preview needs Idle
            let session = SessionHandle { inner: handle };
            session.roll_preview();
        });
    }

    pub fn shutdown_if_needed(&self) {
        let g = self.inner.lock().unwrap();
        if g.config.close_session_on_exit && !g.items.is_empty() {
            let pids: Vec<u32> = g.items.iter().map(|i| i.pid).collect();
            drop(g);
            potplayer::kill_pids(&pids);
        }
    }
}

fn save_history(mode: MediaMode, history: &History) {
    let _ = match mode {
        MediaMode::Movie => history.save(),
        MediaMode::Image => history.save_images(),
    };
}

fn idle_message(g: &Inner) -> String {
    let unit = match g.config.media_mode {
        MediaMode::Movie => "部",
        MediaMode::Image => "张",
    };
    if g.is_scan_busy(g.config.media_mode) {
        return "索引中…".into();
    }
    if g.library().is_empty() {
        if g.config.roots_for(g.config.media_mode).is_empty() {
            format!("请先设置{}库目录", g.config.media_mode.label())
        } else {
            format!("{}库中未找到文件", g.config.media_mode.label())
        }
    } else if g.preview_files.is_empty() {
        format!(
            "就绪 · 已索引 {} {} · 先「随机预览」",
            g.library().len(),
            unit
        )
    } else {
        format!(
            "预览就绪 · {} {} · 可「开启播放」",
            g.preview_files.len(),
            unit
        )
    }
}

fn run_start(handle: Arc<Mutex<Inner>>) {
    let (config, chosen) = {
        let g = handle.lock().unwrap();
        (g.config.clone(), g.preview_files.clone())
    };

    if chosen.is_empty() {
        let mut g = handle.lock().unwrap();
        g.phase = SessionPhase::Idle;
        g.message = "请先「随机预览」生成片单".into();
        return;
    }

    let pot = match potplayer::resolve_potplayer_path(&config.potplayer_path) {
        Some(p) => p,
        None => {
            let mut g = handle.lock().unwrap();
            g.phase = SessionPhase::Idle;
            g.message = "未找到 PotPlayer，请在设置中指定路径".into();
            return;
        }
    };

    let (launched, errors) = potplayer::launch_many(&pot, &chosen);

    if launched.is_empty() {
        let mut g = handle.lock().unwrap();
        g.phase = SessionPhase::Idle;
        g.last_errors = errors;
        g.message = "启动 PotPlayer 失败".into();
        return;
    }

    let pids: Vec<u32> = launched.iter().map(|i| i.pid).collect();
    tile_session(&pids);

    // Late re-tile: some PotPlayer builds move themselves after first paint
    let pids_retile = pids.clone();
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(900));
        tile_session(&pids_retile);
        thread::sleep(std::time::Duration::from_millis(700));
        tile_session(&pids_retile);
    });

    // Update history with successfully launched
    {
        let mut g = handle.lock().unwrap();
        let paths: Vec<PathBuf> = launched.iter().map(|i| i.path.clone()).collect();
        let hist_size = g.config.recent_history_size;
        let mode = g.config.media_mode;
        g.history_mut().push_many(&paths, hist_size);
        let hist = g.history().clone();
        save_history(mode, &hist);
        // Keep preview in sync with what actually launched
        g.preview_files = paths.clone();
        g.items = launched;
        g.phase = SessionPhase::Playing;
        g.last_errors = errors;
        let mut msg = format!("播放中 · {} 部", g.items.len());
        if !g.last_errors.is_empty() {
            msg.push_str(" · 部分失败");
        }
        g.message = msg;
    }

    // After PotPlayers open, raise control panel and pin it (app clears pin on minimize).
    thread::spawn(|| {
        thread::sleep(std::time::Duration::from_millis(450));
        crate::tray::force_show_and_pin();
        thread::sleep(std::time::Duration::from_millis(600));
        crate::tray::force_show_and_pin();
    });
}

fn scan_config_roots(roots: &[String], exts: &[String]) -> Library {
    if roots.is_empty() {
        return Library::empty();
    }
    let paths: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
    Library::scan_many(&paths, exts)
}

fn file_stem(p: &std::path::Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

fn tile_session(pids: &[u32]) {
    let hwnd_pairs = potplayer::find_hwnds_for_pids(pids, 15, 180);
    if hwnd_pairs.is_empty() {
        return;
    }
    let Ok(area) = tiler::work_area() else {
        return;
    };
    let mut hwnds_ordered = Vec::new();
    for pid in pids {
        if let Some((_, h)) = hwnd_pairs.iter().find(|(p, _)| p == pid) {
            hwnds_ordered.push(*h);
        }
    }
    if hwnds_ordered.is_empty() {
        hwnds_ordered = hwnd_pairs.iter().map(|(_, h)| *h).collect();
    }
    let rects = tiler::grid_layout(hwnds_ordered.len(), area);
    tiler::tile_hwnds_stable(&hwnds_ordered, &rects);
}
