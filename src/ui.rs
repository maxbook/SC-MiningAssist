use crate::{
    capture::{self, wide},
    config::Settings,
    core::Catalog,
    difficulty,
    hud::Hud,
    panel::{self, Phase},
    worker::{self, Command, Event},
};
use std::{
    cell::{Cell, RefCell},
    mem::size_of,
    ptr::{null, null_mut},
    sync::mpsc::{Receiver, Sender},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        LibraryLoader::GetModuleHandleW,
        SystemServices::{SS_OWNERDRAW, SS_RIGHT},
        Threading::CreateMutexW,
    },
    UI::{
        Controls::DRAWITEMSTRUCT, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

const TRAY: u32 = WM_APP + 1;
const REOPEN: u32 = WM_APP + 3;
pub const WORKER_READY: u32 = WM_APP + 4;
const START: usize = 102;
const PREVIEW: usize = 103;
const QUIT: usize = 105;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static SCALE: Cell<f64> = const { Cell::new(1.0) };
}
fn px(v: i32) -> i32 {
    SCALE.with(|s| (v as f64 * s.get()).round() as i32)
}
struct InstanceHandle(HANDLE);
impl Drop for InstanceHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct App {
    main: HWND,
    overlay: HWND,
    status: HWND,
    start_button: HWND,
    font: HFONT,
    small_font: HFONT,
    brand_font: HFONT,
    brush: HBRUSH,
    settings: Settings,
    catalog: Catalog,
    running: bool,
    ready: bool,
    epoch: u64,
    sender: Sender<Command>,
    receiver: Receiver<Event>,
    hud: Hud,
    status_text: String,
    last: Option<Instant>,
    preview_until: Option<Instant>,
    hotkeys: bool,
    tray_ok: bool,
}

