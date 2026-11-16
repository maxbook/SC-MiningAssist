use crate::config::Region;
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::Threading::*, UI::WindowsAndMessaging::*,
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[derive(Clone)]
pub struct Monitor {
    pub name: String,
    pub bounds: RECT,
}
pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn callback(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> i32 {
        let mut info = MONITORINFOEXW {
            monitorInfo: MONITORINFO {
                cbSize: size_of::<MONITORINFOEXW>() as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        if GetMonitorInfoW(h, &mut info.monitorInfo) != 0 {
            let n = info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szDevice.len());
            (*(data as *mut Vec<Monitor>)).push(Monitor {
                name: String::from_utf16_lossy(&info.szDevice[..n]),
                bounds: info.monitorInfo.rcMonitor,
            });
        }
        1
    }
    let mut result = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            null_mut(),
            std::ptr::null(),
            Some(callback),
            &mut result as *mut _ as isize,
        );
    }
    result
}
pub fn resolve_region(region: &Region) -> Option<(RECT, Monitor)> {
    if !region.valid() {
        return None;
    }
    let m = monitors().into_iter().find(|m| m.name == region.monitor)?;
    let w = (m.bounds.right - m.bounds.left) as f64;
    let h = (m.bounds.bottom - m.bounds.top) as f64;
    let r = RECT {
        left: m.bounds.left + (region.x * w).round() as i32,
        top: m.bounds.top + (region.y * h).round() as i32,
        right: m.bounds.left + ((region.x + region.width) * w).round() as i32,
        bottom: m.bounds.top + ((region.y + region.height) * h).round() as i32,
    };
    ((r.right - r.left) >= 8
        && (r.bottom - r.top) >= 8
        && (r.right - r.left) <= 1200
        && (r.bottom - r.top) <= 400)
        .then_some((r, m))
}

pub fn game_foreground() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() || IsIconic(hwnd) != 0 {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len);
        CloseHandle(process);
        ok != 0
            && String::from_utf16_lossy(&path[..len as usize])
                .rsplit('\\')
                .next()
                .is_some_and(|s| s.eq_ignore_ascii_case("StarCitizen.exe"))
    }
}

pub fn game_bounds() -> Option<RECT> {
    if !game_foreground() {
        return None;
    }
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut r = RECT::default();
        if GetClientRect(hwnd, &mut r) == 0 {
            return None;
        }
        let mut p = POINT::default();
        if ClientToScreen(hwnd, &mut p) == 0 {
            return None;
        }
        r.left += p.x;
        r.right += p.x;
        r.top += p.y;
        r.bottom += p.y;
        Some(r)
    }
}
/// Broad HUD envelope: 80% of the width and 70% of the height, including head look.
pub fn search_bounds(bounds: RECT) -> RECT {
    let w = bounds.right - bounds.left;
    let h = bounds.bottom - bounds.top;
    RECT {
        left: bounds.left + w / 10,
        right: bounds.right - w / 10,
        top: bounds.top + h * 8 / 100,
        bottom: bounds.top + h * 78 / 100,
    }
}

pub fn hud_bounds(bounds: RECT) -> RECT {
    let w = bounds.right - bounds.left;
    let h = bounds.bottom - bounds.top;
    RECT {
        left: bounds.left + w * 30 / 100,
        right: bounds.left + w * 70 / 100,
        top: bounds.top + h * 12 / 100,
        bottom: bounds.top + h * 62 / 100,
    }
}

pub fn tracking_bounds(found: RECT, broad: RECT) -> RECT {
    let pad_x = (broad.right - broad.left) / 9;
    let pad_y = (broad.bottom - broad.top) / 8;
    RECT {
        left: (found.left - pad_x).max(broad.left),
        top: (found.top - pad_y).max(broad.top),
        right: (found.right + pad_x).min(broad.right),
        bottom: (found.bottom + pad_y).min(broad.bottom),
    }
}

/// Cheap local search on each tracking tick, before the wider head-look search.
pub fn follow_bounds(found: RECT, broad: RECT) -> RECT {
    let h = found.bottom - found.top;
    let pad_x = (h * 3).clamp(24, 120);
    let pad_y = (h * 2).clamp(18, 80);
    RECT {
        left: (found.left - pad_x).max(broad.left),
        top: (found.top - pad_y).max(broad.top),
        right: (found.right + pad_x).min(broad.right),
        bottom: (found.bottom + pad_y).min(broad.bottom),
    }
}

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// One tiny GDI DIB reused across captures. No full-screen image in the scan loop.
pub struct Capture {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}
impl Capture {
    pub fn new(width: i32, height: i32) -> Result<Self, String> {
        if width < 8 || height < 8 || i64::from(width) * i64::from(height) > 24_000_000 {
            return Err("Unsupported capture dimensions".into());
        }
        unsafe {
            let screen = GetDC(null_mut());
            let memory = CreateCompatibleDC(screen);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if screen.is_null() || memory.is_null() || bitmap.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                if !memory.is_null() {
                    DeleteDC(memory);
                }
                if !screen.is_null() {
                    ReleaseDC(null_mut(), screen);
                }
                return Err("Screen capture unavailable".into());
            }
            let previous = SelectObject(memory, bitmap);
            Ok(Self {
                screen,
                memory,
                bitmap,
                previous,
                bits: bits.cast(),
                width,
                height,
            })
        }
    }
    pub fn read(&self, x: i32, y: i32) -> Result<Frame, String> {
        unsafe {
            if BitBlt(
                self.memory,
                0,
                0,
                self.width,
                self.height,
                self.screen,
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            ) == 0
            {
                return Err("Failed to capture this area".into());
            }
            GdiFlush();
            Ok(Frame {
                width: self.width as u32,
                height: self.height as u32,
                bgra: std::slice::from_raw_parts(
                    self.bits,
                    (self.width * self.height * 4) as usize,
                )
                .to_vec(),
            })
        }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.memory);
            ReleaseDC(null_mut(), self.screen);
        }
    }
}
