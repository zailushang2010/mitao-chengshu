//! Permanent exclude lists (never pick again until removed).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::app_data_dir;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Blacklist {
    pub paths: Vec<String>,
}

impl Blacklist {
    pub fn path_movies() -> PathBuf {
        app_data_dir().join("blacklist.json")
    }

    pub fn path_images() -> PathBuf {
        app_data_dir().join("blacklist_images.json")
    }

    pub fn load_movies() -> Self {
        Self::load_from(&Self::path_movies()).unwrap_or_default()
    }

    pub fn load_images() -> Self {
        Self::load_from(&Self::path_images()).unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Option<Self> {
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save_movies(&self) -> Result<(), String> {
        self.save_to(&Self::path_movies())
    }

    pub fn save_images(&self) -> Result<(), String> {
        self.save_to(&Self::path_images())
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

    pub fn contains(&self, path: &Path) -> bool {
        let key = normalize_key(path);
        self.paths
            .iter()
            .any(|p| normalize_key(Path::new(p)) == key)
    }

    pub fn add(&mut self, path: &Path) {
        let s = path.to_string_lossy().to_string();
        let key = normalize_key(path);
        self.paths
            .retain(|p| normalize_key(Path::new(p)) != key);
        self.paths.push(s);
    }

    pub fn remove(&mut self, path: &Path) {
        let key = normalize_key(path);
        self.paths
            .retain(|p| normalize_key(Path::new(p)) != key);
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }
}

fn normalize_key(p: &Path) -> String {
    p.to_string_lossy().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_dedupes_case_insensitive() {
        let mut b = Blacklist::default();
        b.add(Path::new(r"D:\A.mkv"));
        b.add(Path::new(r"d:\a.mkv"));
        assert_eq!(b.len(), 1);
        assert!(b.contains(Path::new(r"D:\a.mkv")));
    }
}
