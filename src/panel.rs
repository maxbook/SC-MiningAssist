use crate::capture::wide;
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{Controls::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

pub const BACKGROUND: u32 = 0x001b1710;
pub const TEXT: u32 = 0x00eee9d8;
pub const MUTED: u32 = 0x009c9281;
const CYAN: u32 = 0x00e8d87c;
const GOLD: u32 = 0x008bcbe8;
const BORDER: u32 = 0x004f4430;
pub const STATUS: i32 = 107;
pub const BRAND: i32 = 108;
pub const FOOTER: i32 = 109;
pub const MINIMIZE: usize = 106;
pub const CLOSE: usize = 104;

#[derive(Clone, Copy)]
#[repr(isize)]
pub enum Phase {
    Initializing,
    Ready,
    Waiting,
    Scanning,
    Paused,
    Detected,
    Uncertain,
    Error,
}

fn px(value: i32, scale: f64) -> i32 {
    (value as f64 * scale).round() as i32
}
unsafe fn fill(dc: HDC, rect: &RECT, color: u32) {
    SetDCBrushColor(dc, color);
    FillRect(dc, rect, GetStockObject(DC_BRUSH));
}
unsafe fn line(dc: HDC, x: i32, y: i32, right: i32, bottom: i32) {
    MoveToEx(dc, x, y, null_mut());
    LineTo(dc, right, bottom);
}
unsafe fn label(dc: HDC, text: &str, font: HFONT, color: u32, mut rect: RECT, flags: u32) {
    let old = SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT as i32);
    let text = wide(text);
    DrawTextW(
        dc,
        text.as_ptr(),
        text.len() as i32 - 1,
        &mut rect,
        flags | DT_NOPREFIX,
    );
    SelectObject(dc, old);
}
unsafe fn caption(hwnd: HWND) -> String {
    let mut text = [0u16; 1024];
    let n = GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
    String::from_utf16_lossy(&text[..n.max(0) as usize])
}

pub unsafe fn draw_frame(hwnd: HWND, dc: HDC, scale: f64) {
    let mut r = RECT::default();
    GetClientRect(hwnd, &mut r);
    fill(dc, &r, BACKGROUND);
    let pen = SelectObject(dc, GetStockObject(DC_PEN));
    let brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
    SetDCPenColor(dc, BORDER);
    Rectangle(dc, 0, 0, r.right, r.bottom);
    line(
        dc,
        px(16, scale),
        px(33, scale),
        r.right - px(16, scale),
        px(33, scale),
    );
    SetDCPenColor(dc, CYAN);
    line(dc, 0, px(14, scale), 0, 0);
    line(dc, 0, 0, px(38, scale), 0);
    line(
        dc,
        r.right - px(24, scale),
        r.bottom - 1,
        r.right - 1,
        r.bottom - 1,
    );
    line(
        dc,
        r.right - 1,
        r.bottom - 1,
        r.right - 1,
        r.bottom - px(10, scale),
    );
    let icon = GetClassLongPtrW(hwnd, GCLP_HICON) as HICON;
    DrawIconEx(
        dc,
        px(15, scale),
        px(7, scale),
        icon,
        px(20, scale),
        px(20, scale),
        0,
        null_mut(),
        DI_NORMAL,
    );
    SelectObject(dc, brush);
    SelectObject(dc, pen);
}

