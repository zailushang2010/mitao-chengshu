#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod history;
mod library;
mod picker;
mod potplayer;
mod session;
mod tiler;

use eframe::egui;
use session::SessionHandle;

fn main() -> eframe::Result<()> {
    // Simple single-instance via lock file next to exe
    let lock_path = config::app_data_dir().join("suiji_potplayer.lock");
    let _lock = match single_instance_lock(&lock_path) {
        Ok(l) => l,
        Err(_) => {
            eprintln!("suijiPotPlayer 已在运行");
            return Ok(());
        }
    };

    let cfg = config::load_or_default();
    if !config::config_path().exists() {
        let _ = config::save(&cfg);
    }

    let session = SessionHandle::new(cfg);
    // Initial scan if path set
    if !session.snapshot().library_root.is_empty() {
        session.rescan();
    }

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([420.0, 600.0])
        .with_min_inner_size([400.0, 560.0])
        .with_max_inner_size([480.0, 720.0])
        .with_resizable(true)
        .with_title("suijiPotPlayer · 今日片单");

    let native_options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "suijiPotPlayer",
        native_options,
        Box::new(|cc| Ok(Box::new(app::SuijiApp::new(cc, session)))),
    )
}

struct LockFile {
    path: std::path::PathBuf,
    file: std::fs::File,
}

impl Drop for LockFile {
    fn drop(&mut self) {
        // Ensure file handle is released before removing the path.
        let _ = self.file.sync_all();
        let _ = std::fs::remove_file(&self.path);
    }
}

fn single_instance_lock(path: &std::path::Path) -> Result<LockFile, ()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    if path.exists() {
        // Stale lock: if we cannot write exclusive, assume running.
        // On Windows, try create new and fail if exists — also check pid liveness loosely.
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                if process_alive(pid) {
                    return Err(());
                }
            }
        }
        let _ = std::fs::remove_file(path);
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| ())?;
    let _ = writeln!(file, "{}", std::process::id());
    Ok(LockFile {
        path: path.to_path_buf(),
        file,
    })
}

fn process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => false,
        }
    }
}
