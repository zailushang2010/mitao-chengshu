//! Thumbnail cache for session previews.
//! Priority: sidecar image next to video → ffmpeg frame extract → none.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::app_data_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbState {
    Pending,
    Ready,
    Missing,
}

#[derive(Clone, Default)]
pub struct ThumbCache {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    /// video path string -> state
    states: HashMap<String, ThumbState>,
    /// video path string -> jpg cache file
    files: HashMap<String, PathBuf>,
    ffmpeg_checked: bool,
    ffmpeg_ok: bool,
}

impl ThumbCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cache_dir() -> PathBuf {
        let d = app_data_dir().join("thumbs");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    pub fn request(&self, video: &Path) {
        let key = video.to_string_lossy().to_string();
        {
            let mut g = self.inner.lock().unwrap();
            match g.states.get(&key) {
                Some(ThumbState::Ready | ThumbState::Pending | ThumbState::Missing) => return,
                None => {
                    g.states.insert(key.clone(), ThumbState::Pending);
                }
            }
            if !g.ffmpeg_checked {
                g.ffmpeg_ok = ffmpeg_available();
                g.ffmpeg_checked = true;
            }
        }

        let cache = self.inner.clone();
        let video = video.to_path_buf();
        thread::spawn(move || {
            let result = resolve_thumb(&video, &cache);
            let mut g = cache.lock().unwrap();
            let key = video.to_string_lossy().to_string();
            match result {
                Some(path) => {
                    g.files.insert(key.clone(), path);
                    g.states.insert(key, ThumbState::Ready);
                }
                None => {
                    g.states.insert(key, ThumbState::Missing);
                }
            }
        });
    }

    pub fn path_if_ready(&self, video: &Path) -> Option<PathBuf> {
        let key = video.to_string_lossy().to_string();
        let g = self.inner.lock().unwrap();
        if g.states.get(&key) == Some(&ThumbState::Ready) {
            g.files.get(&key).cloned()
        } else {
            None
        }
    }
}

fn resolve_thumb(video: &Path, cache: &Arc<Mutex<Inner>>) -> Option<PathBuf> {
    // 1) Sidecar poster: movie.jpg / movie.png next to file
    if let Some(p) = sidecar_image(video) {
        return Some(p);
    }

    // 2) Cached extract
    let out = cache_path_for(video);
    if out.is_file() {
        return Some(out);
    }

    // 3) ffmpeg
    let ffmpeg_ok = {
        let g = cache.lock().unwrap();
        g.ffmpeg_ok
    };
    if ffmpeg_ok && extract_with_ffmpeg(video, &out) {
        return Some(out);
    }

    None
}

fn sidecar_image(video: &Path) -> Option<PathBuf> {
    let stem = video.file_stem()?.to_string_lossy();
    let parent = video.parent()?;
    for ext in ["jpg", "jpeg", "png", "webp"] {
        let p = parent.join(format!("{stem}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
        // also folder-jpg style
        let p2 = parent.join(format!("poster.{ext}"));
        if p2.is_file() {
            return Some(p2);
        }
    }
    None
}

fn cache_path_for(video: &Path) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    video.to_string_lossy().hash(&mut hasher);
    let h = hasher.finish();
    ThumbCache::cache_dir().join(format!("{h:016x}.jpg"))
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn extract_with_ffmpeg(video: &Path, out: &Path) -> bool {
    // Seek ~30s (or start) for a representative frame; hide console on Windows
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-ss",
        "30",
        "-i",
    ])
    .arg(video)
    .args(["-frames:v", "1", "-q:v", "5", "-vf", "scale=320:-1"])
    .arg(out)
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match cmd.status() {
        Ok(st) if st.success() && out.is_file() => true,
        _ => {
            // Retry from start if mid-seek failed (short clips)
            let mut cmd2 = Command::new("ffmpeg");
            cmd2.args(["-hide_banner", "-loglevel", "error", "-y", "-ss", "1", "-i"])
                .arg(video)
                .args(["-frames:v", "1", "-q:v", "5", "-vf", "scale=320:-1"])
                .arg(out)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                cmd2.creation_flags(CREATE_NO_WINDOW);
            }
            cmd2.status()
                .map(|s| s.success() && out.is_file())
                .unwrap_or(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_stable() {
        let a = cache_path_for(Path::new(r"D:\Movies\a.mkv"));
        let b = cache_path_for(Path::new(r"D:\Movies\a.mkv"));
        assert_eq!(a, b);
        assert!(a.extension().and_then(|e| e.to_str()) == Some("jpg"));
    }
}