pub fn error(message: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(message).as_ptr(),
            wide("SC-MiningAssist").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
unsafe fn text(hwnd: HWND, s: &str) {
    SetWindowTextW(hwnd, wide(s).as_ptr());
}
unsafe fn font(size: i32, weight: u32) -> HFONT {
    CreateFontW(
        -px(size),
        0,
        0,
        0,
        weight as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide("Bahnschrift").as_ptr(),
    )
}
unsafe fn child(
    parent: HWND,
    class: &str,
    label: &str,
    style: u32,
    rect: [i32; 4],
    id: usize,
    font: HFONT,
) -> HWND {
    let h = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(label).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        px(rect[0]),
        px(rect[1]),
        px(rect[2]),
        px(rect[3]),
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(h, WM_SETFONT, font as usize, 1);
    h
}
pub fn run() -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        SCALE.with(|s| s.set(GetDpiForSystem() as f64 / 96.0));
        // Retain the pre-release window classes and mutex so the old and renamed
        // executables still share a single instance during an upgrade.
        let mutex = CreateMutexW(null(), 0, wide("Local\\MiningMod.v1").as_ptr());
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        if mutex.is_null() {
            return Err(crate::errors::io(std::io::Error::last_os_error()));
        }
        let _instance_handle = InstanceHandle(mutex);
        if already_running {
            // A second launch can arrive before the first has created its window.
            for _ in 0..50 {
                let existing = FindWindowW(wide("MiningMod.Main").as_ptr(), null());
                if !existing.is_null() {
                    let mut pid = 0;
                    GetWindowThreadProcessId(existing, &mut pid);
                    AllowSetForegroundWindow(pid);
                    PostMessageW(existing, REOPEN, 0, 0);
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            return Err("SC-MiningAssist is running, but its window is unavailable. Close that instance, then restart SC-MiningAssist.".into());
        }
        let instance = GetModuleHandleW(null());
        let brush = CreateSolidBrush(panel::BACKGROUND);
        let icon = LoadIconW(instance, 101 as *const u16);
        let classes: [(&str, WNDPROC); 2] = [
            ("MiningMod.Main", Some(main_proc)),
            ("MiningMod.Overlay", Some(overlay_proc)),
        ];
        for (name, proc) in classes {
            let class_name = wide(name);
            let class = WNDCLASSW {
                lpfnWndProc: proc,
                hInstance: instance,
                lpszClassName: class_name.as_ptr(),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hIcon: icon,
                hbrBackground: brush,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(crate::errors::io(std::io::Error::last_os_error()));
            }
        }
        let settings = Settings::load();
        let catalog_path = std::env::current_exe()
            .map_err(crate::errors::io)?
            .with_file_name("signatures.json");
        let catalog = if catalog_path.exists() {
            Catalog::from_json(&std::fs::read_to_string(catalog_path).map_err(crate::errors::io)?)?
        } else {
            Catalog::builtin()
        };
        let font = font(15, FW_MEDIUM);
        let small_font = self::font(11, FW_NORMAL);
        let brand_font = self::font(14, FW_BOLD);
        let main = CreateWindowExW(
            WS_EX_APPWINDOW,
            wide("MiningMod.Main").as_ptr(),
            wide("SC-MiningAssist").as_ptr(),
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
            (GetSystemMetrics(SM_CXSCREEN) - px(320)) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - px(176)) / 2,
            px(320),
            px(176),
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        let overlay = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            wide("MiningMod.Overlay").as_ptr(),
            wide("SC-MiningAssist · Overlay").as_ptr(),
            WS_POPUP,
            0,
            0,
            px(300),
            px(28),
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if main.is_null() || overlay.is_null() {
            return Err("Unable to create application windows".into());
        }
        SetLayeredWindowAttributes(overlay, 0, settings.opacity, LWA_ALPHA);
        SetWindowDisplayAffinity(overlay, WDA_EXCLUDEFROMCAPTURE);
        let status = child(
            main,
            "STATIC",
            "Starting…",
            SS_OWNERDRAW,
            [18, 44, 284, 48],
            panel::STATUS as usize,
            font,
        );
        SetWindowLongPtrW(status, GWLP_USERDATA, Phase::Initializing as isize);
        child(
            main,
            "STATIC",
            "SC-MININGASSIST",
            0,
            [43, 8, 170, 18],
            panel::BRAND as usize,
            brand_font,
        );
        for (id, title, x) in [
            (panel::MINIMIZE, "Minimize", 260),
            (panel::CLOSE, "Close", 288),
        ] {
            let button = child(
                main,
                "BUTTON",
                title,
                BS_OWNERDRAW as u32,
                [x, 5, 24, 24],
                id,
                font,
            );
            panel::style_button(button);
        }
        let start_button = child(
            main,
            "BUTTON",
            "Start scanning",
            WS_TABSTOP | BS_OWNERDRAW as u32,
            [18, 100, 210, 34],
            START,
            font,
        );
        panel::style_button(start_button);
        EnableWindow(start_button, 0);
        let quit_button = child(
            main,
            "BUTTON",
            "Quit",
            WS_TABSTOP | BS_OWNERDRAW as u32,
            [238, 100, 64, 34],
            QUIT,
            font,
        );
        panel::style_button(quit_button);
        child(
            main,
            "STATIC",
            "CTRL + ALT + F8",
            0,
            [18, 149, 140, 15],
            panel::FOOTER as usize,
            small_font,
        );
        child(
            main,
            "STATIC",
            "PAUSE / RESUME",
            SS_RIGHT,
            [182, 149, 120, 15],
            112,
            small_font,
        );
        let (sender, receiver, handle) = worker::spawn(catalog.clone(), main as usize);
        let hotkeys =
            RegisterHotKey(main, 1, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_F8 as u32) != 0;
        if !hotkeys {
            text(GetDlgItem(main, panel::FOOTER), "USE THE TRAY MENU");
            text(GetDlgItem(main, 112), "HOTKEY IN USE");
        }
        let mut tray = tray_data(main);
        tray.hIcon = icon;
        let tray_ok = Shell_NotifyIconW(NIM_ADD, &tray) != 0;
        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                main,
                overlay,
                status,
                start_button,
                font,
                small_font,
                brand_font,
                brush,
                settings,
                catalog,
                running: false,
                ready: false,
                epoch: 0,
                sender,
                receiver,
                hud: Hud::new(SCALE.with(|s| s.get())),
                status_text: String::new(),
                last: None,
                preview_until: None,
                hotkeys,
                tray_ok,
            })
        });
        SetTimer(main, 1, 200, None);
        ShowWindow(main, SW_SHOWNORMAL);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            let belongs_to_settings = msg.hwnd == main || IsChild(main, msg.hwnd) != 0;
            if !belongs_to_settings || IsDialogMessageW(main, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        APP.with(|a| {
            if let Some(a) = a.borrow_mut().take() {
                let _ = a.sender.send(Command::Stop);
                Shell_NotifyIconW(NIM_DELETE, &tray_data(main));
                UnregisterHotKey(main, 1);
                DestroyWindow(overlay);
                DeleteObject(a.font);
                DeleteObject(a.small_font);
                DeleteObject(a.brand_font);
                DeleteObject(a.brush);
            }
        });
        // Do not keep an invisible process (and its instance lock) alive if a
        // Windows OCR operation fails to finish during shutdown.
        let deadline = Instant::now() + Duration::from_secs(1);
        while !handle.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if handle.is_finished() {
            let _ = handle.join();
        }
        Ok(())
    }
}

