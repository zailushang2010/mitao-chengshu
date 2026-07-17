#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod brand;
mod config;
mod history;
mod library;
mod picker;
mod potplayer;
mod session;
mod thumb;
mod tiler;
mod tray;

use brand::{APP_NAME, MUTEX_NAME, WINDOW_TITLE};
use eframe::egui;
use session::SessionHandle;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

fn main() -> eframe::Result<()> {
    init_log();
    log_line("starting");

    // Named mutex: reliable single-instance (survives crash better than bare PID file)
    let _mutex = match acquire_mutex() {
        MutexAcquire::Ok(m) => m,
        MutexAcquire::AlreadyRunning => {
            log_line("already running — focusing existing window");
            focus_existing_window();
            message_box(
                APP_NAME,
                "程序已在运行。\n\n若看不到窗口，请查看任务栏或托盘区（右下角小图标）。\n已尝试把已有窗口带到前台。",
            );
            return Ok(());
        }
        MutexAcquire::Error(e) => {
            log_line(&format!("mutex error (continue anyway): {e}"));
            None
        }
    };

    // Clean stale lock files from older versions
    let _ = std::fs::remove_file(config::app_data_dir().join("suiji_potplayer.lock"));
    let _ = std::fs::remove_file(config::app_data_dir().join("mitao_chengshu.lock"));

    let mut cfg = config::load_or_default();
    // Ensure demo / known library path if empty
    if !cfg.has_library() {
        let demo = r"F:\电影";
        if std::path::Path::new(demo).is_dir() {
            cfg.add_library_path(demo.to_string());
            let _ = config::save(&cfg);
            log_line("set library_paths to include F:\\电影");
        }
    }
    if !config::config_path().exists() {
        let _ = config::save(&cfg);
    }
    log_line(&format!(
        "config library_paths={:?} count={}",
        cfg.library_roots(),
        cfg.default_count
    ));

    let session = SessionHandle::new(cfg);
    if !session.snapshot().library_roots.is_empty() {
        session.rescan();
        log_line(&format!(
            "indexed {} videos from {} roots",
            session.snapshot().library_count,
            session.snapshot().library_roots.len()
        ));
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([520.0, 720.0])
        .with_min_inner_size([480.0, 560.0])
        .with_max_inner_size([640.0, 980.0])
        .with_resizable(true)
        .with_visible(true)
        .with_active(true)
        .with_title(WINDOW_TITLE);

    if let Some(icon) = brand::egui_icon_data() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let native_options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    let result = eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(|cc| {
            log_line("eframe created");
            Ok(Box::new(app::SuijiApp::new(cc, session)))
        }),
    );

    // Keep mutex alive until process end
    drop(_mutex);

    if let Err(ref e) = result {
        let msg = format!("启动失败：{e}\n\n详细日志：同目录 startup.log");
        log_line(&msg);
        message_box(&format!("{APP_NAME} 启动失败"), &msg);
    }
    result
}

enum MutexAcquire {
    Ok(Option<MutexGuard>),
    AlreadyRunning,
    Error(String),
}

/// Holds the Win32 mutex handle for process lifetime.
struct MutexGuard {
    handle: windows::Win32::Foundation::HANDLE,
}

impl Drop for MutexGuard {
    fn drop(&mut self) {
        unsafe {
            use windows::Win32::Foundation::CloseHandle;
            use windows::Win32::System::Threading::ReleaseMutex;
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

fn acquire_mutex() -> MutexAcquire {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;

    let wide: Vec<u16> = MUTEX_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        match CreateMutexW(None, true, PCWSTR(wide.as_ptr())) {
            Ok(handle) => {
                let err = GetLastError();
                if err == ERROR_ALREADY_EXISTS {
                    let _ = windows::Win32::Foundation::CloseHandle(handle);
                    MutexAcquire::AlreadyRunning
                } else {
                    MutexAcquire::Ok(Some(MutexGuard { handle }))
                }
            }
            Err(e) => MutexAcquire::Error(e.to_string()),
        }
    }
}

fn focus_existing_window() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };

    let title: Vec<u16> = WINDOW_TITLE.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let Ok(hwnd) = FindWindowW(None, PCWSTR(title.as_ptr())) else {
            log_line("FindWindow: no window with title");
            return;
        };
        if hwnd.0.is_null() {
            log_line("FindWindow: null hwnd");
            return;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        log_line("focused existing window");
    }
}

fn message_box(title: &str, body: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    let t: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let b: Vec<u16> = body.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(b.as_ptr()),
            PCWSTR(t.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

static LOG: Mutex<Option<std::fs::File>> = Mutex::new(None);

fn init_log() {
    let path = config::app_data_dir().join("startup.log");
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
        *LOG.lock().unwrap() = Some(f);
    }
}

fn log_line(msg: &str) {
    let line = format!(
        "[{}] {}\n",
        chrono_like_now(),
        msg
    );
    if let Ok(mut g) = LOG.lock() {
        if let Some(f) = g.as_mut() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }
    #[cfg(debug_assertions)]
    eprint!("{line}");
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
