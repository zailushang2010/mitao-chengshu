use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaMode {
    #[default]
    Movie,
    Image,
}

impl MediaMode {
    pub fn label(self) -> &'static str {
        match self {
            MediaMode::Movie => "电影",
            MediaMode::Image => "图片",
        }
    }
}

fn default_slideshow_interval() -> u8 {
    5
}

fn default_volume_percent() -> u8 {
    28
}

fn default_image_default_count() -> usize {
    9
}
fn default_image_count_min() -> usize {
    1
}
fn default_image_count_max() -> usize {
    24
}

/// How images open after 开启幻灯/开启.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImagePlayStyle {
    /// Fullscreen timed slideshow
    #[default]
    Slideshow,
    /// All preview images tiled on one wall
    Wall,
}

impl ImagePlayStyle {
    pub fn label(self) -> &'static str {
        match self {
            ImagePlayStyle::Slideshow => "幻灯片",
            ImagePlayStyle::Wall => "平铺墙",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Legacy single path (kept for old configs / backward compatibility).
    #[serde(default)]
    pub library_path: String,
    /// One or more movie roots (recursive). Preferred field.
    #[serde(default)]
    pub library_paths: Vec<String>,
    /// Image library roots (separate from movies).
    #[serde(default)]
    pub image_library_paths: Vec<String>,
    #[serde(default)]
    pub media_mode: MediaMode,
    /// Movie: how many to pick per round (legacy field names kept).
    #[serde(default = "default_movie_default_count")]
    pub default_count: usize,
    #[serde(default = "default_movie_count_min")]
    pub count_min: usize,
    #[serde(default = "default_movie_count_max")]
    pub count_max: usize,
    /// Image: separate pick count — not shared with movies.
    #[serde(default = "default_image_default_count")]
    pub image_default_count: usize,
    #[serde(default = "default_image_count_min")]
    pub image_count_min: usize,
    #[serde(default = "default_image_count_max")]
    pub image_count_max: usize,
    /// Kept for old config.json compatibility only; not used by UI.
    #[serde(default = "default_volume_percent")]
    pub volume_percent: u8,
    #[serde(default = "default_avoid_recent")]
    pub avoid_recent: bool,
    #[serde(default = "default_recent_history_size")]
    pub recent_history_size: usize,
    #[serde(default)]
    pub potplayer_path: String,
    #[serde(default = "default_video_extensions")]
    pub video_extensions: Vec<String>,
    #[serde(default = "default_image_extensions")]
    pub image_extensions: Vec<String>,
    /// Slideshow seconds per image (1–60).
    #[serde(default = "default_slideshow_interval")]
    pub slideshow_interval_secs: u8,
    /// Slideshow or tile wall for image mode.
    #[serde(default)]
    pub image_play_style: ImagePlayStyle,
    #[serde(default)]
    pub close_session_on_exit: bool,
    /// If true, title-bar close (X) hides to tray instead of quitting.
    #[serde(default)]
    pub minimize_to_tray: bool,
    /// Which display to tile PotPlayer windows onto.
    /// `-1` = system primary work area (SPI); `0..` = `tiler::list_monitors()` index.
    #[serde(default = "default_tile_monitor_index")]
    pub tile_monitor_index: i32,
    /// Workbench ops rail open (persisted across launches).
    #[serde(default = "default_workbench_sidebar_open")]
    pub workbench_sidebar_open: bool,
    /// Movie preview card columns (2–5), remembered across launches.
    #[serde(default = "default_card_cols")]
    pub card_cols: u8,
    /// Image preview card columns (2–5), independent of movie.
    #[serde(default = "default_card_cols")]
    pub image_card_cols: u8,
    /// Last main window size/position (points). Absent → default + center.
    #[serde(default)]
    pub window_geometry: Option<WindowGeometry>,
    /// Keep control panel above PotPlayers while movie session is active.
    #[serde(default = "default_pin_while_playing")]
    pub pin_while_playing: bool,
}

/// Persisted main-window placement (egui points, virtual-desktop space).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    /// Outer top-left X (monitor / virtual desktop).
    pub x: f32,
    /// Outer top-left Y.
    pub y: f32,
    /// Inner client width.
    pub w: f32,
    /// Inner client height.
    pub h: f32,
}

impl WindowGeometry {
    pub const MIN_W: f32 = 900.0;
    pub const MIN_H: f32 = 560.0;
    pub const MAX_W: f32 = 1920.0;
    pub const MAX_H: f32 = 1200.0;
    pub const DEFAULT_W: f32 = 1200.0;
    pub const DEFAULT_H: f32 = 780.0;

