use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Pick up to `n` videos.
/// - `blacklist`: hard exclude (never pick; if nothing left → empty).
/// - `avoid_recent`: soft prefer non-history; may fall back into recent.
pub fn pick(
    library: &[PathBuf],
    n: usize,
    avoid_recent: bool,
    recent: &[PathBuf],
    blacklist: &[PathBuf],
) -> Vec<PathBuf> {
    if library.is_empty() || n == 0 {
        return Vec::new();
    }

    let blocked: HashSet<String> = blacklist.iter().map(|p| normalize_key(p)).collect();
    let eligible: Vec<PathBuf> = library
        .iter()
        .filter(|p| !blocked.contains(&normalize_key(p)))
        .cloned()
        .collect();
    if eligible.is_empty() {
        return Vec::new();
    }

    let n = n.min(eligible.len());
    let mut rng = thread_rng();

    let preferred: Vec<PathBuf> = if avoid_recent && !recent.is_empty() {
        let recent_set: HashSet<String> = recent.iter().map(|p| normalize_key(p)).collect();
        let filtered: Vec<_> = eligible
            .iter()
            .filter(|p| !recent_set.contains(&normalize_key(p)))
            .cloned()
            .collect();
        if filtered.len() >= n {
            filtered
        } else if filtered.is_empty() {
            eligible
        } else {
            let mut pool = filtered;
            let mut rest: Vec<_> = eligible
                .iter()
                .filter(|p| recent_set.contains(&normalize_key(p)))
                .cloned()
                .collect();
            rest.shuffle(&mut rng);
            pool.extend(rest);
            pool
        }
    } else {
        eligible
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
        let got = pick(&lib, 10, false, &[], &[]);
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn avoid_recent_prefers_others() {
        let lib = paths(&["a.mkv", "b.mkv", "c.mkv", "d.mkv", "e.mkv", "f.mkv"]);
        let recent = paths(&["a.mkv", "b.mkv", "c.mkv", "d.mkv"]);
        for _ in 0..20 {
            let got = pick(&lib, 2, true, &recent, &[]);
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
        let got = pick(&lib, 2, true, &recent, &[]);
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn blacklist_never_picked() {
        let lib = paths(&["a.mkv", "b.mkv", "c.mkv"]);
        let blocked = paths(&["a.mkv", "b.mkv"]);
        for _ in 0..15 {
            let got = pick(&lib, 2, false, &[], &blocked);
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].to_string_lossy(), "c.mkv");
        }
    }

    #[test]
    fn all_blacklisted_returns_empty() {
        let lib = paths(&["a.mkv", "b.mkv"]);
        let blocked = paths(&["a.mkv", "b.mkv"]);
        let got = pick(&lib, 2, false, &[], &blocked);
        assert!(got.is_empty());
    }
}
