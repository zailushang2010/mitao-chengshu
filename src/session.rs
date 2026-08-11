use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::blacklist::Blacklist;
use crate::config::{Config, ImagePlayStyle, MediaMode};
use crate::favorites::Favorites;
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

/// User-facing result of toolbar「重新扫描」.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescanOutcome {
    /// Index already matches disk; disk cache kept, no long walk.
    AlreadyFresh { count: usize },
    /// Background scan started. `force`: all roots re-walked; else only stale roots.
    Started { force: bool },
    /// A scan is already running.
    Busy,
    /// No library roots configured.
    NoRoots,
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
    /// Files matched so far during the active mode's scan (0 if idle).
    pub indexing_found: usize,
    /// Movie PotPlayers still running while user is in 图片 mode (plan A).
    pub movie_in_background: bool,
    pub movie_background_count: usize,
    /// Configured/auto path resolves to a PotPlayer exe (movie complete path).
    pub pot_available: bool,
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
            indexing_found: 0,
            movie_in_background: false,
            movie_background_count: 0,
            pot_available: false,
        }
    }
}

/// Movie players parked when switching to 图片 without killing PotPlayer.
struct ParkedMovie {
    items: Vec<LaunchedItem>,
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
    movie_blacklist: Blacklist,
    image_blacklist: Blacklist,
    movie_favorites: Favorites,
    image_favorites: Favorites,
    phase: SessionPhase,
    /// Active mode's preview slate (synced from per-mode stores on switch).
    preview_files: Vec<PathBuf>,
    movie_preview: Vec<PathBuf>,
    image_preview: Vec<PathBuf>,
    items: Vec<LaunchedItem>,
    /// Live PotPlayers kept when leaving 电影 for 图片.
    parked_movie: Option<ParkedMovie>,
    message: String,
    last_errors: Vec<String>,
    ui_count: usize,
    /// Live counter while a background scan runs for any mode.
    scan_found: Arc<AtomicUsize>,
    /// Cooperative cancel for in-flight library scan.
    scan_cancel: Arc<AtomicBool>,
    /// Geometry guard running: force cell size for whole Playing session.
    geometry_guard: bool,
    /// Assigned grid cell per PID (source of truth for enforcement).
    tile_targets: std::collections::HashMap<u32, tiler::Rect>,
    /// Bumped to cancel in-flight `run_start` (mode switch / 取消开启).
    play_gen: u64,
    /// Optional one-shot play list (subset of preview). Taken by `start` / `run_start`.
    play_queue: Option<Vec<PathBuf>>,
    /// Full slate restored after image subset play ends.
    preview_backup: Option<Vec<PathBuf>>,
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

    fn blacklist(&self) -> &Blacklist {
        match self.config.media_mode {
            MediaMode::Movie => &self.movie_blacklist,
            MediaMode::Image => &self.image_blacklist,
        }
    }

    fn blacklist_mut(&mut self) -> &mut Blacklist {
        match self.config.media_mode {
            MediaMode::Movie => &mut self.movie_blacklist,
            MediaMode::Image => &mut self.image_blacklist,
        }
    }

    fn favorites(&self) -> &Favorites {
        self.favorites_for(self.config.media_mode)
    }

    fn favorites_mut(&mut self) -> &mut Favorites {
        self.favorites_mut_for(self.config.media_mode)
    }

    fn favorites_for(&self, mode: MediaMode) -> &Favorites {
        match mode {
            MediaMode::Movie => &self.movie_favorites,
            MediaMode::Image => &self.image_favorites,
        }
    }

    fn favorites_mut_for(&mut self, mode: MediaMode) -> &mut Favorites {
        match mode {
            MediaMode::Movie => &mut self.movie_favorites,
            MediaMode::Image => &mut self.image_favorites,
        }
    }

