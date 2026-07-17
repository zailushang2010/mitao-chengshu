//! Thumbnail cache for session previews.
//!
//! Priority:
//! 1. Local images already on disk next to the video (sidecar / cover / folder art)
//! 2. Previously extracted cache under exe/thumbs/
//! 3. ffmpeg frame grab (if installed)
//!
//! Note: Windows Explorer 内置缩略图缓存在系统 ThumbCache 里，不是视频旁边的文件；
//! 本程序优先用「硬盘上可见的图片文件」，没有再用 ffmpeg 抽帧。

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
    states: HashMap<String, ThumbState>,
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
    // 1) 硬盘上已有的本地图片（优先，不重复抽帧）
    if let Some(p) = find_local_poster(video) {
        return Some(p);
    }

    // 2) 本程序以前抽过的缓存
    let out = cache_path_for(video);
    if out.is_file() {
        return Some(out);
    }

    // 3) ffmpeg 抽一帧
    let ffmpeg_ok = {
        let g = cache.lock().unwrap();
        g.ffmpeg_ok
    };
    if ffmpeg_ok && extract_with_ffmpeg(video, &out) {
        return Some(out);
    }

    None
}

/// Find an image already stored with the movie on disk.
///
/// Recognized layouts (examples for `D:\Movies\Inception.mkv`):
/// - `Inception.jpg` / `.jpeg` / `.png` / `.webp` / `.bmp`
/// - `Inception-poster.jpg`, `Inception_poster.jpg`, `Inception-thumb.jpg`
/// - `poster.jpg`, `cover.jpg`, `folder.jpg`, `thumb.jpg`, `fanart.jpg` in same folder
/// - same names under a `extrafanart` / `.actors` are ignored; only direct folder
fn find_local_poster(video: &Path) -> Option<PathBuf> {
    let stem = video.file_stem()?.to_string_lossy();
    let parent = video.parent()?;

    const EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];

    // Same basename as video (highest priority)
    for ext in EXTS {
        let candidates = [
            parent.join(format!("{stem}.{ext}")),
            parent.join(format!("{stem}-poster.{ext}")),
            parent.join(format!("{stem}_poster.{ext}")),
            parent.join(format!("{stem}-thumb.{ext}")),
            parent.join(format!("{stem}_thumb.{ext}")),
            parent.join(format!("{stem}-cover.{ext}")),
            parent.join(format!("{stem}_cover.{ext}")),
            parent.join(format!("{stem}.tbn")), // Kodi legacy thumb extension sometimes jpg data
        ];
        for p in candidates {
            if is_image_file(&p) {
                return Some(p);
            }
        }
    }

    // Common media-server folder art (same directory as the video)
    for name in [
        "poster",
        "cover",
        "folder",
        "thumb",
        "thumbnail",
        "fanart",
        "backdrop",
        "landscape",
        "movie",
        "封面",
        "海报",
        "缩略图",
    ] {
        for ext in EXTS {
            let p = parent.join(format!("{name}.{ext}"));
            if is_image_file(&p) {
                return Some(p);
            }
        }
    }

    // Movie in its own folder: D:\Movies\Inception\Inception.mkv + poster.jpg already covered.
    // Also try parent if video is in a disc subfolder (BDMV/stream style) — go up one level once.
    if let Some(grand) = parent.parent() {
        let folder_name = parent.file_name()?.to_string_lossy();
        // only when folder looks like a title folder (not drive root)
        if grand.parent().is_some() {
            for ext in EXTS {
                let p = grand.join(format!("{folder_name}.{ext}"));
                if is_image_file(&p) {
                    return Some(p);
                }
                for name in ["poster", "cover", "folder", "封面", "海报"] {
                    let p = parent.join(format!("{name}.{ext}"));
                    if is_image_file(&p) {
                        return Some(p);
                    }
                    let p2 = grand.join(format!("{name}.{ext}"));
                    if is_image_file(&p2) {
                        return Some(p2);
                    }
                }
            }
        }
    }

    None
}

fn is_image_file(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    // Reject tiny/corrupt placeholders
    std::fs::metadata(p)
        .map(|m| m.len() > 256)
        .unwrap_or(false)
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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn cache_path_stable() {
        let a = cache_path_for(Path::new(r"D:\Movies\a.mkv"));
        let b = cache_path_for(Path::new(r"D:\Movies\a.mkv"));
        assert_eq!(a, b);
    }

    #[test]
    fn finds_sidecar_same_name() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("suiji_thumb_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        let video = dir.join("demo.mkv");
        let poster = dir.join("demo.jpg");
        fs::write(&video, b"x").unwrap();
        fs::write(&poster, vec![0u8; 512]).unwrap();
        let found = find_local_poster(&video).unwrap();
        assert_eq!(found, poster);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_folder_poster() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("suiji_thumb2_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        let video = dir.join("film.mp4");
        let poster = dir.join("poster.png");
        fs::write(&video, b"x").unwrap();
        fs::write(&poster, vec![0u8; 512]).unwrap();
        let found = find_local_poster(&video).unwrap();
        assert_eq!(found, poster);
        let _ = fs::remove_dir_all(&dir);
    }
}