unsafe fn tray_data(main: HWND) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: main,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY,
        ..Default::default()
    };
    let tip = wide("SC-MiningAssist · right-click for controls");
    data.szTip[..tip.len()].copy_from_slice(&tip);
    data
}

impl App {
    unsafe fn status(&mut self, s: &str, phase: Phase) {
        if self.status_text != s {
            SetWindowLongPtrW(self.status, GWLP_USERDATA, phase as isize);
            text(self.status, s);
            InvalidateRect(self.status, null(), 0);
            self.status_text = s.into();
        }
    }
    unsafe fn configure(&mut self) {
        self.epoch += 1;
        self.last = None;
        self.preview_until = None;
        ShowWindow(self.overlay, SW_HIDE);
        let _ = self.sender.send(Command::Configure(
            self.settings.clone(),
            self.running,
            self.epoch,
        ));
        if let Err(e) = self.settings.save() {
            self.status(&format!("Settings could not be saved: {e}"), Phase::Error);
        }
        text(
            self.start_button,
            if self.running {
                "Pause scanning"
            } else {
                "Start scanning"
            },
        );
    }
    unsafe fn toggle(&mut self, minimize: bool) {
        if !self.ready {
            self.status(
                "OCR unavailable. Check Windows OCR language options.",
                Phase::Error,
            );
            return;
        }
        self.running = !self.running;
        self.configure();
        if self.running && minimize {
            if self.tray_ok {
                ShowWindow(self.main, SW_HIDE);
            } else {
                ShowWindow(self.main, SW_MINIMIZE);
            }
        }
        self.status(
            if self.running {
                "Active · waiting for Star Citizen"
            } else {
                "Paused · no capture"
            },
            if self.running {
                Phase::Waiting
            } else {
                Phase::Paused
            },
        );
    }
    unsafe fn show_overlay(&mut self, value: u32, preview: bool) {
        let (name, quantity) = self.catalog.hud_text(value, self.settings.mode);
        let badge = difficulty::badge(&self.catalog, value, self.settings.mode);
        let changed = self.hud.set_text(name, quantity, badge);
        SetWindowDisplayAffinity(
            self.overlay,
            if preview {
                WDA_NONE
            } else {
                WDA_EXCLUDEFROMCAPTURE
            },
        );
        let monitor = None::<capture::Monitor>.or_else(|| {
            let bounds = capture::game_bounds();
            let all = capture::monitors();
            bounds
                .and_then(|b| {
                    all.iter()
                        .find(|m| {
                            b.left >= m.bounds.left
                                && b.left < m.bounds.right
                                && b.top >= m.bounds.top
                                && b.top < m.bounds.bottom
                        })
                        .cloned()
                })
                .or_else(|| all.into_iter().next())
        });
        if let Some(m) = monitor {
            let (width, height) = self.hud.size();
            let left = m.bounds.left + (m.bounds.right - m.bounds.left - width) / 2;
            let top = m.bounds.top;
            SetWindowPos(
                self.overlay,
                HWND_TOPMOST,
                left,
                top,
                width,
                height,
                SWP_NOACTIVATE,
            );
        }
        SetLayeredWindowAttributes(self.overlay, 0, self.settings.opacity, LWA_ALPHA);
        let needs_paint = changed || IsWindowVisible(self.overlay) == 0;
        if changed {
            let points = self.hud.outline();
            let region = CreatePolygonRgn(points.as_ptr(), points.len() as i32, WINDING);
            if SetWindowRgn(self.overlay, region, 0) == 0 {
                DeleteObject(region);
            }
        }
        if needs_paint {
            // Paint after releasing the App borrow; do not erase on every OCR tick.
            PostMessageW(self.overlay, WM_APP + 2, 0, 0);
        }
        ShowWindow(self.overlay, SW_SHOWNOACTIVATE);
    }
    unsafe fn tick(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Ready => {
                    self.ready = true;
                    EnableWindow(self.start_button, 1);
                    let note = if !self.hotkeys {
                        " · use the tray menu"
                    } else {
                        ""
                    };
                    self.status(&format!("Ready · mining + salvage{note}"), Phase::Ready);
                }
                Event::Error(e) => {
                    self.last = None;
                    ShowWindow(self.overlay, SW_HIDE);
                    self.status(&e, Phase::Error);
                }
                Event::Reading { epoch, value, at } => {
                    if epoch != self.epoch || at.elapsed() > Duration::from_secs(10) {
                        continue;
                    }
                    if self.running && capture::game_foreground() {
                        if let Some(v) = value {
                            self.last = Some(at);
                            self.show_overlay(v, false);
                            let matches = self.catalog.resolve(v, self.settings.mode);
                            if let [found] = matches.as_slice() {
                                self.status(&found.label(), Phase::Detected);
                            } else {
                                self.status("Ambiguous signal", Phase::Uncertain);
                            }
                        } else {
                            self.last = None;
                            ShowWindow(self.overlay, SW_HIDE);
                            self.status("Scanning · mining + salvage", Phase::Scanning);
                        }
                    }
                }
            }
        }
        if self.preview_until.is_some_and(|t| Instant::now() < t) {
            return;
        }
        self.preview_until = None;
        if self.running && !capture::game_foreground() {
            self.status("Active · waiting for Star Citizen", Phase::Waiting);
        }
        if !self.running
            || !capture::game_foreground()
            || self.last.is_none_or(|t| {
                // Watchdog for a stalled worker only. Normal disappearance is
                // decided by confirmed OCR loss, not a short UI lifetime.
                t.elapsed() > Duration::from_secs(10)
            })
        {
            ShowWindow(self.overlay, SW_HIDE);
        }
    }
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLOSE || (msg == WM_COMMAND && matches!(wp & 0xffff, QUIT | panel::CLOSE)) {
        // The taskbar's Close command, Alt+F4, the X and Quit all actually exit.
        EndMenu();
        DestroyWindow(hwnd);
        return 0;
    }
    if msg == WM_COMMAND && wp & 0xffff == panel::MINIMIZE {
        ShowWindow(hwnd, SW_MINIMIZE);
        return 0;
    }
    if msg == WM_NCHITTEST {
        let mut point = POINT {
            x: lp as i16 as i32,
            y: (lp >> 16) as i16 as i32,
        };
        ScreenToClient(hwnd, &mut point);
        return if point.y >= 0 && point.y < px(33) && point.x < px(256) {
            HTCAPTION
        } else {
            HTCLIENT
        } as isize;
    }
    // Painting can be requested synchronously while an event is updating controls.
    // These handlers deliberately do not borrow the application state.
    if msg == WM_DRAWITEM && lp != 0 {
        return panel::draw_item(&*(lp as *const DRAWITEMSTRUCT), SCALE.with(|s| s.get())) as isize;
    }
    if msg == WM_ERASEBKGND {
        return 1;
    }
    if msg == WM_PRINTCLIENT {
        panel::draw_frame(hwnd, wp as HDC, SCALE.with(|s| s.get()));
        return 0;
    }
    if msg == WM_PAINT {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        panel::draw_frame(hwnd, dc, SCALE.with(|s| s.get()));
        EndPaint(hwnd, &ps);
        return 0;
    }
    if msg == TRAY {
        let event = lp as u32 & 0xffff;
        if event == WM_LBUTTONDBLCLK {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        } else if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
            // TrackPopupMenu runs a nested message loop. Release App before it
            // starts so timer, close and command messages remain serviceable.
            let running = APP.with(|cell| cell.borrow().as_ref().is_some_and(|a| a.running));
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, 200, wide("Show SC-MiningAssist").as_ptr());
            AppendMenuW(
                menu,
                MF_STRING,
                START,
                wide(if running { "Pause" } else { "Start" }).as_ptr(),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(menu, MF_STRING, QUIT, wide("Quit SC-MiningAssist").as_ptr());
            let mut p = POINT::default();
            GetCursorPos(&mut p);
            SetForegroundWindow(hwnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                p.x,
                p.y,
                0,
                hwnd,
                null(),
            );
            DestroyMenu(menu);
            if command == 200 {
                PostMessageW(hwnd, REOPEN, 0, 0);
            } else if command != 0 {
                PostMessageW(hwnd, WM_COMMAND, command as usize, 0);
            }
            PostMessageW(hwnd, WM_NULL, 0, 0);
        }
        return 0;
    }
    // DestroyWindow sends this synchronously, including from inside an App borrow.
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return 0;
    }
    if msg == REOPEN {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
        return 0;
    }
    // STATIC controls can ask for their colors synchronously during SetWindowText.
    // Keep this reentrant path independent of the mutable application state.
    if msg == WM_CTLCOLORSTATIC {
        SetTextColor(
            wp as HDC,
            if GetDlgCtrlID(lp as HWND) == panel::BRAND {
                panel::TEXT
            } else {
                panel::MUTED
            },
        );
        SetBkColor(wp as HDC, panel::BACKGROUND);
        SetDCBrushColor(wp as HDC, panel::BACKGROUND);
        return GetStockObject(DC_BRUSH) as isize;
    }
    // DefWindowProc(WM_SYSCOMMAND/SC_CLOSE) reenters with WM_CLOSE. Never
    // retain the RefCell borrow across that call: it used to bypass our close
    // handler, destroy Main and leave the overlay/message loop running forever.
    if !matches!(msg, WM_TIMER | WORKER_READY | WM_COMMAND | WM_HOTKEY) {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    APP.with(|cell| {
        let Ok(mut borrow) = cell.try_borrow_mut() else {
            if msg != WM_TIMER {
                PostMessageW(hwnd, msg, wp, lp);
            }
            return 0;
        };
        let Some(a) = borrow.as_mut() else {
            drop(borrow);
            return DefWindowProcW(hwnd, msg, wp, lp);
        };
        match msg {
            WM_TIMER | WORKER_READY => {
                a.tick();
                0
            }
            WM_COMMAND => {
                let id = wp & 0xffff;
                match id {
                    START => a.toggle(true),
                    PREVIEW => {
                        a.show_overlay(10800, true);
                        a.preview_until = Some(Instant::now() + Duration::from_secs(5));
                    }
                    _ => {}
                };
                0
            }
            WM_HOTKEY => {
                if wp == 1 {
                    a.toggle(false);
                }
                0
            }
            _ => 0,
        }
    })
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_APP + 2 {
        InvalidateRect(hwnd, null(), 0);
        return 0;
    }
    if msg == WM_ERASEBKGND {
        return 1;
    }
    if msg == WM_PRINTCLIENT {
        APP.with(|cell| {
            if let Ok(borrow) = cell.try_borrow() {
                if let Some(a) = borrow.as_ref() {
                    a.hud.draw(wp as HDC);
                }
            }
        });
        return 0;
    }
    if msg == WM_NCHITTEST {
        return HTTRANSPARENT as isize;
    }
    if msg == WM_MOUSEACTIVATE {
        return MA_NOACTIVATE as isize;
    }
    if msg != WM_PAINT {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut ps);
    APP.with(|cell| {
        if let Ok(borrow) = cell.try_borrow() {
            if let Some(a) = borrow.as_ref() {
                let (w, h) = a.hud.size();
                let buffer = CreateCompatibleDC(dc);
                let bitmap = CreateCompatibleBitmap(dc, w, h);
                if buffer.is_null() || bitmap.is_null() {
                    a.hud.draw(dc);
                } else {
                    let old = SelectObject(buffer, bitmap);
                    a.hud.draw(buffer);
                    BitBlt(dc, 0, 0, w, h, buffer, 0, 0, SRCCOPY);
                    SelectObject(buffer, old);
                }
                DeleteObject(bitmap);
                DeleteDC(buffer);
            }
        }
    });
    EndPaint(hwnd, &ps);
    0
}
