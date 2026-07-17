use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Legacy single path (kept for old configs / backward compatibility).
    #[serde(default)]
    pub library_path: String,
    /// One or more movie roots (recursive). Preferred field.
    #[serde(default)]
    pub library_paths: Vec<String>,
    pub default_count: usize,
    pub count_min: usize,
    pub count_max: usize,
    pub volume_percent: u8,
    pub avoid_recent: bool,
    pub recent_history_size: usize,
    pub potplayer_path: String,
    pub video_extensions: Vec<String>,
    pub close_session_on_exit: bool,
    /// If true, title-bar close (X) hides to tray instead of quitting.
    /// Default false: X exits the app; use the tray icon button to hide intentionally.
    #[serde(default)]
    pub minimize_to_tray: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: String::new(),
            library_paths: Vec::new(),
            default_count: 6,
            // Soft defaults; both limits are user-editable in settings.
            count_min: 1,
            count_max: 16,
            volume_percent: 28,
            avoid_recent: true,
            recent_history_size: 40,
            potplayer_path: String::new(),
            video_extensions: vec![
                ".mkv".into(),
                ".mp4".into(),
                ".avi".into(),
                ".ts".into(),
                ".m2ts".into(),
                ".wmv".into(),
                ".mov".into(),
                ".flv".into(),
                ".webm".into(),
            ],
            close_session_on_exit: false,
            minimize_to_tray: false,
        }
    }
}

impl Config {
    pub fn clamp_count(&self, n: usize) -> usize {
        n.clamp(self.count_min, self.count_max)
    }

    /// All configured roots after normalize (non-empty unique paths).
    pub fn library_roots(&self) -> Vec<String> {
        self.library_paths
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn has_library(&self) -> bool {
        !self.library_roots().is_empty()
    }

    /// Short label for UI header.
    pub fn library_label(&self) -> String {
        let roots = self.library_roots();
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
        self.count_min = n.clamp(Self::ABS_COUNT_MIN, Self::ABS_COUNT_MAX);
        if self.count_max < self.count_min {
            self.count_max = self.count_min;
        }
        self.default_count = self.default_count.clamp(self.count_min, self.count_max);
    }

    pub fn set_count_max(&mut self, n: usize) {
        self.count_max = n.clamp(Self::ABS_COUNT_MIN, Self::ABS_COUNT_MAX);
        if self.count_min > self.count_max {
            self.count_min = self.count_max;
        }
        self.default_count = self.default_count.clamp(self.count_min, self.count_max);
    }

    pub fn normalize(mut self) -> Self {
        self.count_min = self.count_min.clamp(Self::ABS_COUNT_MIN, Self::ABS_COUNT_MAX);
        self.count_max = self.count_max.clamp(Self::ABS_COUNT_MIN, Self::ABS_COUNT_MAX);
        if self.count_max < self.count_min {
            self.count_max = self.count_min;
        }
        self.default_count = self.default_count.clamp(self.count_min, self.count_max);
        self.volume_percent = self.volume_percent.min(100);
        for ext in &mut self.video_extensions {
            let lower = ext.to_ascii_lowercase();
            *ext = if lower.starts_with('.') {
                lower
            } else {
                format!(".{lower}")
            };
        }

        // Merge legacy library_path into library_paths
        let mut paths: Vec<String> = self
            .library_paths
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let legacy = self.library_path.trim().to_string();
        if !legacy.is_empty() {
            let exists = paths
                .iter()
                .any(|p| path_key(p) == path_key(&legacy));
            if !exists {
                paths.insert(0, legacy);
            }
        }
        // Dedupe (Windows: case-insensitive)
        let mut seen = std::collections::HashSet::new();
        paths.retain(|p| seen.insert(path_key(p)));
        self.library_paths = paths;
        // Keep library_path as first root for old tools
        self.library_path = self
            .library_paths
            .first()
            .cloned()
            .unwrap_or_default();
        self
    }

    pub fn add_library_path(&mut self, path: String) {
        let path = path.trim().to_string();
        if path.is_empty() {
            return;
        }
        if self
            .library_paths
            .iter()
            .any(|p| path_key(p) == path_key(&path))
        {
            return;
        }
        self.library_paths.push(path);
        *self = self.clone().normalize();
    }

    pub fn remove_library_path(&mut self, index: usize) {
        if index < self.library_paths.len() {
            self.library_paths.remove(index);
            *self = self.clone().normalize();
        }
    }
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
    let raw = fs::read_to_string(path).ok()?;
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
    fn legacy_library_path_merges_into_paths() {
        let mut c = Config::default();
        c.library_path = r"D:\Movies".into();
        c.library_paths = vec![r"E:\More".into()];
        let c = c.normalize();
        assert_eq!(c.library_roots().len(), 2);
        assert!(c.library_roots().iter().any(|p| p.contains("Movies")));
        assert!(c.library_roots().iter().any(|p| p.contains("More")));
    }

    #[test]
    fn add_path_dedupes_case_insensitive() {
        let mut c = Config::default();
        c.add_library_path(r"D:\Movies".into());
        c.add_library_path(r"d:\movies".into());
        assert_eq!(c.library_roots().len(), 1);
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
}
