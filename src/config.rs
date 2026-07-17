use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub library_path: String,
    pub default_count: usize,
    pub count_min: usize,
    pub count_max: usize,
    pub volume_percent: u8,
    pub avoid_recent: bool,
    pub recent_history_size: usize,
    pub potplayer_path: String,
    pub video_extensions: Vec<String>,
    pub close_session_on_exit: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: String::new(),
            default_count: 6,
            count_min: 5,
            count_max: 10,
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
        }
    }
}

impl Config {
    pub fn clamp_count(&self, n: usize) -> usize {
        n.clamp(self.count_min, self.count_max)
    }

    pub fn normalize(mut self) -> Self {
        if self.count_min == 0 {
            self.count_min = 1;
        }
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
        self
    }
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
        assert_eq!(c.clamp_count(1), 5);
        assert_eq!(c.clamp_count(6), 6);
        assert_eq!(c.clamp_count(99), 10);
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
    fn save_and_load_roundtrip() {
        let path = tmp_file("ok.json");
        let mut c = Config::default();
        c.library_path = r"D:\Movies".into();
        c.default_count = 8;
        save_to(&path, &c).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.library_path, r"D:\Movies");
        assert_eq!(loaded.default_count, 8);
        let _ = fs::remove_file(&path);
    }
}
