use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::Config;
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
    pub current_files: Vec<PathBuf>,
    pub items: Vec<SessionItemView>,
    pub library_count: usize,
    /// Short UI label (1 path or "A 等 N 个目录")
    #[allow(dead_code)]
    pub library_root: String,
    /// Full list of configured roots
    pub library_roots: Vec<String>,
    pub last_errors: Vec<String>,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            phase: SessionPhase::Idle,
            message: "就绪".into(),
            current_files: Vec::new(),
            items: Vec::new(),
            library_count: 0,
            library_root: String::new(),
            library_roots: Vec::new(),
            last_errors: Vec::new(),
        }
    }
}

struct Inner {
    config: Config,
    library: Library,
    history: History,
    phase: SessionPhase,
    items: Vec<LaunchedItem>,
    message: String,
    last_errors: Vec<String>,
    ui_count: usize,
}

pub struct SessionHandle {
    inner: Arc<Mutex<Inner>>,
}

impl SessionHandle {
    pub fn new(config: Config) -> Self {
        let history = History::load();
        let roots = config.library_roots();
        let library = scan_config_roots(&roots, &config.video_extensions);
        let ui_count = config.default_count;
        let inner = Inner {
            config,
            library,
            history,
            phase: SessionPhase::Idle,
            items: Vec::new(),
            message: "就绪".into(),
            last_errors: Vec::new(),
            ui_count,
        };
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
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
        SessionSnapshot {
            phase: g.phase,
            message: g.message.clone(),
            current_files: items.iter().map(|i| i.path.clone()).collect(),
            items,
            library_count: g.library.len(),
            library_root: g.config.library_label(),
            library_roots: g.config.library_roots(),
            last_errors: g.last_errors.clone(),
        }
    }

    pub fn config_clone(&self) -> Config {
        self.inner.lock().unwrap().config.clone()
    }

    pub fn ui_count(&self) -> usize {
        self.inner.lock().unwrap().ui_count
    }

    pub fn set_ui_count(&self, n: usize) {
        let mut g = self.inner.lock().unwrap();
        g.ui_count = g.config.clamp_count(n);
        g.config.default_count = g.ui_count;
        let _ = crate::config::save(&g.config);
    }

    pub fn set_count_bounds(&self, min: usize, max: usize) {
        let mut g = self.inner.lock().unwrap();
        g.config.set_count_min(min);
        g.config.set_count_max(max);
        g.ui_count = g.config.clamp_count(g.ui_count);
        g.config.default_count = g.ui_count;
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
                g.message = format!("就绪 · 已索引 {} 部", g.library.len());
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
        let mut g = self.inner.lock().unwrap();
        g.config.library_paths = if path.trim().is_empty() {
            Vec::new()
        } else {
            vec![path]
        };
        g.config = g.config.clone().normalize();
        let _ = crate::config::save(&g.config);
        g.message = "索引中…".into();
        drop(g);
        self.rescan();
    }

    pub fn add_library_path(&self, path: String) {
        let mut g = self.inner.lock().unwrap();
        g.config.add_library_path(path);
        let _ = crate::config::save(&g.config);
        g.message = "索引中…".into();
        drop(g);
        self.rescan();
    }

    pub fn remove_library_path(&self, index: usize) {
        let mut g = self.inner.lock().unwrap();
        g.config.remove_library_path(index);
        let _ = crate::config::save(&g.config);
        g.message = "索引中…".into();
        drop(g);
        self.rescan();
    }

    pub fn set_potplayer_path(&self, path: String) {
        let mut g = self.inner.lock().unwrap();
        g.config.potplayer_path = path;
        let _ = crate::config::save(&g.config);
    }

    pub fn rescan(&self) {
        let mut g = self.inner.lock().unwrap();
        let roots = g.config.library_roots();
        let exts = g.config.video_extensions.clone();
        g.message = "索引中…".into();
        drop(g);

        let lib = scan_config_roots(&roots, &exts);

        let mut g = self.inner.lock().unwrap();
        g.library = lib;
        g.message = if g.library.is_empty() {
            if roots.is_empty() {
                "请先设置片库目录".into()
            } else {
                "片库中未找到视频".into()
            }
        } else {
            let n_dirs = roots.len();
            if n_dirs > 1 {
                format!(
                    "就绪 · {} 个目录 · 已索引 {} 部",
                    n_dirs,
                    g.library.len()
                )
            } else {
                format!("就绪 · 已索引 {} 部", g.library.len())
            }
        };
    }

    pub fn start(&self) {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Idle {
            return;
        }
        g.phase = SessionPhase::Starting;
        g.message = "正在开启本轮…".into();
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
            g.message = format!("就绪 · 已索引 {} 部", g.library.len());
        });
    }

    pub fn reroll(&self) {
        let mut g = self.inner.lock().unwrap();
        if g.phase != SessionPhase::Playing && g.phase != SessionPhase::Idle {
            return;
        }
        let pids: Vec<u32> = g.items.iter().map(|i| i.pid).collect();
        g.phase = SessionPhase::Stopping;
        g.message = "换一桌中…".into();
        g.items.clear();
        drop(g);

        let handle = self.inner.clone();
        thread::spawn(move || {
            if !pids.is_empty() {
                potplayer::kill_pids(&pids);
                thread::sleep(std::time::Duration::from_millis(300));
            }
            {
                let mut g = handle.lock().unwrap();
                g.phase = SessionPhase::Starting;
                g.message = "正在开启本轮…".into();
                g.last_errors.clear();
            }
            run_start(handle);
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

fn run_start(handle: Arc<Mutex<Inner>>) {
    let (config, library_files, ui_count, history_paths, avoid) = {
        let g = handle.lock().unwrap();
        (
            g.config.clone(),
            g.library.files.clone(),
            g.ui_count,
            g.history.as_path_set(),
            g.config.avoid_recent,
        )
    };

    if library_files.is_empty() {
        let mut g = handle.lock().unwrap();
        g.phase = SessionPhase::Idle;
        g.message = "片库为空，无法开启".into();
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

    let n = config.clamp_count(ui_count);
    let chosen = picker::pick(&library_files, n, avoid, &history_paths);
    if chosen.is_empty() {
        let mut g = handle.lock().unwrap();
        g.phase = SessionPhase::Idle;
        g.message = "未能选出影片".into();
        return;
    }

    let shortfall = n > chosen.len();
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
        g.history.push_many(&paths, hist_size);
        let _ = g.history.save();
        g.items = launched;
        g.phase = SessionPhase::Playing;
        g.last_errors = errors;
        let mut msg = format!("播放中 · {} 部", g.items.len());
        if shortfall {
            msg.push_str("（片源不足，已全部开出）");
        }
        if !g.last_errors.is_empty() {
            msg.push_str(" · 部分失败");
        }
        g.message = msg;
    }

    // PotPlayer steals focus — pull control panel back after tiling settles
    thread::spawn(|| {
        for delay in [200u64, 600, 1200, 2000] {
            thread::sleep(std::time::Duration::from_millis(delay));
            crate::tray::force_show_main_window();
            crate::tray::set_main_window_topmost(true);
        }
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