    pub fn clamp_size(self) -> Self {
        Self {
            x: self.x,
            y: self.y,
            w: self.w.clamp(Self::MIN_W, Self::MAX_W),
            h: self.h.clamp(Self::MIN_H, Self::MAX_H),
        }
    }
}

fn default_tile_monitor_index() -> i32 {
    -1
}

fn default_movie_default_count() -> usize {
    6
}
fn default_movie_count_min() -> usize {
    1
}
fn default_movie_count_max() -> usize {
    16
}
fn default_avoid_recent() -> bool {
    true
}
fn default_recent_history_size() -> usize {
    40
}
fn default_video_extensions() -> Vec<String> {
    vec![
        ".mkv".into(),
        ".mp4".into(),
        ".avi".into(),
        ".ts".into(),
        ".m2ts".into(),
        ".wmv".into(),
        ".mov".into(),
        ".flv".into(),
        ".webm".into(),
    ]
}

fn default_workbench_sidebar_open() -> bool {
    true
}

fn default_card_cols() -> u8 {
    3
}

fn default_pin_while_playing() -> bool {
    true
}

fn default_image_extensions() -> Vec<String> {
    vec![
        ".jpg".into(),
        ".jpeg".into(),
        ".png".into(),
        ".webp".into(),
        ".bmp".into(),
        ".gif".into(),
        ".tif".into(),
        ".tiff".into(),
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: String::new(),
            library_paths: Vec::new(),
            image_library_paths: Vec::new(),
            media_mode: MediaMode::Movie,
            default_count: default_movie_default_count(),
            count_min: default_movie_count_min(),
            count_max: default_movie_count_max(),
            image_default_count: default_image_default_count(),
            image_count_min: default_image_count_min(),
            image_count_max: default_image_count_max(),
            volume_percent: 28,
            avoid_recent: true,
            recent_history_size: 40,
            potplayer_path: String::new(),
            video_extensions: default_video_extensions(),
            image_extensions: default_image_extensions(),
            slideshow_interval_secs: 5,
            image_play_style: ImagePlayStyle::Slideshow,
            close_session_on_exit: false,
            minimize_to_tray: false,
            tile_monitor_index: default_tile_monitor_index(),
            workbench_sidebar_open: default_workbench_sidebar_open(),
            card_cols: default_card_cols(),
            image_card_cols: default_card_cols(),
            window_geometry: None,
            pin_while_playing: default_pin_while_playing(),
        }
    }
}

impl Config {
    pub fn card_cols_for(&self, mode: MediaMode) -> u8 {
        match mode {
            MediaMode::Movie => self.card_cols.clamp(2, 5),
            MediaMode::Image => self.image_card_cols.clamp(2, 5),
        }
    }

    pub fn set_card_cols_for(&mut self, mode: MediaMode, cols: u8) {
        let cols = cols.clamp(2, 5);
        match mode {
            MediaMode::Movie => self.card_cols = cols,
            MediaMode::Image => self.image_card_cols = cols,
        }
    }

    /// Clamp against the **current** media mode's min/max.
    pub fn clamp_count(&self, n: usize) -> usize {
        self.clamp_count_for(self.media_mode, n)
    }

    pub fn clamp_count_for(&self, mode: MediaMode, n: usize) -> usize {
        let (lo, hi) = self.count_bounds_for(mode);
        n.clamp(lo, hi)
    }

    pub fn count_bounds_for(&self, mode: MediaMode) -> (usize, usize) {
        match mode {
            MediaMode::Movie => (self.count_min, self.count_max),
            MediaMode::Image => (self.image_count_min, self.image_count_max),
        }
    }

    pub fn default_count_for(&self, mode: MediaMode) -> usize {
        match mode {
            MediaMode::Movie => self.default_count,
            MediaMode::Image => self.image_default_count,
        }
    }

    pub fn set_default_count_for(&mut self, mode: MediaMode, n: usize) {
        let n = self.clamp_count_for(mode, n);
        match mode {
            MediaMode::Movie => self.default_count = n,
            MediaMode::Image => self.image_default_count = n,
        }
    }

    /// Movie roots (legacy name kept).
    pub fn library_roots(&self) -> Vec<String> {
        self.roots_for(MediaMode::Movie)
    }

