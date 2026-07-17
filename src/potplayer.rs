use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_TERMINATE,
};

/// Win32 STILL_ACTIVE exit code while process is running.
const STILL_ACTIVE: u32 = 259;

const CANDIDATES: &[&str] = &[
    r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe",
    r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini.exe",
    r"C:\Program Files (x86)\DAUM\PotPlayer\PotPlayerMini.exe",
];

/// Gap between launches when using hide-then-place strategy (can be shorter).
const LAUNCH_STAGGER_MS: u64 = 140;

pub fn resolve_potplayer_path(configured: &str) -> Option<PathBuf> {
    if !configured.trim().is_empty() {
        let p = PathBuf::from(configured.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    for c in CANDIDATES {
        let p = PathBuf::from(c);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct LaunchedItem {
    pub path: PathBuf,
    pub pid: u32,
}

pub fn launch_many(potplayer: &Path, files: &[PathBuf]) -> (Vec<LaunchedItem>, Vec<String>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();

    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            thread::sleep(Duration::from_millis(LAUNCH_STAGGER_MS + i as u64 * 30));
        }
        match launch_one(potplayer, file) {
            Ok(pid) => ok.push(LaunchedItem {
                path: file.clone(),
                pid,
            }),
            Err(e) => errors.push(format!("{}: {e}", file.display())),
        }
    }

    (ok, errors)
}

/// Launch each file, **hide the window as soon as it appears**, so PotPlayer cannot
/// flash/restore its remembered size&pos before we place it.
pub fn launch_many_hidden(
    potplayer: &Path,
    files: &[PathBuf],
) -> (Vec<LaunchedItem>, Vec<isize>, Vec<String>) {
    let mut ok = Vec::new();
    let mut hwnds = Vec::new();
    let mut errors = Vec::new();

    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            thread::sleep(Duration::from_millis(LAUNCH_STAGGER_MS + i as u64 * 25));
        }
        match launch_one(potplayer, file) {
            Ok(pid) => {
                // Poll until HWND, then hide immediately (root of "jump out" is visible restore)
                let hwnd = wait_single_hwnd(pid, 4500);
                if hwnd != 0 {
                    // Hide ASAP so remembered size/pos never flashes on screen
                    crate::tiler::hide_window(hwnd);
                }
                ok.push(LaunchedItem {
                    path: file.clone(),
                    pid,
                });
                hwnds.push(hwnd);
            }
            Err(e) => {
                errors.push(format!("{}: {e}", file.display()));
            }
        }
    }

    (ok, hwnds, errors)
}

/// Wait for one process main window; returns 0 on timeout.
pub fn wait_single_hwnd(pid: u32, timeout_ms: u64) -> isize {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    while std::time::Instant::now() < deadline {
        let list = hwnds_aligned_to_pids(&[pid], 2, 40);
        if let Some(&h) = list.first() {
            if h != 0 {
                return h;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    hwnds_aligned_to_pids(&[pid], 8, 80)
        .first()
        .copied()
        .unwrap_or(0)
}

pub fn launch_one(potplayer: &Path, file: &Path) -> Result<u32, String> {
    let child = Command::new(potplayer)
        .arg(file)
        .arg("/new")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(child.id())
}

pub fn kill_pids(pids: &[u32]) {
    for &pid in pids {
        let _ = kill_pid(pid);
    }
}

pub fn kill_pid(pid: u32) -> Result<(), String> {
    unsafe {
        let handle: HANDLE =
            OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| e.to_string())?;
        let result = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        result.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// True if the process is still running (for reaping dead PotPlayers).
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut code).is_ok();
        let _ = CloseHandle(handle);
        ok && code == STILL_ACTIVE
    }
}

/// Focus / restore the main window of a PotPlayer process.
pub fn focus_pid(pid: u32) {
    let pairs = find_hwnds_for_pids(&[pid], 6, 80);
    for (_, hwnd) in pairs {
        raise_hwnd(hwnd as isize);
    }
}

/// Maximize (独播感) the window for a PID.
pub fn maximize_pid(pid: u32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_MAXIMIZE, SW_RESTORE, SW_SHOW};

    let pairs = find_hwnds_for_pids(&[pid], 6, 80);
    for (_, h) in pairs {
        unsafe {
            let hwnd = HWND(h as *mut _);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        }
        raise_hwnd(h);
    }
}

