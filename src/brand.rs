//! App branding: display name + shared icon bytes.

use std::sync::OnceLock;

/// 产品显示名（窗口标题 / 托盘 / 关于）
pub const APP_NAME: &str = "蜜桃成熟";

/// 窗口标题（与托盘 FindWindow 一致）
pub const WINDOW_TITLE: &str = "蜜桃成熟";

/// 单实例互斥量
pub const MUTEX_NAME: &str = "Local\\MiTaoChengShu_SingleInstance_v1";

/// 第二实例唤醒第一实例窗口的命名事件
pub const SHOW_EVENT_NAME: &str = "Local\\MiTaoChengShu_ShowWindow_v1";

/// Embedded application icon (source: src/icon.ico)
pub const ICON_ICO: &[u8] = include_bytes!("icon.ico");

#[derive(Clone)]
pub struct RgbaIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

fn decode_icon() -> Option<RgbaIcon> {
    let img = image::load_from_memory_with_format(ICON_ICO, image::ImageFormat::Ico).ok()?;
    // Prefer a reasonable tray/window size: take largest side <= 256, or full image
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(RgbaIcon {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    })
}

static ICON: OnceLock<Option<RgbaIcon>> = OnceLock::new();

pub fn app_icon() -> Option<&'static RgbaIcon> {
    ICON.get_or_init(decode_icon).as_ref()
}

/// egui / eframe window icon
pub fn egui_icon_data() -> Option<egui::IconData> {
    let ic = app_icon()?;
    Some(egui::IconData {
        rgba: ic.rgba.clone(),
        width: ic.width,
        height: ic.height,
    })
}

/// tray-icon crate icon
pub fn tray_icon() -> Option<tray_icon::Icon> {
    let ic = app_icon()?;
    tray_icon::Icon::from_rgba(ic.rgba.clone(), ic.width, ic.height).ok()
}