    fn save_favorites_for(&self, mode: MediaMode) -> Result<(), String> {
        match mode {
            MediaMode::Movie => self.movie_favorites.save_movies(),
            MediaMode::Image => self.image_favorites.save_images(),
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
        if !busy {
            // leave scan_cancel as-is; next begin_scan clears it
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
        let movie_blacklist = Blacklist::load_movies();
        let image_blacklist = Blacklist::load_images();
        let movie_favorites = Favorites::load_movies();
        let image_favorites = Favorites::load_images();
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
            movie_blacklist,
            image_blacklist,
            movie_favorites,
            image_favorites,
            phase: SessionPhase::Idle,
            preview_files: Vec::new(),
            movie_preview: Vec::new(),
            image_preview: Vec::new(),
            items: Vec::new(),
            parked_movie: None,
            message: "就绪 · 先「随机预览」再确认开启".into(),
            last_errors: Vec::new(),
            ui_count,
            scan_found: Arc::new(AtomicUsize::new(0)),
            scan_cancel: Arc::new(AtomicBool::new(false)),
            geometry_guard: false,
            tile_targets: std::collections::HashMap::new(),
            play_gen: 0,
            play_queue: None,
            preview_backup: None,
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
        // Scan active mode first; also warm the other mode so 电影↔图片 is instant.
        if !roots.is_empty() {
            handle.begin_scan(mode);
        }
        let other = match mode {
            MediaMode::Movie => MediaMode::Image,
            MediaMode::Image => MediaMode::Movie,
        };
        let other_roots = handle.inner.lock().unwrap().config.roots_for(other);
        if !other_roots.is_empty() {
            handle.begin_scan(other);
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
            indexing_found: if g.is_scan_busy(mode) {
                g.scan_found.load(Ordering::Relaxed)
            } else {
                0
            },
            movie_in_background: g.parked_movie.is_some(),
            movie_background_count: g
                .parked_movie
                .as_ref()
                .map(|p| p.items.len())
                .unwrap_or(0),
            pot_available: potplayer::resolve_potplayer_path(&g.config.potplayer_path).is_some(),
        }
    }

    pub fn media_mode(&self) -> MediaMode {
        self.inner.lock().unwrap().config.media_mode
    }

    pub fn set_media_mode(&self, mode: MediaMode) {
        let (pids_to_kill, need_scan, cfg_to_save) = {
            let mut g = self.inner.lock().unwrap();
            let old = g.config.media_mode;
            if old == mode {
                return;
            }

            // Persist active preview into per-mode store
            match old {
                MediaMode::Movie => g.movie_preview = g.preview_files.clone(),
                MediaMode::Image => g.image_preview = g.preview_files.clone(),
            }

            let mut pids_to_kill: Vec<u32> = Vec::new();

            // Leave movie: park running PotPlayers (plan A); kill only mid-start/stop.
            if old == MediaMode::Movie {
                match g.phase {
                    SessionPhase::Playing if !g.items.is_empty() => {
                        let items = std::mem::take(&mut g.items);
                        // Keep tile_targets + geometry_guard so background pots stay in grid
                        g.parked_movie = Some(ParkedMovie { items });
                        g.phase = SessionPhase::Idle;
                    }
                    SessionPhase::Starting | SessionPhase::Stopping => {
                        // Invalidate in-flight run_start so it will kill launched pots
                        g.play_gen = g.play_gen.wrapping_add(1);
                        pids_to_kill = g
                            .items
                            .iter()
                            .map(|i| i.pid)
                            .filter(|p| *p != 0)
                            .collect();
                        g.items.clear();
                        g.tile_targets.clear();
                        g.geometry_guard = false;
                        g.phase = SessionPhase::Idle;
                    }
                    _ => {
                        g.items.clear();
                        g.phase = SessionPhase::Idle;
                    }
                }
            } else {
                // Leave image: in-app only — drop play state, keep preview store.
                g.items.clear();
                g.phase = SessionPhase::Idle;
            }

            g.config.media_mode = mode;
            g.ui_count = g.config.default_count_for(mode);
            g.preview_files = match mode {
                MediaMode::Movie => g.movie_preview.clone(),
                MediaMode::Image => g.image_preview.clone(),
            };
            g.last_errors.clear();

            // Enter movie: restore parked players so user can 关闭本轮
            if mode == MediaMode::Movie {
                if let Some(parked) = g.parked_movie.take() {
                    let n = parked.items.len();
                    g.items = parked.items;
                    g.phase = SessionPhase::Playing;
                    // Resume grid enforcement with existing targets if any
                    if !g.tile_targets.is_empty() {
                        g.geometry_guard = true;
                    }
                    g.message = format!("播放中 · {n} 部 · 已从图片切回，可「关闭本轮」");
                } else {
                    g.phase = SessionPhase::Idle;
                    g.message = mode_switch_message(&g);
                }
            } else {
                // Enter image; movie may be parked in background
                g.phase = SessionPhase::Idle;
                g.message = mode_switch_message(&g);
            }

            let cfg_to_save = g.config.clone();
            let need_scan = !g.is_indexed(mode) && !g.is_scan_busy(mode);
            (pids_to_kill, need_scan, cfg_to_save)
        };
        thread::spawn(move || {
            let _ = crate::config::save(&cfg_to_save);
        });
        if !pids_to_kill.is_empty() {
            thread::spawn(move || {
                potplayer::kill_pids(&pids_to_kill);
            });
        }
        if need_scan {
            self.begin_scan(mode);
        }
    }

    /// Background scan for a media mode. `force` skips disk index cache.
    fn begin_scan(&self, mode: MediaMode) {
        self.begin_scan_ex(mode, false);
    }

    fn begin_scan_ex(&self, mode: MediaMode, force: bool) {
        let (roots, exts, epoch, progress, cancel) = {
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
            g.scan_found.store(0, Ordering::Relaxed);
            g.scan_cancel.store(false, Ordering::Relaxed);
            let progress = g.scan_found.clone();
            let cancel = g.scan_cancel.clone();
            if g.config.media_mode == mode {
                g.message = if force {
                    "全量扫描中… 扫完前仍可用当前名单".into()
                } else {
                    "索引中… 未变目录走缓存".into()
                };
            }
            (roots, exts, epoch, progress, cancel)
        };

        let handle = self.inner.clone();
        thread::spawn(move || {
            let is_cancelled = || cancel.load(Ordering::Relaxed);
            let mut all_files: Vec<PathBuf> = Vec::new();
            let mut root_paths: Vec<PathBuf> = Vec::new();
            let handle_msg = handle.clone();
            let mut from_cache = 0usize;
            let mut walked = 0usize;

            for root_s in &roots {
                if is_cancelled() {
                    break;
                }
                let root = PathBuf::from(root_s);
                root_paths.push(root.clone());
                if !force {
                    if let Some(cached) = crate::index_cache::load_root(&root, &exts) {
                        let base = all_files.len();
                        all_files.extend(cached);
                        from_cache += 1;
                        progress.store(all_files.len(), Ordering::Relaxed);
                        if let Ok(mut g) = handle_msg.try_lock() {
                            if g.config.media_mode == mode && g.is_scan_busy(mode) {
                                g.message = format!(
                                    "索引中… 缓存命中 {} 个（合计 {}）",
                                    all_files.len() - base,
                                    all_files.len()
                                );
                            }
                        }
                        continue;
                    }
                }
                walked += 1;
                let start = all_files.len();
                let mut on_found = |n: usize| {
                    progress.store(n, Ordering::Relaxed);
                    if n == 1 || n % 40 == 0 {
                        if let Ok(mut g) = handle_msg.try_lock() {
                            if g.config.media_mode == mode && g.is_scan_busy(mode) {
                                g.message = format!("索引中… 已发现 {n} 个");
                            }
                        }
                    }
                };
                match Library::scan_one_cancellable(
                    &root,
                    &exts,
                    &mut on_found,
                    &is_cancelled,
                    start,
                ) {
                    Some(files) => {
                        if !is_cancelled() {
                            // Only rewrite this root's disk cache after a successful walk
                            crate::index_cache::save_root(&root, &exts, &files);
                        }
                        all_files.extend(files);
                        progress.store(all_files.len(), Ordering::Relaxed);
                    }
                    None => break,
                }
            }

            let mut g = handle.lock().unwrap();
            if g.scan_epoch(mode) != epoch {
                return;
            }
            if is_cancelled() {
                g.set_scan_busy(mode, false);
                // Keep previous library if any — never left empty mid-cancel after rescan
                if g.config.media_mode == mode {
                    g.message = if g.is_indexed(mode) {
                        format!("索引已取消 · 仍用上次 {} 个", g.library().len())
                    } else {
                        "索引已取消".into()
                    };
                }
                return;
            }
            all_files.sort();
            all_files.dedup();
            let n = all_files.len();
            let lib = Library {
                roots: root_paths,
                files: all_files,
            };
            g.set_library(mode, lib);
            if g.config.media_mode == mode && g.phase == SessionPhase::Idle {
                if force {
                    g.message = format!("全量扫描完成 · {n} 个");
                } else if walked == 0 {
                    g.message = format!("片库无变化 · 缓存 {n} 个");
                } else {
                    g.message = format!(
                        "索引已更新 · {n} 个（重扫 {walked} 目录 · 缓存 {from_cache}）"
                    );
                }
            }
        });
    }

    /// Cancel in-flight index for the active media mode.
    pub fn cancel_scan(&self) {
        let mut g = self.inner.lock().unwrap();
        let mode = g.config.media_mode;
        if !g.is_scan_busy(mode) {
            return;
        }
        g.scan_cancel.store(true, Ordering::Relaxed);
        g.bump_scan_epoch(mode); // supersede worker result
        g.set_scan_busy(mode, false);
        g.message = if g.is_indexed(mode) {
            format!("索引已取消 · 仍用上次 {} 个", g.library().len())
        } else {
            "索引已取消".into()
        };
    }

    /// Drop dead PotPlayer PIDs from active items and parked background.
    /// Returns true if anything changed (UI should refresh message).
    pub fn reap_dead_players(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        let mut changed = false;

        if !g.items.is_empty() && g.config.media_mode == MediaMode::Movie {
            let before = g.items.len();
            g.items.retain(|i| potplayer::pid_alive(i.pid));
            if g.items.len() != before {
                changed = true;
                if g.items.is_empty() && g.phase == SessionPhase::Playing {
                    g.phase = SessionPhase::Idle;
                    g.message = if g.preview_files.is_empty() {
                        idle_message(&g)
                    } else {
                        format!(
                            "播放器已全部关闭 · 预览仍保留 {} 部",
                            g.preview_files.len()
                        )
                    };
                } else if g.phase == SessionPhase::Playing {
                    g.message = format!("播放中 · {} 部", g.items.len());
                }
            }
        }

        if let Some(ref mut parked) = g.parked_movie {
            let before = parked.items.len();
            parked.items.retain(|i| potplayer::pid_alive(i.pid));
            if parked.items.len() != before {
                changed = true;
            }
            if parked.items.is_empty() {
                g.parked_movie = None;
                if g.config.media_mode == MediaMode::Image && g.phase == SessionPhase::Idle {
                    g.message = idle_message(&g);
                }
                changed = true;
            }
        }

        changed
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
            // Solo is intentionally full-screen — pause grid enforcement
            g.geometry_guard = false;
            g.tile_targets.clear();
            g.message = format!("独播 · {}", file_stem(&keep.path));
        }
        potplayer::maximize_pid(keep.pid);
        potplayer::focus_pid(keep.pid);
    }

    /// Close one playing item; if none left → Idle.
    pub fn close_item(&self, index: usize) {
        let (pid, regrid) = {
            let mut g = self.inner.lock().unwrap();
            if index >= g.items.len() {
                return;
            }
            let item = g.items.remove(index);
            g.tile_targets.remove(&item.pid);
            if g.items.is_empty() {
                g.phase = SessionPhase::Idle;
                g.geometry_guard = false;
                g.tile_targets.clear();
                g.message = format!("就绪 · 已索引 {} 部", g.library().len());
                (item.pid, false)
            } else {
                g.message = format!("播放中 · {} 部", g.items.len());
                (item.pid, true)
            }
        };
        let _ = potplayer::kill_pid(pid);
        if regrid {
            self.retile_now();
        }
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

    pub fn set_tile_monitor_index(&self, index: i32) {
        {
            let mut g = self.inner.lock().unwrap();
            g.config.tile_monitor_index = index;
            let _ = crate::config::save(&g.config);
        }
        // Live / parked pots should jump to the new work area immediately
        self.retile_now();
    }

    pub fn set_workbench_sidebar_open(&self, open: bool) {
        let mut g = self.inner.lock().unwrap();
        if g.config.workbench_sidebar_open == open {
            return;
        }
        g.config.workbench_sidebar_open = open;
        let _ = crate::config::save(&g.config);
    }

    /// Persist preview grid column density (2–5).
    pub fn set_card_cols(&self, cols: u8) {
        let cols = cols.clamp(2, 5);
        let mut g = self.inner.lock().unwrap();
        if g.config.card_cols == cols {
            return;
        }
        g.config.card_cols = cols;
        let _ = crate::config::save(&g.config);
    }

    /// Persist main window size/position (debounced by caller).
    pub fn set_window_geometry(&self, geom: crate::config::WindowGeometry) {
        let geom = geom.clamp_size();
        let mut g = self.inner.lock().unwrap();
        if g.config.window_geometry == Some(geom) {
            return;
        }
        g.config.window_geometry = Some(geom);
        let _ = crate::config::save(&g.config);
    }

    pub fn set_pin_while_playing(&self, on: bool) {
        let mut g = self.inner.lock().unwrap();
        if g.config.pin_while_playing == on {
            return;
        }
        g.config.pin_while_playing = on;
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
            let exts = g.config.extensions_for(mode).to_vec();
            g.config.add_path_for(mode, path.clone());
            g.invalidate_mode(mode);
            crate::index_cache::invalidate_root(&path, &exts);
            g.message = "索引中…".into();
            let cfg = g.config.clone();
            drop(g);
            thread::spawn(move || {
                let _ = crate::config::save(&cfg);
            });
            mode
        };
        self.begin_scan_ex(mode, true);
    }

    /// Remove a library root. Returns the removed path for UI feedback.
    pub fn remove_library_path(&self, index: usize) -> Option<String> {
        let (removed, mode) = {
            let mut g = self.inner.lock().unwrap();
            let mode = g.config.media_mode;
            let removed = g.config.remove_path_for(mode, index);
            if let Some(ref p) = removed {
                let exts = g.config.extensions_for(mode).to_vec();
                crate::index_cache::invalidate_root(p, &exts);
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
            self.begin_scan_ex(mode, true);
        }
        removed
    }

    pub fn set_potplayer_path(&self, path: String) {
        let mut g = self.inner.lock().unwrap();
        g.config.potplayer_path = path;
        let _ = crate::config::save(&g.config);
    }

    /// Default toolbar action: **smart** rescan.
    /// - Does **not** wipe the in-memory list or disk cache first (旧名单一直可用到扫完).
    /// - Unchanged roots reuse on-disk cache; only changed roots are re-walked.
    /// - If nothing is stale, returns [`RescanOutcome::AlreadyFresh`] immediately.
    pub fn rescan(&self) -> RescanOutcome {
        self.rescan_ex(false)
    }

    /// Shift+click / advanced: re-walk every root, but still keep the previous
    /// in-memory library until the new scan finishes (and rewrite disk cache only then).
    pub fn rescan_force(&self) -> RescanOutcome {
        self.rescan_ex(true)
    }

    fn rescan_ex(&self, force: bool) -> RescanOutcome {
        let (mode, busy, count, roots, exts) = {
            let g = self.inner.lock().unwrap();
            let mode = g.config.media_mode;
            (
                mode,
                g.is_scan_busy(mode),
                g.library().len(),
                g.config.roots_for(mode),
                g.config.extensions_for(mode).to_vec(),
            )
        };
        if busy {
            return RescanOutcome::Busy;
        }
        if roots.is_empty() {
            return RescanOutcome::NoRoots;
        }
        if !force {
            let any_stale = roots.iter().any(|r| {
                crate::index_cache::is_stale(PathBuf::from(r).as_path(), &exts)
            });
            if !any_stale {
                // Quick consistency pass: rebuild from cache only (still cheap)
                // so UI count stays in sync; do not full-walk.
                {
                    let mut g = self.inner.lock().unwrap();
                    if g.config.media_mode == mode && g.phase == SessionPhase::Idle {
                        g.message = format!("片库无变化 · 继续用缓存 {count} 个");
                    }
                }
                return RescanOutcome::AlreadyFresh { count };
            }
            {
                let mut g = self.inner.lock().unwrap();
                if g.config.media_mode == mode {
                    g.message = "检查片库… 有变化的目录才重扫".into();
                }
            }
            // Do NOT invalidate_mode — keep previous list until worker finishes
            self.begin_scan_ex(mode, false);
            RescanOutcome::Started { force: false }
        } else {
            {
                let mut g = self.inner.lock().unwrap();
                if g.config.media_mode == mode {
                    g.message = "全量扫描中… 扫完前仍可用当前名单".into();
                }
            }
            // Keep memory list; force only skips reading cache per root
            self.begin_scan_ex(mode, true);
            RescanOutcome::Started { force: true }
        }
    }

    /// If any library root's on-disk tree no longer matches the index cache,
    /// start a background non-force scan (stale roots re-walk; fresh roots reuse cache).
    /// Returns true when a scan was started.
    pub fn refresh_if_stale(&self) -> bool {
        matches!(
            self.rescan_ex(false),
            RescanOutcome::Started { force: false }
        )
    }

    /// Randomly pick a slate into preview only — does not launch PotPlayer.
    pub fn roll_preview(&self) {
        // Prefer a fresh index when nested folders changed (no manual 重新扫描).
        if self.refresh_if_stale() {
            let mut g = self.inner.lock().unwrap();
            if g.phase == SessionPhase::Idle {
                g.message = "片库正在更新，完成后请再点随机预览".into();
            }
            return;
        }
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
        let blocked = g.blacklist().as_path_set();
        let lib_files = g.library().files.clone();
        let chosen = picker::pick(
            &lib_files,
            n,
            g.config.avoid_recent,
            &avoid,
            &blocked,
        );
        if chosen.is_empty() {
            let eligible = lib_files.len().saturating_sub(blocked.len());
            g.message = if eligible == 0 && !blocked.is_empty() {
                "可用片源已被拉黑筛空，请在设置中管理黑名单".into()
            } else {
                "未能选出影片".into()
            };
            g.preview_files.clear();
            sync_preview_store(&mut g);
            return;
        }
        g.preview_files = chosen;
        sync_preview_store(&mut g);
        g.last_errors.clear();
        g.message = preview_ready_message(&g);
    }

    /// Remove one title from the preview slate (before play).
    pub fn remove_preview_item(&self, index: usize) {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle || index >= g.preview_files.len() {
            return;
        }
        g.preview_files.remove(index);
        sync_preview_store(&mut g);
        if g.preview_files.is_empty() {
            g.message = idle_message(&g);
        } else {
            g.message = preview_ready_message(&g);
        }
    }

    /// Replace one preview slot with a new random pick; keep the rest.
    /// Returns `Ok(new_path)` or `Err` reason for toast.
    pub fn replace_preview_item(&self, index: usize) -> Result<PathBuf, String> {
        self.replace_preview_items(&[index])
            .map(|_n| {
                // return path at index after replace
                self.inner
                    .lock()
                    .unwrap()
                    .preview_files
                    .get(index)
                    .cloned()
                    .unwrap_or_default()
            })
            .and_then(|p| {
                if p.as_os_str().is_empty() {
                    Err("换一部失败".into())
                } else {
                    Ok(p)
                }
            })
    }

    /// Replace several preview slots; keep unselected. Returns how many were replaced.
    pub fn replace_preview_items(&self, indices: &[usize]) -> Result<usize, String> {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle {
            return Err("播放中不能换预览".into());
        }
        if g.is_scan_busy(g.config.media_mode) {
            return Err("片库索引中，请稍候".into());
        }
        if g.library().is_empty() {
            return Err("片库为空".into());
        }
        let mut idxs: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| i < g.preview_files.len())
            .collect();
        idxs.sort_unstable();
        idxs.dedup();
        if idxs.is_empty() {
            return Err("请先勾选要换的项".into());
        }

        let avoid = g.history().as_path_set();
        let blocked = g.blacklist().as_path_set();
        let lib_files = g.library().files.clone();
        let mut replaced = 0usize;
        let mut last_err = String::new();

        for &index in &idxs {
            // Exclude all current slate so we never duplicate keepers or re-pick same
            let exclude = g.preview_files.clone();
            match picker::pick_one_excluding(
                &lib_files,
                g.config.avoid_recent,
                &avoid,
                &blocked,
                &exclude,
            ) {
                Some(next) => {
                    g.preview_files[index] = next;
                    replaced += 1;
                }
                None => {
                    last_err = "没有更多可换片源（可检查黑名单或扩大片库）".into();
                    break;
                }
            }
        }

        sync_preview_store(&mut g);
        g.message = preview_ready_message(&g);
        if replaced == 0 {
            Err(if last_err.is_empty() {
                "未能换片".into()
            } else {
                last_err
            })
        } else if replaced < idxs.len() {
            Err(format!(
                "已换 {replaced}/{} 部，其余无更多片源",
                idxs.len()
            ))
        } else {
            Ok(replaced)
        }
    }

    /// Remove several preview slots (higher index first). Returns removed count.
    pub fn remove_preview_items(&self, indices: &[usize]) -> usize {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle {
            return 0;
        }
        let mut idxs: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| i < g.preview_files.len())
            .collect();
        idxs.sort_unstable_by(|a, b| b.cmp(a));
        idxs.dedup();
        let mut n = 0;
        for i in idxs {
            if i < g.preview_files.len() {
                g.preview_files.remove(i);
                n += 1;
            }
        }
        sync_preview_store(&mut g);
        if g.preview_files.is_empty() {
            g.message = idle_message(&g);
        } else {
            g.message = preview_ready_message(&g);
        }
        n
    }

