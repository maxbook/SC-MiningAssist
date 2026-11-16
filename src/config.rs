use crate::core::Mode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Region {
    pub monitor: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Region {
    pub fn valid(&self) -> bool {
        !self.monitor.is_empty()
            && [self.x, self.y, self.width, self.height]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= 1.001
            && self.y + self.height <= 1.001
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub region: Option<Region>,
    pub interval_ms: u64,
    pub opacity: u8,
    pub mode: Mode,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            region: None,
            interval_ms: 125,
            opacity: 215,
            mode: Mode::Auto,
        }
    }
}
impl Settings {
    pub fn path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("SC-MiningAssist")
            .join("settings.json")
    }
    pub fn load() -> Self {
        let mut s: Self = std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if ![125, 250, 500, 1000, 2000].contains(&s.interval_ms) {
            s.interval_ms = Self::default().interval_ms;
        }
        s.opacity = s.opacity.clamp(100, 255);
        if s.region.as_ref().is_some_and(|r| !r.valid()) {
            s.region = None;
        }
        // The compact app is always automatic; old UI preferences must not
        // silently constrain detection after upgrading.
        s.region = None;
        s.mode = Mode::Auto;
        s.interval_ms = 125;
        s.opacity = 215;
        s
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        std::fs::create_dir_all(path.parent().unwrap()).map_err(crate::errors::io)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(crate::errors::io)?;
        let from = crate::capture::wide(tmp.to_string_lossy().as_ref());
        let to = crate::capture::wide(path.to_string_lossy().as_ref());
        unsafe {
            if windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            ) == 0
            {
                return Err(crate::errors::io(std::io::Error::last_os_error()));
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_invalid_capture_geometry() {
        let mut r = Region {
            monitor: "test".into(),
            x: 0.5,
            y: 0.5,
            width: 0.1,
            height: 0.1,
        };
        assert!(r.valid());
        r.width = 0.8;
        assert!(!r.valid());
        r.width = f64::NAN;
        assert!(!r.valid());
    }
}
