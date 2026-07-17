use std::path::{Path, PathBuf};
use std::process::Command;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_TERMINATE,
};

const CANDIDATES: &[&str] = &[
    r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe",
    r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini.exe",
    r"C:\Program Files (x86)\DAUM\PotPlayer\PotPlayerMini.exe",
];

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

pub fn launch_many(
    potplayer: &Path,
    files: &[PathBuf],
) -> (Vec<LaunchedItem>, Vec<String>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();

    for file in files {
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

fn kill_pid(pid: u32) -> Result<(), String> {
    unsafe {
        let handle: HANDLE = OpenProcess(PROCESS_TERMINATE, false, pid)
            .map_err(|e| e.to_string())?;
        let result = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        result.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Best-effort: find top-level windows belonging to the given PIDs.
pub fn find_hwnds_for_pids(pids: &[u32], attempts: u32, delay_ms: u64) -> Vec<(u32, isize)> {
    use std::thread;
    use std::time::Duration;

    let mut found: Vec<(u32, isize)> = Vec::new();
    let want: std::collections::HashSet<u32> = pids.iter().copied().collect();

    for _ in 0..attempts {
        let mut batch = Vec::new();
        enum_top_level_windows(|hwnd, pid| {
            if want.contains(&pid) && !found.iter().any(|(p, h)| *p == pid && *h == hwnd) {
                // Prefer first window per pid; keep one primary per pid
                if !batch.iter().any(|(p, _): &(u32, isize)| *p == pid)
                    && !found.iter().any(|(p, _)| *p == pid)
                {
                    batch.push((pid, hwnd));
                }
            }
        });
        found.extend(batch);

        if found.len() >= want.len() {
            break;
        }
        thread::sleep(Duration::from_millis(delay_ms));
    }

    found
}

fn enum_top_level_windows(mut f: impl FnMut(isize, u32)) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
    };

    struct Ctx<'a> {
        f: &'a mut dyn FnMut(isize, u32),
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut Ctx<'_>);
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return BOOL(1);
        }
        // Visible, top-level (no owner)
        if IsWindowVisible(hwnd).as_bool() {
            let owner = GetWindow(hwnd, GW_OWNER).unwrap_or(HWND::default());
            if owner.0.is_null() {
                (ctx.f)(hwnd.0 as isize, pid);
            }
        }
        BOOL(1)
    }

    let mut ctx = Ctx { f: &mut f };
    unsafe {
        let _ = EnumWindows(
            Some(callback),
            LPARAM(&mut ctx as *mut Ctx<'_> as isize),
        );
    }
}