    /// Blacklist several preview slots. Returns how many were banned.
    pub fn blacklist_preview_items(&self, indices: &[usize]) -> usize {
        let mut paths = Vec::new();
        let mode = {
            let mut g = self.inner.lock().unwrap();
            if g.phase != SessionPhase::Idle {
                return 0;
            }
            let mut idxs: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|&i| i < g.preview_files.len())
                .collect();
            idxs.sort_unstable_by(|a, b| b.cmp(a));
            idxs.dedup();
            let mode = g.config.media_mode;
            for i in idxs {
                if i < g.preview_files.len() {
                    let path = g.preview_files.remove(i);
                    g.blacklist_mut().add(&path);
                    g.favorites_mut().remove(&path);
                    paths.push(path);
                }
            }
            sync_preview_store(&mut g);
            if g.preview_files.is_empty() {
                g.message = idle_message(&g);
            } else {
                g.message = preview_ready_message(&g);
            }
            mode
        };
        if !paths.is_empty() {
            let g = self.inner.lock().unwrap();
            let _ = match mode {
                MediaMode::Movie => g.movie_blacklist.save_movies(),
                MediaMode::Image => g.image_blacklist.save_images(),
            };
            let _ = g.save_favorites_for(mode);
        }
        paths.len()
    }

    /// Remove from preview and permanently blacklist (never pick again).
    pub fn blacklist_preview_item(&self, index: usize) -> Option<PathBuf> {
        let (path, mode) = {
            let mut g = self.inner.lock().unwrap();
            if g.phase != SessionPhase::Idle || index >= g.preview_files.len() {
                return None;
            }
            let path = g.preview_files.remove(index);
            g.blacklist_mut().add(&path);
            g.favorites_mut().remove(&path);
            let mode = g.config.media_mode;
            sync_preview_store(&mut g);
            if g.preview_files.is_empty() {
                g.message = idle_message(&g);
            } else {
                g.message = preview_ready_message(&g);
            }
            (path, mode)
        };
        {
            let g = self.inner.lock().unwrap();
            let _ = match mode {
                MediaMode::Movie => g.movie_blacklist.save_movies(),
                MediaMode::Image => g.image_blacklist.save_images(),
            };
            let _ = g.save_favorites_for(mode);
        }
        Some(path)
    }

    pub fn blacklist_count(&self) -> usize {
        self.inner.lock().unwrap().blacklist().len()
    }

    /// Remove one path from the active mode blacklist (settings).
    pub fn unblacklist_path(&self, path: &std::path::Path) {
        let mode = {
            let mut g = self.inner.lock().unwrap();
            g.blacklist_mut().remove(path);
            g.config.media_mode
        };
        let g = self.inner.lock().unwrap();
        let _ = match mode {
            MediaMode::Movie => g.movie_blacklist.save_movies(),
            MediaMode::Image => g.image_blacklist.save_images(),
        };
    }

    pub fn blacklist_paths(&self) -> Vec<PathBuf> {
        self.inner.lock().unwrap().blacklist().as_path_set()
    }

    pub fn favorite_count(&self) -> usize {
        self.favorite_count_for(self.media_mode())
    }

    pub fn favorite_count_for(&self, mode: MediaMode) -> usize {
        self.inner.lock().unwrap().favorites_for(mode).len()
    }

    /// Favorites for the active media mode (may include missing files).
    pub fn favorite_paths(&self) -> Vec<PathBuf> {
        self.favorite_paths_for(self.media_mode())
    }

    pub fn favorite_paths_for(&self, mode: MediaMode) -> Vec<PathBuf> {
        self.inner
            .lock()
            .unwrap()
            .favorites_for(mode)
            .as_path_set()
    }

    /// Favorites that still exist on disk (newest first for stable UI).
    pub fn favorite_paths_existing(&self) -> Vec<PathBuf> {
        self.favorite_paths_existing_for(self.media_mode())
    }

    pub fn favorite_paths_existing_for(&self, mode: MediaMode) -> Vec<PathBuf> {
        self.inner
            .lock()
            .unwrap()
            .favorites_for(mode)
            .as_path_set()
            .into_iter()
            .filter(|p| p.is_file())
            .rev()
            .collect()
    }

    pub fn is_favorite(&self, path: &std::path::Path) -> bool {
        self.inner.lock().unwrap().favorites().contains(path)
    }

    pub fn is_favorite_in(&self, mode: MediaMode, path: &std::path::Path) -> bool {
        self.inner
            .lock()
            .unwrap()
            .favorites_for(mode)
            .contains(path)
    }

    /// Toggle favorite for one path. Returns whether it is now favorited.
    pub fn toggle_favorite(&self, path: &std::path::Path) -> bool {
        let (now, mode) = {
            let mut g = self.inner.lock().unwrap();
            let now = g.favorites_mut().toggle(path);
            (now, g.config.media_mode)
        };
        let g = self.inner.lock().unwrap();
        let _ = g.save_favorites_for(mode);
        now
    }

    /// Add several preview slots to favorites. Returns how many newly added.
    pub fn favorite_preview_items(&self, indices: &[usize]) -> usize {
        let (n, mode) = {
            let mut g = self.inner.lock().unwrap();
            if g.phase != SessionPhase::Idle {
                return 0;
            }
            let mut added = 0usize;
            let mut seen = std::collections::HashSet::new();
            for &i in indices {
                if i >= g.preview_files.len() || !seen.insert(i) {
                    continue;
                }
                let path = g.preview_files[i].clone();
                if !g.favorites().contains(&path) {
                    g.favorites_mut().add(&path);
                    added += 1;
                }
            }
            (added, g.config.media_mode)
        };
        if n > 0 {
            let g = self.inner.lock().unwrap();
            let _ = g.save_favorites_for(mode);
        }
        n
    }

    pub fn unfavorite_path(&self, path: &std::path::Path) {
        self.unfavorite_path_for(self.media_mode(), path);
    }

    pub fn unfavorite_path_for(&self, mode: MediaMode, path: &std::path::Path) {
        {
            let mut g = self.inner.lock().unwrap();
            g.favorites_mut_for(mode).remove(path);
        }
        let g = self.inner.lock().unwrap();
        let _ = g.save_favorites_for(mode);
    }

    pub fn unfavorite_paths(&self, paths: &[PathBuf]) -> usize {
        self.unfavorite_paths_for(self.media_mode(), paths)
    }

    pub fn unfavorite_paths_for(&self, mode: MediaMode, paths: &[PathBuf]) -> usize {
        let n = {
            let mut g = self.inner.lock().unwrap();
            let mut n = 0usize;
            for p in paths {
                if g.favorites_for(mode).contains(p) {
                    g.favorites_mut_for(mode).remove(p);
                    n += 1;
                }
            }
            n
        };
        if n > 0 {
            let g = self.inner.lock().unwrap();
            let _ = g.save_favorites_for(mode);
        }
        n
    }

    /// Random preview slate drawn only from favorites (existing files).
    pub fn roll_preview_from_favorites(&self) -> Result<usize, String> {
        self.roll_preview_from_favorites_for(self.media_mode())
    }

    /// Switch to `mode` if needed, then roll a preview from that mode's favorites.
    pub fn roll_preview_from_favorites_for(&self, mode: MediaMode) -> Result<usize, String> {
        if self.media_mode() != mode {
            self.set_media_mode(mode);
        }
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle {
            return Err("播放中无法换预览".into());
        }
        let pool: Vec<PathBuf> = g
            .favorites_for(mode)
            .as_path_set()
            .into_iter()
            .filter(|p| p.is_file())
            .collect();
        if pool.is_empty() {
            return Err(match mode {
                MediaMode::Movie => "电影收藏为空或文件已不在".into(),
                MediaMode::Image => "图片收藏为空或文件已不在".into(),
            });
        }
        let n = g.config.clamp_count_for(mode, g.ui_count);
        // history/blacklist for the active mode (synced by set_media_mode)
        let avoid = g.history().as_path_set();
        let blocked = g.blacklist().as_path_set();
        let chosen = picker::pick(&pool, n, g.config.avoid_recent, &avoid, &blocked);
        if chosen.is_empty() {
            return Err("未能从收藏中选出".into());
        }
        let got = chosen.len();
        g.preview_files = chosen;
        sync_preview_store(&mut g);
        g.message = preview_ready_message(&g);
        Ok(got)
    }

    /// Play arbitrary paths (favorites stage). Sets them as preview then starts.
    pub fn start_paths(&self, paths: Vec<PathBuf>) -> Result<usize, String> {
        if paths.is_empty() {
            return Err("请先选中要播放的项".into());
        }
        let play = {
            let mut g = self.inner.lock().unwrap();
            if g.phase != SessionPhase::Idle {
                return Err("播放中无法改开播列表".into());
            }
            let cap = Config::ABS_COUNT_MAX;
            let play: Vec<PathBuf> = paths
                .into_iter()
                .filter(|p| p.is_file())
                .take(cap)
                .collect();
            if play.is_empty() {
                return Err("所选文件已不存在".into());
            }
            g.preview_files = play.clone();
            sync_preview_store(&mut g);
            play
        };
        let n = play.len();
        self.start_inner(Some(play));
        Ok(n)
    }

    /// Launch current preview: movies → PotPlayer; images → in-app slideshow.
    pub fn start(&self) {
        self.start_inner(None);
    }

    /// Play only the selected preview indices (search → 单选/多选后单独开播).
    /// Preserves the full preview slate; only this round's play list is narrowed.
    pub fn start_selected(&self, indices: &[usize]) -> Result<usize, String> {
        let paths = {
            let g = self.inner.lock().unwrap();
            if g.phase != SessionPhase::Idle {
                return Err("播放中无法改开播列表".into());
            }
            if g.preview_files.is_empty() {
                return Err("请先生成预览".into());
            }
            let mut idxs: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|&i| i < g.preview_files.len())
                .collect();
            idxs.sort_unstable();
            idxs.dedup();
            if idxs.is_empty() {
                return Err("请先选中要播放的项".into());
            }
            idxs.into_iter()
                .filter_map(|i| g.preview_files.get(i).cloned())
                .collect::<Vec<_>>()
        };
        let n = paths.len();
        self.start_inner(Some(paths));
        Ok(n)
    }

    fn start_inner(&self, queue: Option<Vec<PathBuf>>) {
        let parked_pids = {
            let mut g = self.inner.lock().unwrap();
            if g.phase != SessionPhase::Idle {
                return;
            }
            if g.preview_files.is_empty() && queue.is_none() {
                g.message = "请先「随机预览」生成片单".into();
                return;
            }
            // Safety: never stack a new movie round on parked pots.
            if g.config.media_mode == MediaMode::Movie {
                g.parked_movie
                    .take()
                    .map(|p| p.items.iter().map(|i| i.pid).collect::<Vec<_>>())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        };
        if !parked_pids.is_empty() {
            potplayer::kill_pids(&parked_pids);
        }

        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle {
            return;
        }
        let play_files = queue
            .clone()
            .unwrap_or_else(|| g.preview_files.clone());
        if play_files.is_empty() {
            g.message = "请先「随机预览」生成片单".into();
            return;
        }
        g.play_queue = Some(play_files.clone());

        if g.config.media_mode == MediaMode::Image {
            // Overlays read preview_files while playing; stash full slate if subset.
            let n = play_files.len();
            if queue.is_some() && play_files.len() != g.preview_files.len() {
                g.preview_backup = Some(g.preview_files.clone());
            }
            g.preview_files = play_files;
            g.play_queue = None;
            g.phase = SessionPhase::Playing;
            g.items.clear();
            g.last_errors.clear();
            g.message = match g.config.image_play_style {
                ImagePlayStyle::Slideshow => {
                    format!("幻灯中 · {n} 张 · 空格暂停 · 左右切换 · Esc 结束")
                }
                ImagePlayStyle::Wall => {
                    format!("平铺墙 · {n} 张 · 点击放大 · Esc 结束")
                }
            };
            return;
        }

        g.play_gen = g.play_gen.wrapping_add(1);
        let gen = g.play_gen;
        g.phase = SessionPhase::Starting;
        let n = play_files.len();
        g.message = if n == 1 {
            "正在开启（单部）…".into()
        } else {
            format!("正在开启播放 · {n} 部…")
        };
        g.last_errors.clear();
        drop(g);

        let handle = self.inner.clone();
        thread::spawn(move || {
            run_start(handle, gen);
        });
    }

    pub fn stop(&self) {
        let mut g = self.inner.lock().unwrap();
        // Cancel in-flight movie launch
        if g.phase == SessionPhase::Starting && g.config.media_mode == MediaMode::Movie {
            g.play_gen = g.play_gen.wrapping_add(1);
            g.phase = SessionPhase::Idle;
            g.message = "已取消开启".into();
            return;
        }
        if g.phase != SessionPhase::Playing {
            return;
        }
        if g.config.media_mode == MediaMode::Image {
            g.phase = SessionPhase::Idle;
            g.items.clear();
            if let Some(full) = g.preview_backup.take() {
                g.preview_files = full;
                sync_preview_store(&mut g);
            }
            if g.preview_files.is_empty() {
                g.message = idle_message(&g);
            } else {
                g.message = format!(
                    "已停止 · 预览仍保留 {} 张",
                    g.preview_files.len()
                );
            }
            // Parked movie pots keep running until user returns to 电影.
            return;
        }

        g.phase = SessionPhase::Stopping;
        g.message = "正在关闭本轮…".into();
        g.geometry_guard = false;
        g.tile_targets.clear();
        let pids: Vec<u32> = g.items.iter().map(|i| i.pid).collect();
        g.items.clear();
        // Also clear any stray parked (should be empty in movie mode).
        if let Some(parked) = g.parked_movie.take() {
            let _ = parked;
        }
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

    /// Recompute grid from live movie items **or** parked background pots.
    pub fn retile_now(&self) {
        let (pids, area) = {
            let mut g = self.inner.lock().unwrap();
            let pids: Vec<u32> = if g.phase == SessionPhase::Playing
                && g.config.media_mode == MediaMode::Movie
                && !g.items.is_empty()
            {
                g.items
                    .iter()
                    .map(|i| i.pid)
                    .filter(|p| *p != 0)
                    .collect()
            } else if let Some(ref parked) = g.parked_movie {
                parked
                    .items
                    .iter()
                    .map(|i| i.pid)
                    .filter(|p| *p != 0)
                    .collect()
            } else {
                return;
            };
            let mon = g.config.tile_monitor_index;
            let Ok(area) = tiler::resolve_work_area(mon) else {
                return;
            };
            let rects = tiler::grid_layout(pids.len(), area);
            g.tile_targets.clear();
            for (pid, rect) in pids.iter().zip(rects.iter()) {
                g.tile_targets.insert(*pid, *rect);
            }
            g.geometry_guard = true;
            (pids, area)
        };
        if pids.is_empty() {
            return;
        }
        thread::spawn(move || {
            let rects = tiler::grid_layout(pids.len(), area);
            let hwnds = potplayer::hwnds_aligned_to_pids(&pids, 12, 80);
            for (h, r) in hwnds.iter().zip(rects.iter()) {
                if *h != 0 {
                    let _ = tiler::place_window(*h, *r, true);
                }
            }
            thread::sleep(Duration::from_millis(200));
            let hwnds = potplayer::hwnds_aligned_to_pids(&pids, 6, 50);
            for (h, r) in hwnds.iter().zip(rects.iter()) {
                if *h != 0 {
                    let _ = tiler::place_window(*h, *r, true);
                }
            }
        });
    }

    /// Kill parked background movie players (e.g. from 图片 mode without switching back).
    pub fn stop_background_movie(&self) {
        let pids = {
            let mut g = self.inner.lock().unwrap();
            let Some(parked) = g.parked_movie.take() else {
                return;
            };
            let pids: Vec<u32> = parked.items.iter().map(|i| i.pid).collect();
            if g.config.media_mode == MediaMode::Image {
                g.message = idle_message(&g);
            }
            pids
        };
        if !pids.is_empty() {
            thread::spawn(move || {
                potplayer::kill_pids(&pids);
            });
        }
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
        if !g.config.close_session_on_exit {
            return;
        }
        let mut pids: Vec<u32> = g.items.iter().map(|i| i.pid).collect();
        if let Some(ref parked) = g.parked_movie {
            pids.extend(parked.items.iter().map(|i| i.pid));
        }
        drop(g);
        if !pids.is_empty() {
            potplayer::kill_pids(&pids);
        }
    }
}

fn sync_preview_store(g: &mut Inner) {
    match g.config.media_mode {
        MediaMode::Movie => g.movie_preview = g.preview_files.clone(),
        MediaMode::Image => g.image_preview = g.preview_files.clone(),
    }
}

fn preview_ready_message(g: &Inner) -> String {
    let n = g.preview_files.len();
    let unit = match g.config.media_mode {
        MediaMode::Movie => "部",
        MediaMode::Image => "张",
    };
    let action = match g.config.media_mode {
        MediaMode::Movie => "开启播放",
        MediaMode::Image => "开启幻灯",
    };
    let mut msg = format!("预览就绪 · {n} {unit} · 确认后点「{action}」");
    if let Some(ref p) = g.parked_movie {
        if g.config.media_mode == MediaMode::Image {
            msg.push_str(&format!(" · 电影后台 {} 部", p.items.len()));
        }
    }
    msg
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
        let n = g.scan_found.load(Ordering::Relaxed);
        return if n > 0 {
            format!("索引中… 已发现 {n} 个")
        } else {
            "索引中…".into()
        };
    }
    let bg = g
        .parked_movie
        .as_ref()
        .filter(|_| g.config.media_mode == MediaMode::Image)
        .map(|p| format!(" · 电影后台 {} 部", p.items.len()))
        .unwrap_or_default();

    if g.library().is_empty() {
        if g.config.roots_for(g.config.media_mode).is_empty() {
            format!(
                "请先设置{}库目录{bg}",
                g.config.media_mode.label()
            )
        } else {
            format!("{}库中未找到文件{bg}", g.config.media_mode.label())
        }
    } else if g.preview_files.is_empty() {
        format!(
            "就绪 · 已索引 {} {} · 先「随机预览」{bg}",
            g.library().len(),
            unit
        )
    } else {
        let action = match g.config.media_mode {
            MediaMode::Movie => "开启播放",
            MediaMode::Image => "开启幻灯",
        };
        format!(
            "预览就绪 · {} {} · 可「{action}」{bg}",
            g.preview_files.len(),
            unit
        )
    }
}

