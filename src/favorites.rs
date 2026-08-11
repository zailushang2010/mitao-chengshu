//! User favorites (pin for later). Independent of blacklist.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::app_data_dir;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Favorites {
    pub paths: Vec<String>,
}

impl Favorites {
    pub fn path_movies() -> PathBuf {
        app_data_dir().join("favorites.json")
    }

    pub fn path_images() -> PathBuf {
        app_data_dir().join("favorites_images.json")
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

    /// Returns `true` if now favorited.
    pub fn toggle(&mut self, path: &Path) -> bool {
        if self.contains(path) {
            self.remove(path);
            false
        } else {
            self.add(path);
            true
        }
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

fn normalize_key(p: &Path) -> String {
    p.to_string_lossy().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_and_dedupe() {
        let mut f = Favorites::default();
        assert!(f.toggle(Path::new(r"D:\A.mkv")));
        assert!(f.contains(Path::new(r"d:\a.mkv")));
        assert!(!f.toggle(Path::new(r"d:\A.mkv")));
        assert_eq!(f.len(), 0);
        f.add(Path::new(r"E:\b.mp4"));
        f.add(Path::new(r"e:\b.mp4"));
        assert_eq!(f.len(), 1);
    }
}