pub unsafe fn draw_item(item: &DRAWITEMSTRUCT, scale: f64) -> bool {
    let dc = item.hDC;
    let r = item.rcItem;
    let p = |v| px(v, scale);
    if item.CtlID == STATUS as u32 {
        fill(dc, &r, BACKGROUND);
        let message = caption(item.hwndItem);
        let phase = GetWindowLongPtrW(item.hwndItem, GWLP_USERDATA);
        let (title, detail, color) = match phase {
            1 => ("READY", "Mining + salvage · automatic", CYAN),
            2 => ("STANDING BY", "Waiting for Star Citizen", GOLD),
            3 => ("SCANNING", "Searching for a signature", CYAN),
            4 => ("PAUSED", "Detection is paused", MUTED),
            5 => ("SIGNAL ACQUIRED", message.as_str(), CYAN),
            6 => ("MULTIPLE MATCHES", "Waiting for a clear reading", GOLD),
            7 => ("ATTENTION", message.as_str(), 0x009198ed),
            _ => ("INITIALIZING", "Preparing the scanner…", MUTED),
        };
        let pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let brush = SelectObject(dc, GetStockObject(DC_BRUSH));
        SetDCBrushColor(dc, color);
        Ellipse(dc, p(1), p(6), p(7), p(12));
        SelectObject(dc, brush);
        SelectObject(dc, pen);
        let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
        let small = SendMessageW(
            GetDlgItem(GetParent(item.hwndItem), FOOTER),
            WM_GETFONT,
            0,
            0,
        ) as HFONT;
        label(
            dc,
            title,
            font,
            TEXT,
            RECT {
                left: p(18),
                top: 0,
                right: r.right,
                bottom: p(20),
            },
            DT_SINGLELINE | DT_VCENTER,
        );
        label(
            dc,
            detail,
            small,
            MUTED,
            RECT {
                left: p(18),
                top: p(22),
                right: r.right,
                bottom: r.bottom,
            },
            DT_WORDBREAK | DT_END_ELLIPSIS,
        );
        return true;
    }
    if item.CtlType != ODT_BUTTON {
        return false;
    }
    let id = item.CtlID as usize;
    let down = item.itemState & ODS_SELECTED != 0;
    let disabled = item.itemState & ODS_DISABLED != 0;
    let hover = GetWindowLongPtrW(item.hwndItem, GWLP_USERDATA) != 0;
    let title_button = id == MINIMIZE || id == CLOSE;
    let primary = id == 102;
    let bg = if title_button {
        if hover || down {
            0x00372d20
        } else {
            BACKGROUND
        }
    } else if primary {
        if disabled {
            0x00251f17
        } else if down {
            0x00473e1e
        } else if hover {
            0x00524725
        } else {
            0x003c3420
        }
    } else if down || hover {
        0x00342a1e
    } else {
        0x00221c13
    };
    fill(dc, &r, BACKGROUND);
    let pen = SelectObject(dc, GetStockObject(DC_PEN));
    let brush = SelectObject(dc, GetStockObject(DC_BRUSH));
    SetDCPenColor(
        dc,
        if disabled {
            BORDER
        } else if primary || hover {
            CYAN
        } else {
            BORDER
        },
    );
    SetDCBrushColor(dc, bg);
    if title_button {
        fill(dc, &r, bg);
        let cx = r.right / 2;
        let cy = r.bottom / 2;
        SetDCPenColor(dc, if hover { TEXT } else { MUTED });
        if id == CLOSE {
            line(dc, cx - p(4), cy - p(4), cx + p(4) + 1, cy + p(4) + 1);
            line(dc, cx - p(4), cy + p(4), cx + p(4) + 1, cy - p(4) - 1);
        } else {
            line(dc, cx - p(5), cy + p(3), cx + p(5), cy + p(3));
        }
    } else {
        let cut = p(5);
        let points = [
            POINT { x: 0, y: 0 },
            POINT {
                x: r.right - cut - 1,
                y: 0,
            },
            POINT {
                x: r.right - 1,
                y: cut,
            },
            POINT {
                x: r.right - 1,
                y: r.bottom - 1,
            },
            POINT {
                x: cut,
                y: r.bottom - 1,
            },
            POINT {
                x: 0,
                y: r.bottom - cut - 1,
            },
        ];
        Polygon(dc, points.as_ptr(), points.len() as i32);
        let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
        let text = caption(item.hwndItem);
        let mut tr = r;
        if primary {
            tr.left += p(18);
            SetDCBrushColor(dc, if disabled { MUTED } else { CYAN });
            SetDCPenColor(dc, if disabled { MUTED } else { CYAN });
            let x = p(17);
            let y = r.bottom / 2;
            if text.starts_with("Pause") {
                fill(
                    dc,
                    &RECT {
                        left: x - p(3),
                        top: y - p(4),
                        right: x - p(1),
                        bottom: y + p(4),
                    },
                    CYAN,
                );
                fill(
                    dc,
                    &RECT {
                        left: x + p(2),
                        top: y - p(4),
                        right: x + p(4),
                        bottom: y + p(4),
                    },
                    CYAN,
                );
            } else {
                let triangle = [
                    POINT {
                        x: x - p(3),
                        y: y - p(4),
                    },
                    POINT { x: x + p(4), y },
                    POINT {
                        x: x - p(3),
                        y: y + p(4),
                    },
                ];
                Polygon(dc, triangle.as_ptr(), 3);
            }
        }
        if down {
            tr.top += p(1);
        }
        label(
            dc,
            &text,
            font,
            if disabled {
                MUTED
            } else if primary {
                CYAN
            } else {
                TEXT
            },
            tr,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if item.itemState & ODS_FOCUS != 0 {
            SetDCPenColor(dc, GOLD);
            line(dc, p(12), r.bottom - p(4), r.right - p(12), r.bottom - p(4));
        }
    }
    SelectObject(dc, brush);
    SelectObject(dc, pen);
    true
}

pub unsafe fn style_button(hwnd: HWND) {
    SetWindowSubclass(hwnd, Some(button_proc), 1, 0);
}
unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            if GetWindowLongPtrW(hwnd, GWLP_USERDATA) == 0 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 1);
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut track);
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
        }
        WM_MOUSELEAVE => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            InvalidateRect(hwnd, std::ptr::null(), 0);
        }
        WM_SETCURSOR if IsWindowEnabled(hwnd) != 0 => {
            SetCursor(LoadCursorW(null_mut(), IDC_HAND));
            return 1;
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(button_proc), 1);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}
