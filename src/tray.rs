//! System tray: minimize-to-tray and quick actions.

use std::sync::mpsc::{self, Receiver, Sender};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

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
    pub rx: Receiver<TrayCommand>,
    /// Keep sender alive for internal threads
    _tx: Sender<TrayCommand>,
}

impl TrayService {
    pub fn try_new() -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();

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

        let icon = make_icon();
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("suijiPotPlayer · 今日片单")
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;

        // Menu events
        let tx_menu = tx.clone();
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
                    if tx_menu.send(c).is_err() {
                        break;
                    }
                }
            }
        });

        // Tray icon click → show
        let tx_icon = tx.clone();
        std::thread::spawn(move || {
            let icon_rx = TrayIconEvent::receiver();
            while let Ok(ev) = icon_rx.recv() {
                if matches!(
                    ev,
                    TrayIconEvent::DoubleClick { .. } | TrayIconEvent::Click { .. }
                ) {
                    if tx_icon.send(TrayCommand::Show).is_err() {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            _tray: tray,
            rx,
            _tx: tx,
        })
    }
}

fn make_icon() -> Icon {
    // 32×32 paper-magazine style: warm cream + dark square mark
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            // background #F7F3EC
            rgba[i] = 0xF7;
            rgba[i + 1] = 0xF3;
            rgba[i + 2] = 0xEC;
            rgba[i + 3] = 0xFF;
            // inner ink tile
            if x >= 8 && x < 24 && y >= 8 && y < 24 {
                rgba[i] = 0x1C;
                rgba[i + 1] = 0x19;
                rgba[i + 2] = 0x17;
            }
            // small accent bar
            if x >= 10 && x < 22 && y >= 14 && y < 18 {
                rgba[i] = 0xF7;
                rgba[i + 1] = 0xF3;
                rgba[i + 2] = 0xEC;
            }
        }
    }
    Icon::from_rgba(rgba, size, size).expect("icon")
}
