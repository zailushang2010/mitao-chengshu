//! System tray: minimize-to-tray and quick actions.
//!
//! Important: when the main window is hidden (`Visible(false)`), eframe may stop
//! calling `App::update` regularly. Tray handlers therefore restore the window via
//! Win32 directly and call `Context::request_repaint()` so the UI loop wakes up.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::Context;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Must match the real window title set in `main.rs`.
pub const WINDOW_TITLE: &str = "suijiPotPlayer · 今日片单";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    StartOrStop,
    Reroll,
    StopSession,
    Exit,
}

pub struct TrayService {
    _tray: TrayIcon,
    /// Menu items must outlive the tray menu on Windows.
    _menu_items: Vec<MenuItem>,
    pub rx: Receiver<TrayCommand>,
    _tx: Sender<TrayCommand>,
    /// Shared flag so UI can know we were asked to show (optional).
    pub show_requested: Arc<AtomicBool>,
}

impl TrayService {
    pub fn try_new(ctx: Context) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let show_requested = Arc::new(AtomicBool::new(false));

        let menu = Menu::new();
        let show = MenuItem::new("显示主窗口", true, None);
        let toggle = MenuItem::new("开启 / 关闭本轮", true, None);
        let reroll = MenuItem::new("再来一轮", true, None);
        let stop = MenuItem::new("关闭本轮", true, None);
        let exit = MenuItem::new("退出", true, None);

        menu.append(&show).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&toggle).map_err(|e| e.to_string())?;
        menu.append(&reroll).map_err(|e| e.to_string())?;
        menu.append(&stop).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&exit).map_err(|e| e.to_string())?;

        let show_id = show.id().clone();
        let toggle_id = toggle.id().clone();
        let reroll_id = reroll.id().clone();
        let stop_id = stop.id().clone();
        let exit_id = exit.id().clone();
        let menu_items = vec![show, toggle, reroll, stop, exit];

        let icon = make_icon();
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("suijiPotPlayer · 今日片单（点击显示窗口）")
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;

        let dispatch = |tx: &Sender<TrayCommand>,
                        ctx: &Context,
                        flag: &AtomicBool,
                        cmd: TrayCommand| {
            if matches!(cmd, TrayCommand::Show | TrayCommand::Exit) {
                // Restore HWND immediately — do not wait for App::update.
                force_show_main_window();
                flag.store(true, Ordering::SeqCst);
            }
            let _ = tx.send(cmd);
            ctx.request_repaint();
            // A second kick in case the first paint was missed while hidden.
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                force_show_main_window();
                ctx2.request_repaint();
            });
        };

        // Menu events
        let tx_menu = tx.clone();
        let ctx_menu = ctx.clone();
        let flag_menu = show_requested.clone();
        std::thread::spawn(move || {
            let menu_rx = MenuEvent::receiver();
            while let Ok(ev) = menu_rx.recv() {
                let cmd = if ev.id == show_id {
                    Some(TrayCommand::Show)
                } else if ev.id == toggle_id {
                    Some(TrayCommand::StartOrStop)
                } else if ev.id == reroll_id {
                    Some(TrayCommand::Reroll)
                } else if ev.id == stop_id {
                    Some(TrayCommand::StopSession)
                } else if ev.id == exit_id {
                    Some(TrayCommand::Exit)
                } else {
                    None
                };
                if let Some(c) = cmd {
                    dispatch(&tx_menu, &ctx_menu, &flag_menu, c);
                    if matches!(c, TrayCommand::Exit) {
                        // still keep loop; process will exit from UI
                    }
                }
            }
        });

        // Tray icon click / double-click → show
        let tx_icon = tx.clone();
        let ctx_icon = ctx.clone();
        let flag_icon = show_requested.clone();
        std::thread::spawn(move || {
            let icon_rx = TrayIconEvent::receiver();
            while let Ok(ev) = icon_rx.recv() {
                let show = match ev {
                    TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => true,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => true,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        ..
                    } => true,
                    _ => false,
                };
                if show {
                    dispatch(&tx_icon, &ctx_icon, &flag_icon, TrayCommand::Show);
                }
            }
        });

        Ok(Self {
            _tray: tray,
            _menu_items: menu_items,
            rx,
            _tx: tx,
            show_requested,
        })
    }
}

/// Show / restore the main window by title (works even when eframe marked it invisible).
pub fn force_show_main_window() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
        SW_SHOWNORMAL,
    };

    let title: Vec<u16> = WINDOW_TITLE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let Ok(hwnd) = FindWindowW(None, PCWSTR(title.as_ptr())) else {
            // Fallback: enum windows containing our brand string
            if let Some(h) = find_window_containing("suijiPotPlayer") {
                show_hwnd(h);
            }
            return;
        };
        if hwnd.0.is_null() {
            if let Some(h) = find_window_containing("suijiPotPlayer") {
                show_hwnd(h);
            }
            return;
        }
        show_hwnd(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = ShowWindow(hwnd, SW_SHOW);
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        // Nudge z-order
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOP, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        };
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
    }
}

fn show_hwnd(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL,
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = ShowWindow(hwnd, SW_SHOW);
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

fn find_window_containing(part: &str) -> Option<windows::Win32::Foundation::HWND> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, IsWindowVisible,
    };

    struct Ctx {
        part: String,
        found: Option<HWND>,
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut Ctx);
        if ctx.found.is_some() {
            return BOOL(0);
        }
        // Include hidden windows (IsWindowVisible may be false when in tray)
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n > 0 {
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            if title.contains(&ctx.part) {
                ctx.found = Some(hwnd);
                return BOOL(0);
            }
        }
        let _ = IsWindowVisible(hwnd); // silence unused in some builds
        BOOL(1)
    }

    let mut ctx = Ctx {
        part: part.to_string(),
        found: None,
    };
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut ctx as *mut Ctx as isize));
    }
    ctx.found
}

fn make_icon() -> Icon {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            rgba[i] = 0xF7;
            rgba[i + 1] = 0xF3;
            rgba[i + 2] = 0xEC;
            rgba[i + 3] = 0xFF;
            if x >= 8 && x < 24 && y >= 8 && y < 24 {
                rgba[i] = 0x1C;
                rgba[i + 1] = 0x19;
                rgba[i + 2] = 0x17;
            }
            if x >= 10 && x < 22 && y >= 14 && y < 18 {
                rgba[i] = 0xF7;
                rgba[i + 1] = 0xF3;
                rgba[i + 2] = 0xEC;
            }
        }
    }
    Icon::from_rgba(rgba, size, size).expect("icon")
}
