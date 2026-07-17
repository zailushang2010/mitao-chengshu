use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, ShowWindow, SystemParametersInfoW, HWND_TOP, SPI_GETWORKAREA, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_RESTORE, SW_SHOWNORMAL, SYSTEM_PARAMETERS_INFO_ACTION,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
}

pub fn work_area() -> Result<Rect, String> {
    unsafe {
        let mut r = RECT::default();
        SystemParametersInfoW(
            SYSTEM_PARAMETERS_INFO_ACTION(SPI_GETWORKAREA.0),
            0,
            Some(&mut r as *mut RECT as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .map_err(|e| e.to_string())?;
        Ok(Rect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        })
    }
}

/// Choose rows/cols for n tiles (spec heuristics).
pub fn rows_cols(n: usize) -> (usize, usize) {
    match n {
        0 => (0, 0),
        1 => (1, 1),
        2 => (1, 2),
        3 => (1, 3),
        4 => (2, 2),
        5 | 6 => (2, 3),
        7 | 8 => (2, 4),
        9 => (3, 3),
        10 => (2, 5),
        _ => {
            let cols = (n as f64).sqrt().ceil() as usize;
            let rows = n.div_ceil(cols);
            (rows, cols.max(1))
        }
    }
}

pub fn grid_layout(n: usize, area: Rect) -> Vec<Rect> {
    if n == 0 || area.width() <= 0 || area.height() <= 0 {
        return Vec::new();
    }
    let (rows, cols) = rows_cols(n);
    let cell_w = area.width() / cols as i32;
    let cell_h = area.height() / rows as i32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let r = i / cols;
        let c = i % cols;
        let left = area.left + c as i32 * cell_w;
        let top = area.top + r as i32 * cell_h;
        let right = if c + 1 == cols {
            area.right
        } else {
            left + cell_w
        };
        let bottom = if r + 1 == rows {
            area.bottom
        } else {
            top + cell_h
        };
        out.push(Rect {
            left,
            top,
            right,
            bottom,
        });
    }
    out
}

pub fn tile_hwnds(hwnds: &[isize], rects: &[Rect]) {
    // Front-to-back first
    for (hwnd, rect) in hwnds.iter().zip(rects.iter()) {
        if *hwnd == 0 {
            continue;
        }
        let _ = move_window(*hwnd, *rect);
    }
    // Last slot again — last-launched instance most often jumps out after open
    if let (Some(&h), Some(r)) = (hwnds.last(), rects.last()) {
        if h != 0 {
            let _ = move_window(h, *r);
        }
    }
}

/// Place windows; multiple passes for stubborn / late PotPlayer instances.
pub fn tile_hwnds_stable(hwnds: &[isize], rects: &[Rect]) {
    tile_hwnds(hwnds, rects);
    std::thread::sleep(std::time::Duration::from_millis(220));
    tile_hwnds(hwnds, rects);
    std::thread::sleep(std::time::Duration::from_millis(180));
    // Hammer the last window twice more (common offender)
    if let (Some(&h), Some(r)) = (hwnds.last(), rects.last()) {
        if h != 0 {
            let _ = move_window(h, *r);
            std::thread::sleep(std::time::Duration::from_millis(120));
            let _ = move_window(h, *r);
        }
    }
}

fn move_window(hwnd: isize, rect: Rect) -> Result<(), String> {
    unsafe {
        let h = HWND(hwnd as *mut _);
        // Always restore — last instance often opens maximized / remembered fullscreen
        let _ = ShowWindow(h, SW_RESTORE);
        let _ = ShowWindow(h, SW_SHOWNORMAL);
        let flags = SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_FRAMECHANGED;
        SetWindowPos(
            h,
            Some(HWND_TOP),
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            flags,
        )
        .map_err(|e| e.to_string())?;
        // Immediate second apply — PotPlayer rewrites geometry on first message
        SetWindowPos(
            h,
            Some(HWND_TOP),
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            flags,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_six_is_two_by_three() {
        assert_eq!(rows_cols(6), (2, 3));
        let area = Rect {
            left: 0,
            top: 0,
            right: 900,
            bottom: 600,
        };
        let cells = grid_layout(6, area);
        assert_eq!(cells.len(), 6);
        for c in &cells {
            assert!(c.left >= area.left);
            assert!(c.top >= area.top);
            assert!(c.right <= area.right);
            assert!(c.bottom <= area.bottom);
            assert!(c.width() > 0);
            assert!(c.height() > 0);
        }
        assert_eq!(cells[0].right, cells[1].left);
        assert_eq!(cells[1].right, cells[2].left);
    }

    #[test]
    fn grid_ten_is_two_by_five() {
        assert_eq!(rows_cols(10), (2, 5));
    }
}
