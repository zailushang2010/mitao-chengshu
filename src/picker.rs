use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Pick up to `n` videos. When `avoid_recent` is true, prefer files not in history.
/// If not enough after filtering, fall back to the full library.
pub fn pick(
    library: &[PathBuf],
    n: usize,
    avoid_recent: bool,
    recent: &[PathBuf],
) -> Vec<PathBuf> {
    if library.is_empty() || n == 0 {
        return Vec::new();
    }

    let n = n.min(library.len());
    let mut rng = thread_rng();

    let preferred: Vec<PathBuf> = if avoid_recent && !recent.is_empty() {
        let recent_set: HashSet<String> = recent
            .iter()
            .map(|p| normalize_key(p))
            .collect();
        let filtered: Vec<_> = library
            .iter()
            .filter(|p| !recent_set.contains(&normalize_key(p)))
            .cloned()
            .collect();
        if filtered.len() >= n {
            filtered
        } else if filtered.is_empty() {
            library.to_vec()
        } else {
            // Prefer non-recent first, then fill from recent pool.
            let mut pool = filtered;
            let mut rest: Vec<_> = library
                .iter()
                .filter(|p| recent_set.contains(&normalize_key(p)))
                .cloned()
                .collect();
            rest.shuffle(&mut rng);
            pool.extend(rest);
            pool
        }
    } else {
        library.to_vec()
    };

    let mut pool = preferred;
    pool.shuffle(&mut rng);
    pool.truncate(n);
    pool
}

fn normalize_key(p: &Path) -> String {
    p.to_string_lossy().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn pick_n_greater_than_library_returns_all() {
        let lib = paths(&["a.mkv", "b.mkv"]);
        let got = pick(&lib, 10, false, &[]);
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn avoid_recent_prefers_others() {
        let lib = paths(&["a.mkv", "b.mkv", "c.mkv", "d.mkv", "e.mkv", "f.mkv"]);
        let recent = paths(&["a.mkv", "b.mkv", "c.mkv", "d.mkv"]);
        for _ in 0..20 {
            let got = pick(&lib, 2, true, &recent);
            assert_eq!(got.len(), 2);
            for g in &got {
                let s = g.to_string_lossy();
                assert!(s == "e.mkv" || s == "f.mkv", "got unexpected {s}");
            }
        }
    }

    #[test]
    fn avoid_when_exhausted_still_returns() {
        let lib = paths(&["a.mkv", "b.mkv"]);
        let recent = paths(&["a.mkv", "b.mkv"]);
        let got = pick(&lib, 2, true, &recent);
        assert_eq!(got.len(), 2);
    }
}
