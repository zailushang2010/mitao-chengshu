use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

const CANDIDATES: &[&str] = &[
    r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe",
    r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini.exe",
    r"C:\Program Files (x86)\DAUM\PotPlayer\PotPlayerMini.exe",
];

/// Stagger between multi-instance launches so each window can register.
const LAUNCH_STAGGER_MS: u64 = 180;

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
            thread::sleep(Duration::from_millis(LAUNCH_STAGGER_MS));
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

/// Find primary (largest visible top-level) window for each PID, with retries.
pub fn find_hwnds_for_pids(pids: &[u32], attempts: u32, delay_ms: u64) -> Vec<(u32, isize)> {
    let want: HashSet<u32> = pids.iter().copied().collect();
    let mut best: HashMap<u32, (isize, i64)> = HashMap::new();

    for attempt in 0..attempts {
        enum_top_level_windows(|hwnd, pid, area| {
            if !want.contains(&pid) || area < 20_000 {
                return;
            }
            let entry = best.entry(pid).or_insert((hwnd, 0));
            if area > entry.1 {
                *entry = (hwnd, area);
            }
        });

        if best.len() >= want.len() && attempt >= 2 {
            // Found all; still give one more short settle after attempt 2
            break;
        }
        thread::sleep(Duration::from_millis(delay_ms));
    }

    // Preserve input PID order
    pids.iter()
        .filter_map(|pid| best.get(pid).map(|(h, _)| (*pid, *h)))
        .collect()
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
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        // Skip minimized-only enumeration for size; still include for restore later
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
        // Minimized windows often report small shell rects — still accept with floor
        if IsIconic(hwnd).as_bool() {
            area = area.max(100_000);
        }
        (ctx.f)(hwnd.0 as isize, pid, area);
        BOOL(1)
    }

    let mut ctx = Ctx { f: &mut f };
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut ctx as *mut Ctx<'_> as isize));
    }
}
