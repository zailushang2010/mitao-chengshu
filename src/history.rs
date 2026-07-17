use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::app_data_dir;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    pub paths: Vec<String>,
}

impl History {
    pub fn path() -> PathBuf {
        app_data_dir().join("history.json")
    }

    pub fn path_for_images() -> PathBuf {
        app_data_dir().join("history_images.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path()).unwrap_or_default()
    }

    pub fn load_images() -> Self {
        Self::load_from(&Self::path_for_images()).unwrap_or_default()
    }

    pub fn save_images(&self) -> Result<(), String> {
        self.save_to(&Self::path_for_images())
    }

    pub fn load_from(path: &Path) -> Option<Self> {
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, raw).map_err(|e| e.to_string())
    }

    pub fn as_path_set(&self) -> Vec<PathBuf> {
        self.paths.iter().map(PathBuf::from).collect()
    }

    pub fn push_many(&mut self, new_paths: &[PathBuf], max_size: usize) {
        for p in new_paths {
            let s = p.to_string_lossy().to_string();
            self.paths.retain(|x| x != &s);
            self.paths.push(s);
        }
        if self.paths.len() > max_size {
            let drop_n = self.paths.len() - max_size;
            self.paths.drain(0..drop_n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_many_dedup_and_trim() {
        let mut h = History::default();
        h.push_many(&[PathBuf::from("a"), PathBuf::from("b")], 3);
        h.push_many(&[PathBuf::from("a"), PathBuf::from("c")], 3);
        assert_eq!(h.paths, vec!["b".to_string(), "a".to_string(), "c".to_string()]);
        h.push_many(&[PathBuf::from("d")], 3);
        assert_eq!(h.paths, vec!["a".to_string(), "c".to_string(), "d".to_string()]);
    }
}
