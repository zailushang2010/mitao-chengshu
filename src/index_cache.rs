//! Per-root media index cache on disk (speeds up second launch / mode switch).
//!
//! Invalidation uses a **directory-tree signature** (entry names + dir mtimes), not only
//! the library root's mtime — so nested add/remove is detected without a manual rescan.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::app_data_dir;

/// Bump when signature algorithm changes (forces one rebuild of caches).
const SIG_VERSION: u32 = 2;

/// Cap tree walk so huge folders stay responsive (still far cheaper than media listing).
const MAX_DIRS: usize = 6_000;
const MAX_ENTRIES: usize = 40_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RootCacheFile {
    root: String,
    extensions: Vec<String>,
    /// Signature of folder tree when scanned.
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

fn hash_mtime(h: &mut DefaultHasher, meta: &fs::Metadata) {
    if let Ok(modified) = meta.modified() {
        if let Ok(d) = modified.duration_since(SystemTime::UNIX_EPOCH) {
            d.as_secs().hash(h);
            d.subsec_nanos().hash(h);
        }
    }
}

fn skip_dir_name(name: &std::ffi::OsStr) -> bool {
    let s = name.to_string_lossy();
    matches!(
        s.as_ref(),
        ".git"
            | ".svn"
            | ".hg"
            | "node_modules"
            | "$RECYCLE.BIN"
            | "System Volume Information"
            | "@eaDir"
            | ".Trash"
            | ".Trash-1000"
    )
}

/// Tree signature: walk directories (capped), hash each dir mtime + child entry names.
/// Detects nested add/remove without scanning file contents.
pub fn root_signature(root: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    SIG_VERSION.hash(&mut h);
    root.to_string_lossy().to_ascii_lowercase().hash(&mut h);

    if !root.is_dir() {
        return h.finish();
    }

    // DFS with sorted children for stable hashes across runs
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut dirs_seen = 0usize;
    let mut entries_seen = 0usize;

    while let Some(dir) = stack.pop() {
        if dirs_seen >= MAX_DIRS || entries_seen >= MAX_ENTRIES {
            // Cap hit — include markers so growing past the cap can still invalidate
            "cap".hash(&mut h);
            dirs_seen.hash(&mut h);
            entries_seen.hash(&mut h);
            break;
        }
        dirs_seen += 1;

        if let Ok(meta) = fs::metadata(&dir) {
            dir.to_string_lossy().to_ascii_lowercase().hash(&mut h);
            hash_mtime(&mut h, &meta);
        }

        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        let mut kids: Vec<(bool, String, PathBuf)> = Vec::new();
        for e in rd.flatten() {
            if entries_seen >= MAX_ENTRIES {
                break;
            }
            entries_seen += 1;
            let name = e.file_name();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir && skip_dir_name(&name) {
                continue;
            }
            let name_lc = name.to_string_lossy().to_ascii_lowercase();
            kids.push((is_dir, name_lc, e.path()));
        }
        kids.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        for (is_dir, name_lc, path) in kids {
            is_dir.hash(&mut h);
            name_lc.hash(&mut h);
            if is_dir {
                stack.push(path);
            }
        }
    }

    dirs_seen.hash(&mut h);
    entries_seen.hash(&mut h);
    h.finish()
}

/// True when disk tree no longer matches the on-disk cache (or cache missing).
pub fn is_stale(root: &Path, extensions: &[String]) -> bool {
    if !root.is_dir() {
        return true;
    }
    let root_s = root.to_string_lossy().to_string();
    let path = cache_path(&root_s, extensions);
    let Ok(raw) = fs::read_to_string(&path) else {
        return true;
    };
    let Ok(cached) = serde_json::from_str::<RootCacheFile>(&raw) else {
        return true;
    };
    if cached.root.to_ascii_lowercase() != root_s.to_ascii_lowercase() {
        return true;
    }
    let mut cached_exts = cached.extensions.clone();
    let mut want_exts: Vec<_> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();
    cached_exts.sort();
    want_exts.sort();
    if cached_exts != want_exts {
        return true;
    }
    cached.signature != root_signature(root)
}

/// Load cached file list if tree signature still matches.
/// Filters out paths that no longer exist; if too many are gone, treats as miss.
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

    let total = cached.files.len();
    let files: Vec<PathBuf> = cached
        .files
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect();
    // Many missing files → disk changed in a way signature missed (e.g. content wipe)
    if total > 8 {
        let kept = files.len() as f32 / total as f32;
        if kept < 0.92 {
            return None;
        }
    }
    Some(files)
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
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

    #[test]
    fn nested_add_invalidates_cache() {
        let root = tmp_root("nested");
        let sub = root.join("season1");
        fs::create_dir_all(&sub).unwrap();
        let f = sub.join("a.mkv");
        fs::write(&f, b"x").unwrap();
        let exts = vec![".mkv".into()];
        save_root(&root, &exts, &[f.clone()]);
        assert!(load_root(&root, &exts).is_some());

        // Nested new file changes tree entry list (signature)
        std::thread::sleep(Duration::from_millis(20));
        fs::write(sub.join("b.mkv"), b"y").unwrap();
        assert!(
            is_stale(&root, &exts),
            "nested add should make cache stale"
        );
        assert!(load_root(&root, &exts).is_none());
        let _ = fs::remove_dir_all(&root);
        invalidate_root(&root.to_string_lossy(), &exts);
    }
}
