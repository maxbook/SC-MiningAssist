use crate::{
    core::{Catalog, Mode, StableReading},
    ocr::{self, Ocr},
    tracker::{Observation, Tracker},
};
use serde::Deserialize;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::RECT;

#[derive(Deserialize)]
struct Step {
    image: String,
    at_ms: u64,
}

/// Runs the same tracker and stabilizer as the live worker on a deterministic
/// sequence of supplied images. Never captures the desktop or changes settings.
pub fn sequence_report(args: &[String]) -> Result<serde_json::Value, String> {
    let path = args
        .first()
        .ok_or("--ocr-sequence MANIFEST.json --output report.json")?;
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(path).map_err(crate::errors::io)?)
        .map_err(|e| e.to_string())?;
    if steps.is_empty() || steps.len() > 1000 {
        return Err("Sequence is empty or too long".into());
    }
    if steps.windows(2).any(|w| w[1].at_ms < w[0].at_ms) {
        return Err("Sequence timestamps must be nondecreasing".into());
    }
    let engine = Ocr::new()?;
    let catalog = Catalog::builtin();
    let mut tracker = Tracker::default();
    let mut stable = StableReading::default();
    let start = Instant::now();
    let mut result = Vec::new();
    let mut previous_size = (0, 0);
    for step in steps {
        let frame = ocr::load_frame(&step.image)?;
        if previous_size != (frame.width, frame.height) {
            stable.reset();
            previous_size = (frame.width, frame.height);
        }
        let bounds = RECT {
            left: 0,
            top: 0,
            right: frame.width as i32,
            bottom: frame.height as i32,
        };
        let now = start
            .checked_add(Duration::from_millis(step.at_ms))
            .ok_or("Sequence timestamp is too large")?;
        let timer = Instant::now();
        let mut captures = 0;
        let observation = tracker.scan(&engine, &catalog, Mode::Auto, bounds, now, |r| {
            captures += 1;
            if r.left < 0 || r.top < 0 || r.right > bounds.right || r.bottom > bounds.bottom {
                return Err("Capture area is outside the image".into());
            }
            Ok(ocr::crop(
                &frame,
                r.left as u32,
                r.top as u32,
                (r.right - r.left) as u32,
                (r.bottom - r.top) as u32,
            ))
        })?;
        let row = match observation {
            Observation::Skipped => {
                serde_json::json!({"at_ms":step.at_ms,"stage":"skipped","captures":captures})
            }
            Observation::Read {
                value,
                ambiguous,
                stage,
            } => {
                if ambiguous {
                    stable.reset();
                }
                let confirmed = stable.push(value, now);
                serde_json::json!({"at_ms":step.at_ms,"raw":value,"stable":confirmed,"ambiguous":ambiguous,"stage":stage,"elapsed_ms":timer.elapsed().as_millis(),"captures":captures})
            }
        };
        result.push(row);
    }
    Ok(serde_json::json!({"catalog_version":catalog.version,"frames":result}))
}
