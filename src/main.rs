#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod brand;
mod config;
mod history;
mod index_cache;
mod library;
mod picker;
mod potplayer;
mod session;
mod thumb;
mod tiler;
mod tray;

use brand::{APP_NAME, MUTEX_NAME, SHOW_EVENT_NAME, WINDOW_TITLE};
use eframe::egui;
use session::SessionHandle;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result<()> {
    init_log();
    log_line("starting");

    // Named mutex: reliable single-instance (survives crash better than bare PID file)
    let _mutex = match acquire_mutex() {
        MutexAcquire::Ok(m) => m,
        MutexAcquire::AlreadyRunning => {
            log_line("already running — signal + focus existing");
            let signaled = signal_show_event();
            log_line(&format!("show event signaled={signaled}"));
            // Give first instance a moment to poll the event
            std::thread::sleep(std::time::Duration::from_millis(80));
            let focused = tray::force_show_main_window_result();
            log_line(&format!("focus existing hwnd_ok={focused}"));
            if !focused {
                message_box(
                    APP_NAME,
                    "程序已在运行。\n\n若看不到窗口，请查看任务栏或托盘区（右下角小图标）。\n已尝试把已有窗口带到前台。",
                );
            }
            return Ok(());
        }
        MutexAcquire::Error(e) => {
            log_line(&format!("mutex error (continue anyway): {e}"));
            None
        }
    };

    // Second-instance wake flag (named event waiter → AtomicBool)
    let show_from_second = Arc::new(AtomicBool::new(false));
    spawn_show_event_waiter(show_from_second.clone());

    // Clean stale lock files from older versions
    let _ = std::fs::remove_file(config::app_data_dir().join("suiji_potplayer.lock"));
    let _ = std::fs::remove_file(config::app_data_dir().join("mitao_chengshu.lock"));

    // Config is the single source of truth. Never inject hard-coded disks (e.g. F:\电影).
    // Do not log full library paths (privacy).
    let cfg_path = config::config_path();
    log_line(&format!("config: {}", cfg_path.display()));

    let cfg = if cfg_path.is_file() {
        match config::load_from(&cfg_path) {
            Some(c) => {
                log_line("config: loaded");
                c
            }
            None => {
                // Keep broken file as .bak; do not silently invent a new library path
                log_line("config: unreadable → defaults (backup: config.json.bak)");
                let c = config::Config::default().normalize();
                let _ = config::save(&c);
                c
            }
        }
    } else {
        log_line("config: missing → create empty defaults");
        let c = config::Config::default().normalize();
        let _ = config::save(&c);
        c
    };

    let n_roots = cfg.library_roots().len();
    log_line(&format!(
        "session: mode={:?} movie_roots={} count={} range={}-{} | image count={} range={}-{}",
        cfg.media_mode,
        n_roots,
        cfg.default_count,
        cfg.count_min,
        cfg.count_max,
        cfg.image_default_count,
        cfg.image_count_min,
        cfg.image_count_max
    ));

    let session = SessionHandle::new(cfg);
    // SessionHandle::new already starts a background scan for the active mode.
    if n_roots == 0 {
        log_line("session: no library — UI will open settings");
    } else {
        let snap = session.snapshot();
        log_line(&format!(
            "session: indexing in background · roots={} · mode={:?}",
            snap.library_roots.len(),
            snap.media_mode
        ));
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([540.0, 780.0])
        .with_min_inner_size([500.0, 620.0])
        .with_max_inner_size([680.0, 1200.0])
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

    let show_flag = show_from_second;
    let result = eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(move |cc| {
            log_line("eframe created");
            Ok(Box::new(app::SuijiApp::new(cc, session, show_flag)))
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

/// Second instance → wake first instance via named auto-reset event.
fn signal_show_event() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};

    let wide: Vec<u16> = SHOW_EVENT_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let Ok(h) = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(wide.as_ptr())) else {
            return false;
        };
        let ok = SetEvent(h).is_ok();
        let _ = CloseHandle(h);
        ok
    }
}

/// First instance: wait on named event forever; set AtomicBool for UI to raise window.
fn spawn_show_event_waiter(flag: Arc<AtomicBool>) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        CreateEventW, WaitForSingleObject, INFINITE,
    };

    std::thread::spawn(move || {
        let wide: Vec<u16> = SHOW_EVENT_NAME
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            // Auto-reset event; create if missing
            CreateEventW(None, false, false, PCWSTR(wide.as_ptr()))
        };
        let Ok(handle) = handle else {
            log_line("show event: CreateEvent failed");
            return;
        };
        log_line("show event: waiter started");
        loop {
            let wait = unsafe { WaitForSingleObject(handle, INFINITE) };
            if wait == WAIT_OBJECT_0 {
                flag.store(true, Ordering::SeqCst);
                log_line("show event: signaled by second instance");
            } else {
                break;
            }
        }
        unsafe {
            let _ = CloseHandle(handle);
        }
    });
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