fn raise_hwnd(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, IsIconic, SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOPMOST,
        HWND_NOTOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL,
    };
    if hwnd == 0 {
        return;
    }
    unsafe {
        let h = HWND(hwnd as *mut _);
        let _ = ShowWindow(h, SW_SHOWNORMAL);
        let _ = ShowWindow(h, SW_SHOW);
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let _ = SetWindowPos(
            h,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let _ = BringWindowToTop(h);
        let _ = SetForegroundWindow(h);
        let _ = SetWindowPos(
            h,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
    }
}

/// Find primary top-level window for each PID (prefer PotPlayer class + largest area).
/// Returns `(pid, hwnd)` in the same order as `pids`; missing PIDs are omitted.
pub fn find_hwnds_for_pids(pids: &[u32], attempts: u32, delay_ms: u64) -> Vec<(u32, isize)> {
    let want: HashSet<u32> = pids.iter().copied().collect();
    // score = class_bonus + area
    let mut best: HashMap<u32, (isize, i64)> = HashMap::new();

    for attempt in 0..attempts {
        enum_top_level_windows(|hwnd, pid, score| {
            if !want.contains(&pid) {
                return;
            }
            let entry = best.entry(pid).or_insert((hwnd, i64::MIN));
            if score > entry.1 {
                *entry = (hwnd, score);
            }
        });

        if best.len() >= want.len() && attempt >= 2 {
            break;
        }
        thread::sleep(Duration::from_millis(delay_ms));
    }

    pids.iter()
        .filter_map(|pid| best.get(pid).map(|(h, _)| (*pid, *h)))
        .collect()
}

/// Ordered hwnd list aligned to `pids` (0 = not found yet).
pub fn hwnds_aligned_to_pids(pids: &[u32], attempts: u32, delay_ms: u64) -> Vec<isize> {
    let found = find_hwnds_for_pids(pids, attempts, delay_ms);
    let map: HashMap<u32, isize> = found.into_iter().collect();
    pids.iter().map(|p| map.get(p).copied().unwrap_or(0)).collect()
}

/// Poll until every PID has a window, or `timeout_ms` elapses.
/// Favors waiting for the last PID (usually the one that "jumps out").
pub fn wait_hwnds_aligned(pids: &[u32], timeout_ms: u64) -> Vec<isize> {
    if pids.is_empty() {
        return Vec::new();
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut last = hwnds_aligned_to_pids(pids, 3, 80);
    while std::time::Instant::now() < deadline {
        if last.iter().all(|&h| h != 0) {
            // All present — short extra settle for last window chrome
            thread::sleep(Duration::from_millis(180));
            return hwnds_aligned_to_pids(pids, 2, 50);
        }
        thread::sleep(Duration::from_millis(100));
        last = hwnds_aligned_to_pids(pids, 2, 60);
    }
    // Final longer hunt for any still missing (esp. last)
    hwnds_aligned_to_pids(pids, 12, 100)
}

fn window_class_name(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    if n == 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n])
}

fn is_potplayer_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.contains("potplayer") || c.contains("potplayermini")
}

fn enum_top_level_windows(mut f: impl FnMut(isize, u32, i64)) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        GW_OWNER,
    };

    struct Ctx<'a> {
        f: &'a mut dyn FnMut(isize, u32, i64),
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut Ctx<'_>);
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return BOOL(1);
        }
        // Accept invisible briefly-creating windows only if iconic; else need visible
        let visible = IsWindowVisible(hwnd).as_bool();
        let iconic = IsIconic(hwnd).as_bool();
        if !visible && !iconic {
            return BOOL(1);
        }
        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or(HWND::default());
        if !owner.0.is_null() {
            return BOOL(1);
        }

        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return BOOL(1);
        }
        let w = (rect.right - rect.left).max(0) as i64;
        let h = (rect.bottom - rect.top).max(0) as i64;
        let mut area = w * h;
        if iconic {
            area = area.max(80_000);
        }
        // Ignore tiny tool windows / splash (unless PotPlayer class)
        let class = window_class_name(hwnd);
        let pot = is_potplayer_class(&class);
        if !pot && area < 8_000 {
            return BOOL(1);
        }
        // Prefer real player frames over auxiliary windows of same process
        let class_bonus = if pot { 50_000_000_i64 } else { 0 };
        let score = class_bonus + area;
        (ctx.f)(hwnd.0 as isize, pid, score);
        BOOL(1)
    }

    let mut ctx = Ctx { f: &mut f };
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut ctx as *mut Ctx<'_> as isize));
    }
}