    pub fn roots_for(&self, mode: MediaMode) -> Vec<String> {
        let list = match mode {
            MediaMode::Movie => &self.library_paths,
            MediaMode::Image => &self.image_library_paths,
        };
        list.iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn extensions_for(&self, mode: MediaMode) -> &[String] {
        match mode {
            MediaMode::Movie => &self.video_extensions,
            MediaMode::Image => &self.image_extensions,
        }
    }

    pub fn has_library(&self) -> bool {
        !self.roots_for(self.media_mode).is_empty()
    }

    /// Short label for UI header (current media mode).
    pub fn library_label(&self) -> String {
        self.library_label_for(self.media_mode)
    }

    pub fn library_label_for(&self, mode: MediaMode) -> String {
        let roots = self.roots_for(mode);
        match roots.len() {
            0 => String::new(),
            1 => roots[0].clone(),
            n => format!("{} 等 {} 个目录", short_name(&roots[0]), n),
        }
    }

    /// Absolute safety caps (prevents absurd process storms).
    pub const ABS_COUNT_MIN: usize = 1;
    pub const ABS_COUNT_MAX: usize = 32;

    pub fn set_count_min(&mut self, n: usize) {
        self.set_count_min_for(self.media_mode, n);
    }

    pub fn set_count_max(&mut self, n: usize) {
        self.set_count_max_for(self.media_mode, n);
    }

    pub fn set_count_min_for(&mut self, mode: MediaMode, n: usize) {
        let n = n.clamp(Self::ABS_COUNT_MIN, Self::ABS_COUNT_MAX);
        match mode {
            MediaMode::Movie => {
                self.count_min = n;
                if self.count_max < self.count_min {
                    self.count_max = self.count_min;
                }
                self.default_count = self
                    .default_count
                    .clamp(self.count_min, self.count_max);
            }
            MediaMode::Image => {
                self.image_count_min = n;
                if self.image_count_max < self.image_count_min {
                    self.image_count_max = self.image_count_min;
                }
                self.image_default_count = self
                    .image_default_count
                    .clamp(self.image_count_min, self.image_count_max);
            }
        }
    }

    pub fn set_count_max_for(&mut self, mode: MediaMode, n: usize) {
        let n = n.clamp(Self::ABS_COUNT_MIN, Self::ABS_COUNT_MAX);
        match mode {
            MediaMode::Movie => {
                self.count_max = n;
                if self.count_min > self.count_max {
                    self.count_min = self.count_max;
                }
                self.default_count = self
                    .default_count
                    .clamp(self.count_min, self.count_max);
            }
            MediaMode::Image => {
                self.image_count_max = n;
                if self.image_count_min > self.image_count_max {
                    self.image_count_min = self.image_count_max;
                }
                self.image_default_count = self
                    .image_default_count
                    .clamp(self.image_count_min, self.image_count_max);
            }
        }
    }

    pub fn normalize(mut self) -> Self {
        fn clamp_triple(min: &mut usize, max: &mut usize, def: &mut usize) {
            *min = (*min).clamp(Config::ABS_COUNT_MIN, Config::ABS_COUNT_MAX);
            *max = (*max).clamp(Config::ABS_COUNT_MIN, Config::ABS_COUNT_MAX);
            if *max < *min {
                *max = *min;
            }
            *def = (*def).clamp(*min, *max);
        }
        clamp_triple(
            &mut self.count_min,
            &mut self.count_max,
            &mut self.default_count,
        );
        clamp_triple(
            &mut self.image_count_min,
            &mut self.image_count_max,
            &mut self.image_default_count,
        );
        self.volume_percent = self.volume_percent.min(100);
        normalize_ext_list(&mut self.video_extensions);
        normalize_ext_list(&mut self.image_extensions);
        self.slideshow_interval_secs = self.slideshow_interval_secs.clamp(1, 60);
        self.card_cols = self.card_cols.clamp(2, 5);
        self.image_card_cols = self.image_card_cols.clamp(2, 5);
        if let Some(g) = self.window_geometry {
            let g = g.clamp_size();
            self.window_geometry = if g.w.is_finite() && g.h.is_finite() {
                Some(g)
            } else {
                None
            };
        }

        // library_paths is authoritative. Only migrate legacy library_path when
        // library_paths is empty (old configs). Never re-insert a removed path.
        self.library_paths = dedupe_paths(std::mem::take(&mut self.library_paths), true);
        if self.library_paths.is_empty() {
            let legacy = self.library_path.trim().to_string();
            if !legacy.is_empty() {
                self.library_paths.push(legacy);
                self.library_paths = dedupe_paths(std::mem::take(&mut self.library_paths), false);
            }
        }
        self.library_path = self
            .library_paths
            .first()
            .cloned()
            .unwrap_or_default();

        self.image_library_paths =
            dedupe_paths(std::mem::take(&mut self.image_library_paths), false);
        self
    }

    pub fn add_library_path(&mut self, path: String) {
        self.add_path_for(self.media_mode, path);
    }

    pub fn add_path_for(&mut self, mode: MediaMode, path: String) {
        let path = path.trim().to_string();
        if path.is_empty() {
            return;
        }
        let list = match mode {
            MediaMode::Movie => &mut self.library_paths,
            MediaMode::Image => &mut self.image_library_paths,
        };
        if list.iter().any(|p| path_key(p) == path_key(&path)) {
            return;
        }
        list.push(path);
        *self = self.clone().normalize();
    }

    /// Returns the removed path when successful.
    pub fn remove_library_path(&mut self, index: usize) -> Option<String> {
        self.remove_path_for(self.media_mode, index)
    }

    pub fn remove_path_for(&mut self, mode: MediaMode, index: usize) -> Option<String> {
        let list = match mode {
            MediaMode::Movie => &mut self.library_paths,
            MediaMode::Image => &mut self.image_library_paths,
        };
        if index >= list.len() {
            return None;
        }
        let removed = list.remove(index);
        // Clear legacy field before normalize, or empty library_paths would
        // re-migrate the just-removed path from library_path.
        if matches!(mode, MediaMode::Movie) && self.library_paths.is_empty() {
            self.library_path.clear();
        }
        *self = self.clone().normalize();
        Some(removed)
    }
}

fn normalize_ext_list(exts: &mut Vec<String>) {
    for ext in exts {
        let lower = ext.to_ascii_lowercase();
        *ext = if lower.starts_with('.') {
            lower
        } else {
            format!(".{lower}")
        };
    }
}

fn dedupe_paths(mut paths: Vec<String>, _movie: bool) -> Vec<String> {
    paths = paths
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut seen = std::collections::HashSet::new();
    paths.retain(|p| seen.insert(path_key(p)));
    paths
}

fn path_key(p: &str) -> String {
    let s = p.trim().trim_end_matches(['/', '\\']).to_string();
    #[cfg(windows)]
    {
        s.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        s
    }
}

fn short_name(p: &str) -> String {
    Path::new(p)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| p.to_string())
}

