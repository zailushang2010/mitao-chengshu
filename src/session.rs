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
pub struct SessionSnapshot {
    pub phase: SessionPhase,
    pub message: String,
    pub current_files: Vec<PathBuf>,
    pub library_count: usize,
    pub library_root: String,
    pub last_errors: Vec<String>,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            phase: SessionPhase::Idle,
            message: "就绪".into(),
            current_files: Vec::new(),
            library_count: 0,
            library_root: String::new(),
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
        let library = if config.library_path.is_empty() {
            Library::empty()
        } else {
            Library::scan(&config.library_path, &config.video_extensions)
        };
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
        SessionSnapshot {
            phase: g.phase,
            message: g.message.clone(),
            current_files: g.items.iter().map(|i| i.path.clone()).collect(),
            library_count: g.library.len(),
            library_root: g.config.library_path.clone(),
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

    pub fn update_library_path(&self, path: String) {
        let mut g = self.inner.lock().unwrap();
        g.config.library_path = path.clone();
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
        let path = g.config.library_path.clone();
        let exts = g.config.video_extensions.clone();
        g.message = "索引中…".into();
        drop(g);

        let lib = if path.is_empty() {
            Library::empty()
        } else {
            Library::scan(&path, &exts)
        };

        let mut g = self.inner.lock().unwrap();
        g.library = lib;
        g.message = if g.library.is_empty() {
            if path.is_empty() {
                "请先设置片库目录".into()
            } else {
                "片库中未找到视频".into()
            }
        } else {
            format!("就绪 · 已索引 {} 部", g.library.len())
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

    // Tile windows
    let pids: Vec<u32> = launched.iter().map(|i| i.pid).collect();
    let hwnd_pairs = potplayer::find_hwnds_for_pids(&pids, 12, 200);
    if let Ok(area) = tiler::work_area() {
        let rects = tiler::grid_layout(hwnd_pairs.len().max(1), area);
        // Map hwnd order to rects by launch order when possible
        let mut hwnds_ordered = Vec::new();
        for item in &launched {
            if let Some((_, h)) = hwnd_pairs.iter().find(|(p, _)| *p == item.pid) {
                hwnds_ordered.push(*h);
            }
        }
        if !hwnds_ordered.is_empty() {
            let rects = tiler::grid_layout(hwnds_ordered.len(), area);
            tiler::tile_hwnds(&hwnds_ordered, &rects);
        } else {
            let hwnds: Vec<isize> = hwnd_pairs.iter().map(|(_, h)| *h).collect();
            tiler::tile_hwnds(&hwnds, &rects);
        }
    }

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
}
