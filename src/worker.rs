use crate::{
    capture::{self, Capture, Frame},
    config::Settings,
    core::{Catalog, StableReading},
    ocr::Ocr,
    tracker::{Observation, Tracker},
};
use std::{
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::Foundation::RECT;

pub enum Command {
    Configure(Settings, bool, u64),
    Stop,
}
pub enum Event {
    Ready,
    Reading {
        epoch: u64,
        value: Option<u32>,
        at: Instant,
    },
    Error(String),
}
pub fn spawn(
    catalog: Catalog,
    window: usize,
) -> (Sender<Command>, Receiver<Event>, thread::JoinHandle<()>) {
    let (tx, commands) = mpsc::channel();
    let (events, rx) = mpsc::sync_channel(4);
    let handle = thread::spawn(move || {
        let ocr = match Ocr::new() {
            Ok(o) => o,
            Err(e) => {
                let _ = events.send(Event::Error(e));
                notify(window);
                return;
            }
        };
        let _ = events.send(Event::Ready);
        notify(window);
        scan_loop(ocr, catalog, commands, events, window);
    });
    (tx, rx, handle)
}
fn notify(window: usize) {
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
            window as _,
            crate::ui::WORKER_READY,
            0,
            0,
        );
    }
}
fn scan_loop(
    ocr: Ocr,
    catalog: Catalog,
    commands: Receiver<Command>,
    events: SyncSender<Event>,
    window: usize,
) {
    let mut settings = Settings::default();
    let mut running = false;
    let mut epoch = 0;
    let mut stable = StableReading::default();
    let mut capture: Option<Capture> = None;
    let mut size = (0, 0);
    let mut next = Instant::now();
    let mut tracker = Tracker::default();
    let mut previous_bounds = (0, 0, 0, 0);
    loop {
        let wait = if running {
            next.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100))
        } else {
            Duration::from_millis(100)
        };
        match commands.recv_timeout(wait) {
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Command::Configure(s, run, e)) => {
                settings = s;
                running = run;
                epoch = e;
                stable.reset();
                capture = None;
                tracker.reset();
                next = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if !running || !capture::game_foreground() {
            stable.reset();
            tracker.reset();
            capture = None;
            next = Instant::now() + Duration::from_millis(100);
            continue;
        }
        if Instant::now() < next {
            continue;
        }
        let started = Instant::now();
        let result = (|| {
            let at = Instant::now();
            let raw = if let Some(region) = settings.region.as_ref() {
                let (rect, _) = capture::resolve_region(region)
                    .ok_or("Monitor disconnected: switch back to automatic search.")?;
                let frame = read_rect(&mut capture, &mut size, rect)?;
                ocr.read(&frame)?.1
            } else {
                let bounds = capture::game_bounds().ok_or("Game window unavailable")?;
                let key = (bounds.left, bounds.top, bounds.right, bounds.bottom);
                if key != previous_bounds {
                    tracker.reset();
                    stable.reset();
                    previous_bounds = key;
                }
                match tracker.scan(&ocr, &catalog, settings.mode, bounds, at, |rect| {
                    read_rect(&mut capture, &mut size, rect)
                })? {
                    Observation::Skipped => return Ok(None),
                    Observation::Read {
                        value,
                        ambiguous,
                        stage: _,
                    } => {
                        if ambiguous {
                            stable.reset();
                        }
                        value
                    }
                }
            };
            let completed = Instant::now();
            let value = stable.push(raw, completed);
            let value = if capture::game_foreground() {
                value
            } else {
                stable.reset();
                None
            };
            Ok::<_, String>(Some(Event::Reading {
                epoch,
                value,
                at: completed,
            }))
        })();
        // Fast confirmation/tracking, quieter idle search. Never run a backlog
        // of overdue OCR work after an expensive fallback.
        next = (started + tracker.interval()).max(Instant::now() + Duration::from_millis(20));
        match result {
            Ok(Some(event)) => {
                let _ = events.try_send(event);
                notify(window);
            }
            Ok(None) => {}
            Err(e) => {
                stable.reset();
                tracker.reset();
                capture = None;
                let _ = events.try_send(Event::Error(e));
                notify(window);
                next = Instant::now() + Duration::from_secs(3);
            }
        }
    }
}
fn read_rect(
    capture: &mut Option<Capture>,
    size: &mut (i32, i32),
    rect: RECT,
) -> Result<Frame, String> {
    let new_size = (rect.right - rect.left, rect.bottom - rect.top);
    if capture.is_none() || *size != new_size {
        *capture = Some(Capture::new(new_size.0, new_size.1)?);
        *size = new_size;
    }
    capture.as_ref().unwrap().read(rect.left, rect.top)
}
