use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Library {
    /// Roots that were scanned (may include missing dirs).
    #[allow(dead_code)]
    pub roots: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
}

impl Library {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn scan(root: impl AsRef<Path>, extensions: &[String]) -> Self {
        Self::scan_many(&[root.as_ref().to_path_buf()], extensions)
    }

    /// Scan multiple roots recursively; merge and dedupe file paths.
    pub fn scan_many(roots: &[PathBuf], extensions: &[String]) -> Self {
        Self::scan_many_with_progress(roots, extensions, |_| {})
    }

    /// Like `scan_many`, but invokes `on_found` after each matched media file.
    pub fn scan_many_with_progress(
        roots: &[PathBuf],
        extensions: &[String],
        mut on_found: impl FnMut(usize),
    ) -> Self {
        Self::scan_many_cancellable(roots, extensions, &mut on_found, &|| false)
            .unwrap_or_else(Self::empty)
    }

    /// Full scan with progress + cancel. Returns `None` if cancelled mid-way.
    pub fn scan_many_cancellable(
        roots: &[PathBuf],
        extensions: &[String],
        on_found: &mut impl FnMut(usize),
        is_cancelled: &impl Fn() -> bool,
    ) -> Option<Self> {
        let roots: Vec<PathBuf> = roots
            .iter()
            .filter(|r| !r.as_os_str().is_empty())
            .cloned()
            .collect();

        if roots.is_empty() {
            return Some(Self::empty());
        }

        let ext_set: BTreeSet<String> = extensions
            .iter()
            .map(|e| e.to_ascii_lowercase())
            .collect();

        let mut files = Vec::new();
        for root in &roots {
            if is_cancelled() {
                return None;
            }
            if !root.is_dir() {
                continue;
            }
            if !collect_under(root, &ext_set, &mut files, on_found, is_cancelled) {
                return None;
            }
        }

        files.sort();
        files.dedup();

        Some(Self { roots, files })
    }

    /// Scan one root only (for per-root disk cache fill).
    pub fn scan_one_cancellable(
        root: &Path,
        extensions: &[String],
        on_found: &mut impl FnMut(usize),
        is_cancelled: &impl Fn() -> bool,
        start_count: usize,
    ) -> Option<Vec<PathBuf>> {
        if is_cancelled() {
            return None;
        }
        if !root.is_dir() {
            return Some(Vec::new());
        }
        let ext_set: BTreeSet<String> = extensions
            .iter()
            .map(|e| e.to_ascii_lowercase())
            .collect();
        let mut local = Vec::new();
        if !collect_under(
            root,
            &ext_set,
            &mut local,
            &mut |n| on_found(start_count + n),
            is_cancelled,
        ) {
            return None;
        }
        Some(local)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Returns false if cancelled.
fn collect_under(
    root: &Path,
    ext_set: &BTreeSet<String>,
    files: &mut Vec<PathBuf>,
    on_found: &mut impl FnMut(usize),
    is_cancelled: &impl Fn() -> bool,
) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if is_cancelled() {
            return false;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if is_cancelled() {
                return false;
            }
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                let ok = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| {
                        let with_dot = format!(".{}", e.to_ascii_lowercase());
                        ext_set.contains(&with_dot)
                    })
                    .unwrap_or(false);
                if ok {
                    files.push(path);
                    on_found(files.len());
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("suiji_lib_{tag}_{nanos}"));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn scan_filters_extensions_and_nested() {
        let root = temp_root("one");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("x.mkv"), b"1").unwrap();
        fs::write(root.join("a/y.MP4"), b"1").unwrap();
        fs::write(root.join("a/b/z.txt"), b"1").unwrap();
        fs::write(root.join("skip.nfo"), b"1").unwrap();

        let lib = Library::scan(&root, &[".mkv".into(), ".mp4".into()]);
        assert_eq!(lib.len(), 2);
        let names: Vec<_> = lib
            .files
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .collect();
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("x.mkv")));
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("y.MP4")));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_many_merges_and_dedupes() {
        let a = temp_root("a");
        let b = temp_root("b");
        fs::write(a.join("one.mkv"), b"1").unwrap();
        fs::write(b.join("two.mp4"), b"1").unwrap();
        // same file path if we scan a twice — use identical path in list
        let lib = Library::scan_many(
            &[a.clone(), b.clone(), a.clone()],
            &[".mkv".into(), ".mp4".into()],
        );
        assert_eq!(lib.len(), 2);
        assert_eq!(lib.roots.len(), 3);

        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    /// Optional local smoke test. Set env then run:
    /// `$env:MITAO_TEST_LIBRARY="D:\Videos"; cargo test scan_local_library_env -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scan_local_library_env() {
        let path = std::env::var("MITAO_TEST_LIBRARY").unwrap_or_default();
        assert!(
            !path.is_empty(),
            "set MITAO_TEST_LIBRARY to a folder with videos"
        );
        let lib = Library::scan(
            &path,
            &[
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
        );
        eprintln!("{path} indexed = {}", lib.len());
        assert!(!lib.is_empty(), "expected at least one video under {path}");
    }
}
