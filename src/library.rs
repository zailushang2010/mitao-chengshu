use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Library {
    #[allow(dead_code)]
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
}

impl Library {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn scan(root: impl AsRef<Path>, extensions: &[String]) -> Self {
        let root = root.as_ref().to_path_buf();
        if root.as_os_str().is_empty() || !root.is_dir() {
            return Self {
                root,
                files: Vec::new(),
            };
        }

        let ext_set: BTreeSet<String> = extensions
            .iter()
            .map(|e| e.to_ascii_lowercase())
            .collect();

        let mut files = Vec::new();
        let mut stack = vec![root.clone()];

        while let Some(dir) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
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
                    }
                }
            }
        }

        files.sort();
        files.dedup();

        Self { root, files }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("suiji_lib_{nanos}"));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn scan_filters_extensions_and_nested() {
        let root = temp_root();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("x.mkv"), b"1").unwrap();
        fs::write(root.join("a/y.MP4"), b"1").unwrap();
        fs::write(root.join("a/b/z.txt"), b"1").unwrap();
        fs::write(root.join("skip.nfo"), b"1").unwrap();

        let lib = Library::scan(
            &root,
            &[".mkv".into(), ".mp4".into()],
        );
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
}
