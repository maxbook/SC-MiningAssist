use crate::{
    capture::{self, Frame},
    core::{Catalog, Mode},
    ocr::{Candidate, Ocr},
};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::RECT;

#[derive(Debug)]
pub enum Observation {
    Skipped,
    Read {
        value: Option<u32>,
        ambiguous: bool,
        stage: &'static str,
    },
}
#[derive(Default)]
pub struct Tracker {
    target: Option<(RECT, Instant)>,
    bounds: Option<(i32, i32, i32, i32)>,
    wide_after: Option<Instant>,
    fallback_after: Option<Instant>,
}
impl Tracker {
    pub fn interval(&self) -> Duration {
        Duration::from_millis(if self.target.is_some() { 125 } else { 250 })
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    fn accept(
        &mut self,
        found: Vec<Candidate>,
        rect: RECT,
        now: Instant,
        stage: &'static str,
    ) -> Option<Observation> {
        match found.as_slice() {
            [f] => {
                self.target = Some((
                    RECT {
                        left: rect.left + f.x as i32,
                        top: rect.top + f.y as i32,
                        right: rect.left + (f.x + f.width) as i32,
                        bottom: rect.top + (f.y + f.height) as i32,
                    },
                    now,
                ));
                Some(Observation::Read {
                    value: Some(f.value),
                    ambiguous: false,
                    stage,
                })
            }
            [] => None,
            _ => {
                self.target = None;
                Some(Observation::Read {
                    value: None,
                    ambiguous: true,
                    stage,
                })
            }
        }
    }
    pub fn scan(
        &mut self,
        ocr: &Ocr,
        catalog: &Catalog,
        mode: Mode,
        bounds: RECT,
        now: Instant,
        mut capture: impl FnMut(RECT) -> Result<Frame, String>,
    ) -> Result<Observation, String> {
        let key = (bounds.left, bounds.top, bounds.right, bounds.bottom);
        if self.bounds != Some(key) {
            self.reset();
            self.bounds = Some(key);
        }
        let broad = capture::search_bounds(bounds);
        if self
            .target
            .is_some_and(|(_, seen)| now.duration_since(seen) >= Duration::from_secs(5))
        {
            self.target = None;
        }
        let attempted = self.target.is_some();
        if let Some((rect, _)) = self.target {
            // Recenter immediately for ordinary head motion, independently of the
            // expensive whole-HUD search timer. Keep pending targets through misses.
            let local = capture::follow_bounds(rect, broad);
            let frame = capture(local)?;
            let found = ocr.locate_fast(&frame, catalog, mode)?;
            if let Some(reading) = self.accept(found.candidates, local, now, "local tracking") {
                return Ok(reading);
            }
            let cx = (rect.left + rect.right) / 2 - local.left;
            let cy = (rect.top + rect.bottom) / 2 - local.top;
            if found.measurements.iter().any(|r| {
                cx >= r.x as i32
                    && cx < (r.x + r.width) as i32
                    && cy >= r.y as i32
                    && cy < (r.y + r.height) as i32
            }) {
                // Explicit contradictory context is not an OCR dropout. Do not
                // refresh or retain a signature when its text becomes a distance.
                self.target = None;
                self.wide_after = None;
                return Ok(Observation::Read {
                    value: None,
                    ambiguous: true,
                    stage: "non-signature text",
                });
            }
            let found = ocr.locate_quick(&frame, catalog, mode)?.0;
            if let Some(reading) = self.accept(found, local, now, "local OCR") {
                return Ok(reading);
            }
        }
        if self.wide_after.is_some_and(|next| now < next) {
            return Ok(if attempted {
                Observation::Read {
                    value: None,
                    ambiguous: false,
                    stage: "signal lost",
                }
            } else {
                Observation::Skipped
            });
        }
        self.wide_after = Some(now + Duration::from_millis(250));
        if let Some((rect, _)) = self.target {
            let medium = capture::tracking_bounds(rect, broad);
            let found = ocr.locate_quick(&capture(medium)?, catalog, mode)?.0;
            if let Some(reading) = self.accept(found, medium, now, "nearby search") {
                return Ok(reading);
            }
        }
        let frame = capture(broad)?;
        if self.target.is_none() {
            let hud = capture::hud_bounds(bounds);
            let focused = crate::ocr::crop(
                &frame,
                (hud.left - broad.left) as u32,
                (hud.top - broad.top) as u32,
                (hud.right - hud.left) as u32,
                (hud.bottom - hud.top) as u32,
            );
            let found = ocr.locate_fast(&focused, catalog, mode)?.candidates;
            if let Some(reading) = self.accept(found, hud, now, "visual HUD search") {
                return Ok(reading);
            }
        }
        let found = ocr.locate_fast(&frame, catalog, mode)?.candidates;
        if let Some(reading) = self.accept(found, broad, now, "visual search") {
            return Ok(reading);
        }
        if self.fallback_after.is_some_and(|next| now < next) {
            return Ok(Observation::Read {
                value: None,
                ambiguous: false,
                stage: "search",
            });
        }
        self.fallback_after = Some(now + Duration::from_secs(1));
        let found = ocr.locate(&frame, catalog, mode)?.0;
        Ok(self
            .accept(found, broad, now, "wide search")
            .unwrap_or(Observation::Read {
                value: None,
                ambiguous: false,
                stage: "search",
            }))
    }
}
