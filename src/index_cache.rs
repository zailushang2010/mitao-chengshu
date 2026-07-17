//! Per-root media index cache on disk (speeds up second launch / mode switch).

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::app_data_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RootCacheFile {
    root: String,
    extensions: Vec<String>,
    /// Signature of root folder metadata when scanned.
    signature: u64,
    files: Vec<String>,
}

fn cache_dir() -> PathBuf {
    let d = app_data_dir().join("index_cache");
    let _ = fs::create_dir_all(&d);
    d
}

fn cache_key(root: &str, extensions: &[String]) -> String {
    let mut h = DefaultHasher::new();
    root.to_ascii_lowercase().hash(&mut h);
    let mut exts: Vec<_> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();
    exts.sort();
    exts.hash(&mut h);
    format!("{:016x}.json", h.finish())
}

fn cache_path(root: &str, extensions: &[String]) -> PathBuf {
    cache_dir().join(cache_key(root, extensions))
}

/// Cheap signature: root path mtime (when available). Not perfect for deep trees,
/// but good enough with explicit 重新扫描 for correctness.
pub fn root_signature(root: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    root.to_string_lossy().to_ascii_lowercase().hash(&mut h);
    if let Ok(meta) = fs::metadata(root) {
        if let Ok(modified) = meta.modified() {
            if let Ok(d) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                d.as_secs().hash(&mut h);
                d.subsec_nanos().hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Load cached file list if signature still matches.
pub fn load_root(root: &Path, extensions: &[String]) -> Option<Vec<PathBuf>> {
    if !root.is_dir() {
        return None;
    }
    let root_s = root.to_string_lossy().to_string();
    let path = cache_path(&root_s, extensions);
    let raw = fs::read_to_string(&path).ok()?;
    let cached: RootCacheFile = serde_json::from_str(&raw).ok()?;
    if cached.root.to_ascii_lowercase() != root_s.to_ascii_lowercase() {
        return None;
    }
    let mut cached_exts = cached.extensions.clone();
    let mut want_exts: Vec<_> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();
    cached_exts.sort();
    want_exts.sort();
    if cached_exts != want_exts {
        return None;
    }
    if cached.signature != root_signature(root) {
        return None;
    }
    Some(cached.files.into_iter().map(PathBuf::from).collect())
}

pub fn save_root(root: &Path, extensions: &[String], files: &[PathBuf]) {
    if !root.is_dir() {
        return;
    }
    let root_s = root.to_string_lossy().to_string();
    let mut exts: Vec<_> = extensions.iter().cloned().collect();
    exts.sort_by_key(|e| e.to_ascii_lowercase());
    let payload = RootCacheFile {
        root: root_s.clone(),
        extensions: exts,
        signature: root_signature(root),
        files: files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
    };
    let path = cache_path(&root_s, extensions);
    if let Ok(raw) = serde_json::to_string(&payload) {
        let _ = fs::write(path, raw);
    }
}

pub fn invalidate_root(root: &str, extensions: &[String]) {
    let path = cache_path(root, extensions);
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_root(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("mitao_idx_{nanos}_{tag}"));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn save_load_roundtrip() {
        let root = tmp_root("rt");
        let f = root.join("a.mkv");
        fs::write(&f, b"x").unwrap();
        let exts = vec![".mkv".into()];
        save_root(&root, &exts, &[f.clone()]);
        let loaded = load_root(&root, &exts).expect("cache hit");
        assert_eq!(loaded, vec![f]);
        let _ = fs::remove_dir_all(&root);
        invalidate_root(&root.to_string_lossy(), &exts);
    }
}
