use crate::capture::wide;
use std::ptr::null_mut;
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*};

const BACKGROUND: u32 = 0x00231c0a;
const CYAN: u32 = 0x00ece078;
const GOLD: u32 = 0x008bd9f3;
const OUTLINE: u32 = 0x00827a3a;

/// GDI-only, one-line HUD. Fonts are retained; no animation or rendering loop.
pub struct Hud {
    name: String,
    quantity: String,
    font: HFONT,
    scale: f64,
    width: i32,
    quantity_width: i32,
    badge: Option<(String, u32)>,
    badge_width: i32,
}
impl Hud {
    pub unsafe fn new(scale: f64) -> Self {
        Self {
            name: String::new(),
            quantity: String::new(),
            scale,
            width: (300.0 * scale) as i32,
            quantity_width: 0,
            badge: None,
            badge_width: 0,
            font: CreateFontW(
                -(14.0 * scale).round() as i32,
                0,
                0,
                0,
                FW_SEMIBOLD as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                wide("Bahnschrift").as_ptr(),
            ),
        }
    }
    fn px(&self, value: i32) -> i32 {
        (value as f64 * self.scale).round() as i32
    }
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.px(28))
    }
    pub unsafe fn set_text(
        &mut self,
        name: String,
        quantity: String,
        badge: Option<(String, u32)>,
    ) -> bool {
        let name = name.to_uppercase();
        let quantity = quantity.to_uppercase();
        if self.name == name && self.quantity == quantity && self.badge == badge {
            return false;
        }
        self.name = name;
        self.quantity = quantity;
        self.badge = badge;
        let dc = CreateCompatibleDC(null_mut());
        let old = SelectObject(dc, self.font);
        let measure = |text: &str| {
            let text = wide(text);
            let mut size = SIZE::default();
            GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32 - 1, &mut size);
            size.cx
        };
        self.quantity_width = measure(&self.quantity);
        self.badge_width = self
            .badge
            .as_ref()
            .map(|(text, _)| measure(text) + self.px(24))
            .unwrap_or(0);
        self.width = (measure(&self.name) + self.quantity_width + self.badge_width + self.px(100))
            .clamp(self.px(280), self.px(720));
        SelectObject(dc, old);
        DeleteDC(dc);
        true
    }
    pub fn outline(&self) -> [POINT; 6] {
        let (w, h) = self.size();
        let cut = self.px(9);
        [
            POINT { x: 0, y: 0 },
            POINT { x: w, y: 0 },
            POINT { x: w, y: h - cut },
            POINT { x: w - cut, y: h },
            POINT { x: cut, y: h },
            POINT { x: 0, y: h - cut },
        ]
    }
    pub unsafe fn draw(&self, dc: HDC) {
        let (w, h) = self.size();
        let brush = CreateSolidBrush(BACKGROUND);
        FillRect(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            },
            brush,
        );
        DeleteObject(brush);
        let old_pen = SelectObject(dc, GetStockObject(DC_PEN));
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        SetDCPenColor(dc, OUTLINE);
        let edge = self.outline().map(|p| POINT {
            x: p.x.min(w - 1),
            y: p.y.min(h - 1),
        });
        Polygon(dc, edge.as_ptr(), edge.len() as i32);

        // Open corner brackets, a small crystal, and a fine top-edge highlight.
        SetDCPenColor(dc, CYAN);
        for points in [
            [
                POINT {
                    x: 0,
                    y: self.px(8),
                },
                POINT { x: 0, y: 0 },
                POINT {
                    x: self.px(32),
                    y: 0,
                },
            ],
            [
                POINT {
                    x: w - 1,
                    y: self.px(8),
                },
                POINT { x: w - 1, y: 0 },
                POINT {
                    x: w - self.px(32),
                    y: 0,
                },
            ],
        ] {
            Polyline(dc, points.as_ptr(), points.len() as i32);
        }
        MoveToEx(dc, w / 2 - self.px(22), 0, null_mut());
        LineTo(dc, w / 2 + self.px(22), 0);
        let cx = self.px(22);
        let cy = h / 2;
        let gem = [
            POINT {
                x: cx,
                y: cy - self.px(5),
            },
            POINT {
                x: cx + self.px(5),
                y: cy,
            },
            POINT {
                x: cx,
                y: cy + self.px(5),
            },
            POINT {
                x: cx - self.px(5),
                y: cy,
            },
        ];
        Polygon(dc, gem.as_ptr(), gem.len() as i32);
        let quantity_right = w - self.px(16) - self.badge_width;
        let divider = quantity_right - self.quantity_width - self.px(12);
        SetDCPenColor(dc, OUTLINE);
        MoveToEx(dc, divider, self.px(7), null_mut());
        LineTo(dc, divider, h - self.px(7));

        let old_font = SelectObject(dc, self.font);
        SetBkMode(dc, TRANSPARENT as i32);
        let mut labels = vec![
            (&self.name, CYAN, self.px(42), divider - self.px(12)),
            (&self.quantity, GOLD, divider + self.px(12), quantity_right),
        ];
        if let Some((text, color)) = &self.badge {
            let split = quantity_right + self.px(12);
            MoveToEx(dc, split, self.px(7), null_mut());
            LineTo(dc, split, h - self.px(7));
            labels.push((text, *color, split + self.px(12), w - self.px(16)));
        }
        for (label, color, left, right) in labels {
            SetTextColor(dc, color);
            let text = wide(label);
            let mut rect = RECT {
                left,
                top: 0,
                right,
                bottom: h - self.px(1),
            };
            DrawTextW(
                dc,
                text.as_ptr(),
                text.len() as i32 - 1,
                &mut rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
        SelectObject(dc, old_font);
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
    }
}
impl Drop for Hud {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.font);
        }
    }
}
