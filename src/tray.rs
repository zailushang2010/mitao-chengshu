//! System tray + Win32 window raise helpers.
//!
//! When the main window is hidden, eframe may stop calling `App::update` often.
//! Tray handlers restore via Win32 and `request_repaint`.
//! After launching many PotPlayer windows, focus is stolen — use topmost +
//! AttachThreadInput to bring the control panel back.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crate::brand::{self, APP_NAME, WINDOW_TITLE};
use egui::Context;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::HWND;

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
    _menu_items: Vec<MenuItem>,
    pub rx: Receiver<TrayCommand>,
    _tx: Sender<TrayCommand>,
    pub show_requested: Arc<AtomicBool>,
}

impl TrayService {
    pub fn try_new(ctx: Context) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let show_requested = Arc::new(AtomicBool::new(false));

        let menu = Menu::new();
        let show = MenuItem::new("显示控制面板", true, None);
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

        let icon = brand::tray_icon().unwrap_or_else(fallback_tray_icon);
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(&format!("{APP_NAME}（点击显示控制面板）"))
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;

        let dispatch = |tx: &Sender<TrayCommand>,
                        ctx: &Context,
                        flag: &AtomicBool,
                        cmd: TrayCommand| {
            if matches!(
                cmd,
                TrayCommand::Show
                    | TrayCommand::Exit
                    | TrayCommand::StartOrStop
                    | TrayCommand::Reroll
                    | TrayCommand::StopSession
            ) {
                // Cut through PotPlayer windows immediately
                force_show_main_window();
                flag.store(true, Ordering::SeqCst);
            }
            let _ = tx.send(cmd);
            ctx.request_repaint();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                for delay in [30u64, 120, 350, 800] {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    force_show_main_window();
                    ctx2.request_repaint();
                }
            });
        };

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
                }
            }
        });

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

/// Show / restore control window above PotPlayer instances.
/// Flash TOPMOST then clear — caller may re-pin with `set_main_window_topmost(true)`.
pub fn force_show_main_window() {
    let _ = force_show_main_window_result();
}

/// Returns whether a main window HWND was found and raise was attempted.
pub fn force_show_main_window_result() -> bool {
    let Some(hwnd) = find_main_hwnd() else {
        return false;
    };
    raise_hwnd(hwnd, true);
    set_topmost(hwnd, false);
    true
}

/// Raise and keep on top (for 播放中可操作). Pair with clear when minimized / idle.
pub fn force_show_and_pin() {
    let Some(hwnd) = find_main_hwnd() else {
        return;
    };
    raise_hwnd(hwnd, true);
    set_topmost(hwnd, true);
}

/// Explicitly set/clear always-on-top.
/// Does NOT restore/show — safe to call while minimized (only clears z-order flag).
pub fn set_main_window_topmost(topmost: bool) {
    let Some(hwnd) = find_main_hwnd() else {
        return;
    };
    set_topmost(hwnd, topmost);
}

fn find_main_hwnd() -> Option<HWND> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title: Vec<u16> = WINDOW_TITLE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        if let Ok(hwnd) = FindWindowW(None, PCWSTR(title.as_ptr())) {
            if !hwnd.0.is_null() {
                return Some(hwnd);
            }
        }
    }
    find_window_containing(APP_NAME)
}

fn raise_hwnd(hwnd: HWND, flash_topmost: bool) {
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
        SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOPMOST,
        SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL,
    };

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = ShowWindow(hwnd, SW_SHOW);
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }

        use windows::Win32::UI::WindowsAndMessaging::HWND_NOTOPMOST;

        // Brief TOPMOST to surface above PotPlayer, then always clear it
        if flash_topmost {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }

        // Classic focus-stealing workaround
        let fg = GetForegroundWindow();
        let mut fg_pid = 0u32;
        let fg_tid = GetWindowThreadProcessId(fg, Some(&mut fg_pid));
        let cur_tid = GetCurrentThreadId();
        if fg_tid != 0 && fg_tid != cur_tid {
            let _ = AttachThreadInput(cur_tid, fg_tid, true);
        }
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        // Release always-on-top so user can minimize / switch windows
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        if fg_tid != 0 && fg_tid != cur_tid {
            let _ = AttachThreadInput(cur_tid, fg_tid, false);
        }
    }
}

fn set_topmost(hwnd: HWND, topmost: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };
    unsafe {
        let insert = if topmost {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        let _ = SetWindowPos(
            hwnd,
            Some(insert),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
    }
}

fn find_window_containing(part: &str) -> Option<HWND> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW};

    struct Ctx {
        part: String,
        found: Option<HWND>,
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut Ctx);
        if ctx.found.is_some() {
            return BOOL(0);
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n > 0 {
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            if title.contains(&ctx.part) {
                ctx.found = Some(hwnd);
                return BOOL(0);
            }
        }
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

fn fallback_tray_icon() -> tray_icon::Icon {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let cx = x as f32 - 15.5;
            let cy = y as f32 - 16.5;
            let r2 = cx * cx + cy * cy;
            if r2 < 12.0 * 12.0 {
                rgba[i] = 0xF4;
                rgba[i + 1] = 0x8F;
                rgba[i + 2] = 0xA0;
                rgba[i + 3] = 0xFF;
            } else {
                rgba[i] = 0xF7;
                rgba[i + 1] = 0xF3;
                rgba[i + 2] = 0xEC;
                rgba[i + 3] = 0xFF;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("fallback icon")
}