pub fn app_data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    app_data_dir().join("config.json")
}

pub fn load_or_default() -> Config {
    let path = config_path();
    load_from(&path).unwrap_or_default().normalize()
}

pub fn load_from(path: &Path) -> Option<Config> {
    // Read as bytes then UTF-8 to tolerate BOM from some editors
    let bytes = fs::read(path).ok()?;
    let raw = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    match serde_json::from_str::<Config>(&raw) {
        Ok(c) => Some(c.normalize()),
        Err(_) => {
            let bak = path.with_extension("json.bak");
            let _ = fs::copy(path, bak);
            None
        }
    }
}

pub fn save(config: &Config) -> Result<(), String> {
    let path = config_path();
    save_to(&path, config)
}

pub fn save_to(path: &Path, config: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("suiji_cfg_{nanos}_{name}"))
    }

    #[test]
    fn clamp_count_respects_bounds() {
        let c = Config::default();
        assert_eq!(c.clamp_count(1), 1);
        assert_eq!(c.clamp_count(6), 6);
        assert_eq!(c.clamp_count(99), 16);
    }

    #[test]
    fn image_and_movie_counts_are_independent() {
        let mut c = Config::default();
        c.media_mode = MediaMode::Movie;
        c.set_count_min_for(MediaMode::Movie, 2);
        c.set_count_max_for(MediaMode::Movie, 8);
        c.set_default_count_for(MediaMode::Movie, 5);

        c.set_count_min_for(MediaMode::Image, 4);
        c.set_count_max_for(MediaMode::Image, 20);
        c.set_default_count_for(MediaMode::Image, 12);

        assert_eq!(c.count_bounds_for(MediaMode::Movie), (2, 8));
        assert_eq!(c.default_count_for(MediaMode::Movie), 5);
        assert_eq!(c.count_bounds_for(MediaMode::Image), (4, 20));
        assert_eq!(c.default_count_for(MediaMode::Image), 12);

        // Movie clamp must not use image bounds
        c.media_mode = MediaMode::Movie;
        assert_eq!(c.clamp_count(99), 8);
        c.media_mode = MediaMode::Image;
        assert_eq!(c.clamp_count(99), 20);
    }

    #[test]
    fn legacy_library_path_merges_when_paths_empty() {
        let mut c = Config::default();
        c.library_path = r"D:\Movies".into();
        c.library_paths = vec![];
        let c = c.normalize();
        assert_eq!(c.library_roots(), vec![r"D:\Movies".to_string()]);
    }

    #[test]
    fn library_paths_authoritative_over_legacy() {
        let mut c = Config::default();
        c.library_path = r"D:\Movies".into();
        c.library_paths = vec![r"E:\More".into()];
        let c = c.normalize();
        // When library_paths is non-empty, legacy field is not re-merged
        assert_eq!(c.library_roots().len(), 1);
        assert!(c.library_roots()[0].contains("More"));
        assert_eq!(c.library_path, c.library_roots()[0]);
    }

    #[test]
    fn add_path_dedupes_case_insensitive() {
        let mut c = Config::default();
        c.add_library_path(r"D:\Movies".into());
        c.add_library_path(r"d:\movies".into());
        assert_eq!(c.library_roots().len(), 1);
    }

    #[test]
    fn remove_does_not_resurrect_via_legacy_field() {
        let mut c = Config::default();
        c.add_library_path(r"F:\电影".into());
        assert_eq!(c.library_roots().len(), 1);
        let removed = c.remove_library_path(0);
        assert_eq!(removed.as_deref(), Some(r"F:\电影"));
        assert!(c.library_roots().is_empty(), "must stay empty after remove");
        assert!(c.library_path.is_empty());
    }

    #[test]
    fn corrupt_json_returns_none() {
        let path = tmp_file("bad.json");
        fs::write(&path, "{not json").unwrap();
        assert!(load_from(&path).is_none());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn save_and_load_roundtrip_multi() {
        let path = tmp_file("ok.json");
        let mut c = Config::default();
        c.library_paths = vec![r"D:\Movies".into(), r"F:\电影".into()];
        c.default_count = 8;
        let c = c.normalize();
        save_to(&path, &c).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.library_roots().len(), 2);
        assert_eq!(loaded.default_count, 8);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn workbench_sidebar_defaults_open_and_roundtrips() {
        let path = tmp_file("sidebar.json");
        let mut c = Config::default();
        assert!(c.workbench_sidebar_open);
        c.workbench_sidebar_open = false;
        save_to(&path, &c).unwrap();
        let loaded = load_from(&path).unwrap();
        assert!(!loaded.workbench_sidebar_open);
        // Missing field → default true
        let raw = r#"{"default_count":4,"count_min":1,"count_max":8,"avoid_recent":true,"recent_history_size":10,"potplayer_path":"","video_extensions":[],"close_session_on_exit":false}"#;
        fs::write(&path, raw).unwrap();
        let legacy = load_from(&path).unwrap();
        assert!(legacy.workbench_sidebar_open);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn card_cols_defaults_and_roundtrips() {
        let path = tmp_file("card_cols.json");
        let mut c = Config::default();
        assert_eq!(c.card_cols, 3);
        assert_eq!(c.image_card_cols, 3);
        c.card_cols = 5;
        c.image_card_cols = 2;
        let c = c.normalize();
        save_to(&path, &c).unwrap();
        let loaded = load_from(&path).unwrap().normalize();
        assert_eq!(loaded.card_cols, 5);
        assert_eq!(loaded.image_card_cols, 2);
        // Out of range clamped
        let mut bad = Config::default();
        bad.card_cols = 9;
        bad.image_card_cols = 1;
        let bad = bad.normalize();
        assert_eq!(bad.card_cols, 5);
        assert_eq!(bad.image_card_cols, 2);
        // Missing field → default 3
        let raw = r#"{"default_count":4,"count_min":1,"count_max":8,"avoid_recent":true,"recent_history_size":10,"potplayer_path":"","video_extensions":[],"close_session_on_exit":false}"#;
        fs::write(&path, raw).unwrap();
        let legacy = load_from(&path).unwrap();
        assert_eq!(legacy.card_cols, 3);
        assert_eq!(legacy.image_card_cols, 3);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn window_geometry_roundtrip_and_clamp() {
        let path = tmp_file("win_geom.json");
        let mut c = Config::default();
        assert!(c.window_geometry.is_none());
        c.window_geometry = Some(WindowGeometry {
            x: 100.0,
            y: 80.0,
            w: 1400.0,
            h: 900.0,
        });
        save_to(&path, &c).unwrap();
        let loaded = load_from(&path).unwrap().normalize();
        let g = loaded.window_geometry.expect("geom");
        assert!((g.w - 1400.0).abs() < 0.5);
        assert!((g.x - 100.0).abs() < 0.5);
        let mut huge = Config::default();
        huge.window_geometry = Some(WindowGeometry {
            x: 0.0,
            y: 0.0,
            w: 9999.0,
            h: 20.0,
        });
        let g = huge.normalize().window_geometry.unwrap();
        assert_eq!(g.w, WindowGeometry::MAX_W);
        assert_eq!(g.h, WindowGeometry::MIN_H);
        let _ = fs::remove_file(&path);
    }
}