fn mode_switch_message(g: &Inner) -> String {
    idle_message(g)
}

fn run_start(handle: Arc<Mutex<Inner>>, gen: u64) {
    let still_valid = |h: &Mutex<Inner>| -> bool {
        let g = h.lock().unwrap();
        g.play_gen == gen
            && g.phase == SessionPhase::Starting
            && g.config.media_mode == MediaMode::Movie
    };

    let abort_kill = |h: &Mutex<Inner>, pids: &[u32], msg: &str| {
        if !pids.is_empty() {
            potplayer::kill_pids(pids);
        }
        let mut g = h.lock().unwrap();
        if g.play_gen == gen {
            g.phase = SessionPhase::Idle;
            g.message = msg.into();
        }
    };

    let (config, chosen) = {
        let mut g = handle.lock().unwrap();
        if g.play_gen != gen {
            return;
        }
        // Prefer one-shot queue (subset play); else full preview slate.
        let chosen = g
            .play_queue
            .take()
            .unwrap_or_else(|| g.preview_files.clone());
        (g.config.clone(), chosen)
    };

    if chosen.is_empty() {
        let mut g = handle.lock().unwrap();
        if g.play_gen == gen {
            g.phase = SessionPhase::Idle;
            g.play_queue = None;
            g.message = "请先「随机预览」生成片单".into();
        }
        return;
    }

    let pot = match potplayer::resolve_potplayer_path(&config.potplayer_path) {
        Some(p) => p,
        None => {
            // Fallback: open with OS default player (no tiling / no per-item control).
            if !still_valid(&handle) {
                return;
            }
            let (ok_n, errors) = potplayer::open_with_system_default(&chosen);
            let mut g = handle.lock().unwrap();
            if g.play_gen != gen {
                return;
            }
            g.phase = SessionPhase::Idle;
            g.last_errors = errors;
            if ok_n > 0 {
                let hist_size = g.config.recent_history_size;
                g.history_mut().push_many(&chosen, hist_size);
                let hist = g.history().clone();
                let mode = g.config.media_mode;
                save_history(mode, &hist);
                g.message = format!(
                    "已用系统默认打开 {ok_n} 部 · 无平铺 · 设置可装 PotPlayer"
                );
            } else {
                g.message = "无法打开：未找到 PotPlayer，且系统打开失败".into();
            }
            return;
        }
    };

    if !still_valid(&handle) {
        return;
    }

    // Root strategy: hide each window ASAP on create → place into grid while hidden → show.
    let (launched, mut hwnds, errors) = potplayer::launch_many_hidden(&pot, &chosen);

    if launched.is_empty() {
        let mut g = handle.lock().unwrap();
        if g.play_gen == gen {
            g.phase = SessionPhase::Idle;
            g.last_errors = errors;
            g.message = "启动 PotPlayer 失败".into();
        }
        return;
    }

    let pids: Vec<u32> = launched.iter().map(|i| i.pid).collect();
    let n = pids.len();

    if !still_valid(&handle) {
        abort_kill(&handle, &pids, "已取消开启");
        return;
    }

    for (i, pid) in pids.iter().enumerate() {
        if hwnds.get(i).copied().unwrap_or(0) == 0 {
            let h = potplayer::wait_single_hwnd(*pid, 2000);
            if let Some(slot) = hwnds.get_mut(i) {
                *slot = h;
            }
            if h != 0 {
                tiler::hide_window(h);
            }
        }
    }

    if !still_valid(&handle) {
        abort_kill(&handle, &pids, "已取消开启");
        return;
    }

    let mon = handle.lock().unwrap().config.tile_monitor_index;
    let Ok(area) = tiler::resolve_work_area(mon) else {
        abort_kill(&handle, &pids, "无法读取屏幕工作区");
        return;
    };
    let rects = tiler::grid_layout(n, area);

    for (h, r) in hwnds.iter().zip(rects.iter()) {
        if *h != 0 {
            let _ = tiler::place_window(*h, *r, false);
        }
    }
    thread::sleep(Duration::from_millis(80));
    if !still_valid(&handle) {
        abort_kill(&handle, &pids, "已取消开启");
        return;
    }
    for (h, r) in hwnds.iter().zip(rects.iter()) {
        if *h != 0 {
            let _ = tiler::place_window(*h, *r, true);
        }
    }
    thread::sleep(Duration::from_millis(200));
    if !still_valid(&handle) {
        abort_kill(&handle, &pids, "已取消开启");
        return;
    }
    for (h, r) in hwnds.iter().zip(rects.iter()) {
        if *h != 0 {
            let _ = tiler::place_window(*h, *r, true);
        }
    }

    // Commit only if this generation is still the active start
    let pin_after = {
        let mut g = handle.lock().unwrap();
        if g.play_gen != gen
            || g.phase != SessionPhase::Starting
            || g.config.media_mode != MediaMode::Movie
        {
            drop(g);
            potplayer::kill_pids(&pids);
            return;
        }
        let paths: Vec<PathBuf> = launched.iter().map(|i| i.path.clone()).collect();
        let hist_size = g.config.recent_history_size;
        let mode = g.config.media_mode;
        let pin = g.config.pin_while_playing;
        g.history_mut().push_many(&paths, hist_size);
        let hist = g.history().clone();
        save_history(mode, &hist);
        g.preview_files = paths.clone();
        sync_preview_store(&mut g);
        g.items = launched;
        g.phase = SessionPhase::Playing;
        g.last_errors = errors;
        g.tile_targets.clear();
        for (pid, rect) in pids.iter().zip(rects.iter()) {
            g.tile_targets.insert(*pid, *rect);
        }
        g.geometry_guard = true;
        let mut msg = format!("播放中 · {} 部", g.items.len());
        if !g.last_errors.is_empty() {
            msg.push_str(" · 部分失败");
        }
        g.message = msg;
        pin
    };

    let handle_guard = handle.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(140));
            let (guard_on, targets, should_exit): (bool, Vec<(u32, tiler::Rect)>, bool) = {
                let g = handle_guard.lock().unwrap();
                let pots_live = (g.phase == SessionPhase::Playing
                    && g.config.media_mode == MediaMode::Movie
                    && !g.items.is_empty())
                    || g.parked_movie.is_some();
                if !pots_live {
                    (false, Vec::new(), true)
                } else if !g.geometry_guard || g.tile_targets.is_empty() {
                    (false, Vec::new(), false)
                } else {
                    (
                        true,
                        g.tile_targets.iter().map(|(p, r)| (*p, *r)).collect(),
                        false,
                    )
                }
            };
            if should_exit {
                break;
            }
            if !guard_on {
                continue;
            }
            let pids: Vec<u32> = targets.iter().map(|(p, _)| *p).collect();
            let hwnds = potplayer::hwnds_aligned_to_pids(&pids, 3, 30);
            for (i, (_pid, rect)) in targets.iter().enumerate() {
                let h = hwnds.get(i).copied().unwrap_or(0);
                if h != 0 && !tiler::matches_target(h, *rect, 12) {
                    let _ = tiler::place_window(h, *rect, true);
                }
            }
        }
    });

    // Raise panel; pin only if user still wants it
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(450));
        if pin_after {
            crate::tray::force_show_and_pin();
        } else {
            crate::tray::force_show_main_window();
        }
        thread::sleep(Duration::from_millis(600));
        if pin_after {
            crate::tray::force_show_and_pin();
        } else {
            crate::tray::force_show_main_window();
        }
    });
}

fn file_stem(p: &std::path::Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}



