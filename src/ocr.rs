use crate::{
    capture::Frame,
    core::{parse_signature, Catalog, Mode},
};
use std::time::Instant;
use windows::{
    core::HSTRING,
    Globalization::Language,
    Graphics::Imaging::*,
    Media::Ocr::OcrEngine,
    Storage::{FileAccessMode, StorageFile, Streams::DataWriter},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

// Drop the COM engine before uninitializing its apartment.
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}
pub struct Ocr {
    engine: OcrEngine,
    _apartment: Apartment,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Candidate {
    pub value: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
pub struct FastReading {
    pub candidates: Vec<Candidate>,
    pub measurements: Vec<crate::vision::TextBox>,
    pub trace: String,
}

fn is_measurement(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    ["km", "m/s", "m", "scu", "%", "k", "°"].iter().any(|unit| {
        text.strip_suffix(unit).is_some_and(|number| {
            let number = number.trim();
            parse_signature(number).is_some()
                || number
                    .split_whitespace()
                    .last()
                    .and_then(parse_signature)
                    .is_some()
        })
    })
}
impl Ocr {
    pub fn new() -> Result<Self, String> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED).map_err(crate::errors::windows)?;
        }
        let engine = Language::CreateLanguage(&HSTRING::from("en-US"))
            .and_then(|lang| OcrEngine::TryCreateFromLanguage(&lang))
            .or_else(|_| OcrEngine::TryCreateFromUserProfileLanguages());
        match engine {
            Ok(engine) => Ok(Self {
                engine,
                _apartment: Apartment,
            }),
            Err(e) => {
                unsafe {
                    RoUninitialize();
                }
                let e = crate::errors::windows(e);
                Err(format!("Windows OCR unavailable. Install optical character recognition in Windows language options (English or French). {e}"))
            }
        }
    }
    pub fn read(&self, frame: &Frame) -> Result<(String, Option<u32>), String> {
        self.read_crop(frame, None)
    }
    pub fn read_known(
        &self,
        frame: &Frame,
        catalog: &Catalog,
        mode: Mode,
    ) -> Result<(String, Option<u32>), String> {
        self.read_crop(frame, Some((catalog, mode)))
    }
    fn read_crop(
        &self,
        frame: &Frame,
        known: Option<(&Catalog, Mode)>,
    ) -> Result<(String, Option<u32>), String> {
        self.read_variants(frame, known, false)
    }
    fn read_variants(
        &self,
        frame: &Frame,
        known: Option<(&Catalog, Mode)>,
        fast: bool,
    ) -> Result<(String, Option<u32>), String> {
        let mut last = String::new();
        let variants = [(3, false), (3, true), (2, false), (1, false), (2, true)];
        for &(scale, threshold) in &variants[..if fast { 1 } else { variants.len() }] {
            let scale = scale.min((2400 / frame.width).max(1));
            let (data, width, height) = preprocess(frame, threshold, scale);
            let writer = DataWriter::new().map_err(crate::errors::windows)?;
            writer.WriteBytes(&data).map_err(crate::errors::windows)?;
            let buffer = writer.DetachBuffer().map_err(crate::errors::windows)?;
            let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
                &buffer,
                BitmapPixelFormat::Bgra8,
                width as i32,
                height as i32,
            )
            .map_err(crate::errors::windows)?;
            let result = self
                .engine
                .RecognizeAsync(&bitmap)
                .and_then(|op| op.join())
                .map_err(crate::errors::windows)?;
            let lines = result.Lines().map_err(crate::errors::windows)?;
            last = result.Text().map_err(crate::errors::windows)?.to_string();
            // Do not concatenate unrelated HUD lines into one false reading.
            let value = if lines.Size().unwrap_or(0) == 1 {
                parse_signature(&last)
            } else {
                None
            };
            let _ = bitmap.Close();
            if value.is_some_and(|v| {
                known.is_none_or(|(catalog, mode)| !catalog.resolve(v, mode).is_empty())
            }) {
                return Ok((last, value));
            }
        }
        Ok((last, None))
    }
    pub fn locate(
        &self,
        frame: &Frame,
        catalog: &Catalog,
        mode: Mode,
    ) -> Result<(Vec<Candidate>, String), String> {
        let (mut found, mut trace) = self.locate_pass(frame, catalog, mode)?;
        if found.is_empty() {
            let (contrast, detail) = self.locate_contrast(frame, catalog, mode)?;
            found = contrast;
            trace.push_str(&format!(" | contrast: {detail}"));
        }
        if !found.is_empty() {
            return Ok((found, trace));
        }
        // Low-resolution HUD digits can disappear in a global OCR pass. Four
        // overlapping tiles preserve their size, without an unbounded upscale.
        let tile_w = frame.width * 55 / 100;
        let tile_h = frame.height * 55 / 100;
        for y in [0, frame.height - tile_h] {
            for x in [0, frame.width - tile_w] {
                let tile = crop(frame, x, y, tile_w, tile_h);
                let (local, detail) = self.locate_quick(&tile, catalog, mode)?;
                trace.push_str(&format!(" | tile {x},{y}: {detail}"));
                for mut c in local {
                    c.x += x;
                    c.y += y;
                    if !found.iter().any(|f| {
                        f.value == c.value && f.x.abs_diff(c.x) < 35 && f.y.abs_diff(c.y) < 35
                    }) {
                        found.push(c);
                    }
                }
            }
        }
        Ok((found, trace))
    }
    pub fn locate_fast(
        &self,
        frame: &Frame,
        catalog: &Catalog,
        mode: Mode,
    ) -> Result<FastReading, String> {
        let regions = crate::vision::text_regions(frame);
        let mut found = Vec::new();
        let mut trace = String::new();
        let mut measurements = Vec::new();
        for r in regions {
            let small = crop(frame, r.x, r.y, r.width, r.height);
            let (text, value) = self.read_variants(&small, Some((catalog, mode)), true)?;
            trace.push_str(&format!(
                " | {},{},{},{}={text:?}",
                r.x, r.y, r.width, r.height
            ));
            if is_measurement(&text) {
                measurements.push(r);
                continue;
            }
            if let Some(value) = value {
                trace.push_str(&format!(" | {text}"));
                found.push(Candidate {
                    value,
                    x: r.x,
                    y: r.y,
                    width: r.width,
                    height: r.height,
                });
            } else if text
                .split_whitespace()
                .last()
                .and_then(parse_signature)
                .is_some_and(|v| !catalog.resolve(v, mode).is_empty())
            {
                // Scanner marker icons can be recognized as an extra short word.
                // Let contextual OCR locate and reread the complete numeric word;
                // never strip or substitute digits in the recognized string.
                for mut c in self.locate_contrast(&small, catalog, mode)?.0 {
                    c.x += r.x;
                    c.y += r.y;
                    found.push(c);
                }
            }
        }
        Ok(FastReading {
            candidates: found,
            measurements,
            trace,
        })
    }
    pub fn locate_pass(
        &self,
        frame: &Frame,
        catalog: &Catalog,
        mode: Mode,
    ) -> Result<(Vec<Candidate>, String), String> {
        self.locate_variant(frame, catalog, mode, true)
    }
    pub fn locate_contrast(
        &self,
        frame: &Frame,
        catalog: &Catalog,
        mode: Mode,
    ) -> Result<(Vec<Candidate>, String), String> {
        self.locate_variant(frame, catalog, mode, false)
    }
    pub fn locate_quick(
        &self,
        frame: &Frame,
        catalog: &Catalog,
        mode: Mode,
    ) -> Result<(Vec<Candidate>, String), String> {
        let result = self.locate_pass(frame, catalog, mode)?;
        if result.0.is_empty() {
            self.locate_contrast(frame, catalog, mode)
        } else {
            Ok(result)
        }
    }
    fn locate_variant(
        &self,
        frame: &Frame,
        catalog: &Catalog,
        mode: Mode,
        threshold: bool,
    ) -> Result<(Vec<Candidate>, String), String> {
        // Search at bounded resolution; recognize the small candidate again at
        // original resolution. No full-size screenshot is retained or written.
        let scale = (2200.0 / frame.width as f64)
            .min(1500.0 / frame.height as f64)
            .min(3.0);
        let width = (frame.width as f64 * scale) as u32;
        let height = (frame.height as f64 * scale) as u32;
        let mut pixels = vec![255; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let sx = ((x as f64 / scale) as u32).min(frame.width - 1);
                let sy = ((y as f64 / scale) as u32).min(frame.height - 1);
                let i = ((sy * frame.width + sx) * 4) as usize;
                let b = frame.bgra[i].max(frame.bgra[i + 1]).max(frame.bgra[i + 2]);
                // Grayscale preserves dim and antialiased HUD strokes discarded
                // by the fast threshold pass. No OCR digit substitutions.
                let v = if threshold {
                    if b >= 165 {
                        0
                    } else {
                        255
                    }
                } else {
                    255 - b
                };
                let o = ((y * width + x) * 4) as usize;
                pixels[o..o + 3].fill(v);
            }
        }
        let writer = DataWriter::new().map_err(crate::errors::windows)?;
        writer.WriteBytes(&pixels).map_err(crate::errors::windows)?;
        let buffer = writer.DetachBuffer().map_err(crate::errors::windows)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            width as i32,
            height as i32,
        )
        .map_err(crate::errors::windows)?;
        let result = self
            .engine
            .RecognizeAsync(&bitmap)
            .and_then(|op| op.join())
            .map_err(crate::errors::windows)?;
        let mut candidates = Vec::new();
        let mut trace = String::new();
        let angle = result
            .TextAngle()
            .and_then(|a| a.Value())
            .unwrap_or(0.0)
            .to_radians();
        let (sin, cos) = angle.sin_cos();
        for line in result.Lines().map_err(crate::errors::windows)? {
            let words = line.Words().map_err(crate::errors::windows)?;
            for index in 0..words.Size().unwrap_or(0) {
                let word = words.GetAt(index).map_err(crate::errors::windows)?;
                let mut token = word.Text().map_err(crate::errors::windows)?.to_string();
                let mut r = word.BoundingRect().map_err(crate::errors::windows)?;
                let mut consumed = 1;
                if parse_signature(&token).is_none() {
                    let prefix = token.trim_end_matches([',', '.']);
                    if (1..=3).contains(&prefix.len()) && prefix.bytes().all(|c| c.is_ascii_digit())
                    {
                        if let Ok(next) = words.GetAt(index + 1) {
                            let tail = next.Text().unwrap_or_default().to_string();
                            let next_rect = next.BoundingRect().map_err(crate::errors::windows)?;
                            if tail.len() == 3
                                && tail.bytes().all(|c| c.is_ascii_digit())
                                && next_rect.X - r.X - r.Width < r.Height
                                && (next_rect.Y - r.Y).abs() < r.Height / 2.0
                            {
                                token = format!("{prefix},{tail}");
                                consumed = 2;
                                r.Width = next_rect.X + next_rect.Width - r.X;
                                let bottom = (r.Y + r.Height).max(next_rect.Y + next_rect.Height);
                                r.Y = r.Y.min(next_rect.Y);
                                r.Height = bottom - r.Y;
                            }
                        }
                    }
                }
                let Some(_) = parse_signature(&token) else {
                    continue;
                };
                // Keep context to the right: a tile must not cut off a unit
                // (e.g. "km") and turn a distance into an apparent signature.
                if r.X < r.Height * 0.2 || r.X + r.Width + r.Height * 2.5 >= width as f32 {
                    continue;
                }
                // A distance is not a radar signature even if its number matches.
                if let Ok(next) = words.GetAt(index + consumed) {
                    let suffix = next.Text().unwrap_or_default().to_string().to_lowercase();
                    if ["m", "km", "m/s", "°", "scu", "%", "k"].contains(&suffix.as_str()) {
                        continue;
                    }
                }
                // OCR rectangles are in the deskewed image. Map all four corners
                // back around the image center (OcrResult.TextAngle).
                let corners = [
                    (r.X, r.Y),
                    (r.X + r.Width, r.Y),
                    (r.X, r.Y + r.Height),
                    (r.X + r.Width, r.Y + r.Height),
                ];
                let points = corners.map(|(x, y)| {
                    let dx = x as f64 - width as f64 / 2.0;
                    let dy = y as f64 - height as f64 / 2.0;
                    (
                        (width as f64 / 2.0 + dx * cos - dy * sin) / scale,
                        (height as f64 / 2.0 + dx * sin + dy * cos) / scale,
                    )
                });
                let x = (points
                    .iter()
                    .map(|p| p.0)
                    .fold(f64::INFINITY, f64::min)
                    .floor()
                    .max(0.0) as u32)
                    .saturating_sub(12);
                let y = (points
                    .iter()
                    .map(|p| p.1)
                    .fold(f64::INFINITY, f64::min)
                    .floor()
                    .max(0.0) as u32)
                    .saturating_sub(10);
                let right = (points.iter().map(|p| p.0).fold(0.0, f64::max).ceil() as u32 + 12)
                    .min(frame.width);
                let bottom = (points.iter().map(|p| p.1).fold(0.0, f64::max).ceil() as u32 + 10)
                    .min(frame.height);
                if right <= x || bottom <= y {
                    continue;
                }
                let crop = crop(frame, x, y, right - x, bottom - y);
                let (small_text, confirmed) = self.read_crop(&crop, Some((catalog, mode)))?;
                trace.push_str(&format!(
                    " | {token}: crop {x},{y},{},{}, read={small_text:?}",
                    right - x,
                    bottom - y
                ));
                if let Some(value) = confirmed.filter(|v| !catalog.resolve(*v, mode).is_empty()) {
                    candidates.push(Candidate {
                        value,
                        x,
                        y,
                        width: right - x,
                        height: bottom - y,
                    });
                }
            }
        }
        let _ = bitmap.Close();
        Ok((
            candidates,
            result.Text().map_err(crate::errors::windows)?.to_string() + &trace,
        ))
    }
}

pub(crate) fn crop(frame: &Frame, x: u32, y: u32, width: u32, height: u32) -> Frame {
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for row in y..y + height {
        let start = ((row * frame.width + x) * 4) as usize;
        bgra.extend_from_slice(&frame.bgra[start..start + (width * 4) as usize]);
    }
    Frame {
        width,
        height,
        bgra,
    }
}

fn preprocess(frame: &Frame, threshold: bool, scale: u32) -> (Vec<u8>, u32, u32) {
    let pad = 12;
    let width = frame.width * scale + pad * 2;
    let height = frame.height * scale + pad * 2;
    let mut out = vec![255; (width * height * 4) as usize];
    for y in 0..frame.height * scale {
        for x in 0..frame.width * scale {
            let i = (((y / scale) * frame.width + x / scale) * 4) as usize;
            let brightness = frame.bgra[i].max(frame.bgra[i + 1]).max(frame.bgra[i + 2]);
            let v = if threshold {
                if brightness >= 165 {
                    0
                } else {
                    255
                }
            } else {
                255 - brightness
            };
            let target = (((y + pad) * width + x + pad) * 4) as usize;
            out[target..target + 3].fill(v);
        }
    }
    (out, width, height)
}

pub fn file_report(args: &[String]) -> Result<serde_json::Value, String> {
    let mode = if let Some(index) = args.iter().position(|a| a == "--mode") {
        match args.get(index + 1).map(String::as_str) {
            Some("auto") => Mode::Auto,
            Some("mining") => Mode::Ship,
            Some("salvage") => Mode::Salvage,
            Some("roc") => Mode::Roc,
            Some("fps") => Mode::Fps,
            _ => return Err("--mode: auto, mining, salvage, roc or fps".into()),
        }
    } else {
        Mode::Auto
    };
    let path = args
        .first()
        .ok_or("--ocr-file IMAGE --rect x,y,w,h [--output report.json]")?;
    let rect = args
        .iter()
        .position(|s| s == "--rect")
        .and_then(|i| args.get(i + 1));
    let ocr = Ocr::new()?;
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))
        .and_then(|op| op.join())
        .map_err(crate::errors::windows)?;
    let stream = file
        .OpenAsync(FileAccessMode::Read)
        .and_then(|op| op.join())
        .map_err(crate::errors::windows)?;
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .and_then(|op| op.join())
        .map_err(crate::errors::windows)?;
    let dims: Vec<u32> = if let Some(rect) = rect {
        rect.split(',')
            .map(str::parse::<u32>)
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    } else {
        let width = decoder.PixelWidth().map_err(crate::errors::windows)?;
        let height = decoder.PixelHeight().map_err(crate::errors::windows)?;
        let b = crate::capture::search_bounds(windows_sys::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        });
        vec![
            b.left as u32,
            b.top as u32,
            (b.right - b.left) as u32,
            (b.bottom - b.top) as u32,
        ]
    };
    if dims.len() != 4
        || dims[2] < 8
        || dims[3] < 8
        || u64::from(dims[2]) * u64::from(dims[3]) > 24_000_000
    {
        return Err("Invalid rectangle".into());
    }
    if dims[0].saturating_add(dims[2]) > decoder.PixelWidth().map_err(crate::errors::windows)?
        || dims[1].saturating_add(dims[3])
            > decoder.PixelHeight().map_err(crate::errors::windows)?
    {
        return Err("Rectangle is outside the image".into());
    }
    let transform = BitmapTransform::new().map_err(crate::errors::windows)?;
    transform
        .SetBounds(BitmapBounds {
            X: dims[0],
            Y: dims[1],
            Width: dims[2],
            Height: dims[3],
        })
        .map_err(crate::errors::windows)?;
    let pixels = decoder
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Ignore,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )
        .and_then(|op| op.join())
        .and_then(|p| p.DetachPixelData())
        .map_err(crate::errors::windows)?;
    let frame = Frame {
        width: dims[2],
        height: dims[3],
        bgra: pixels.to_vec(),
    };
    let start = Instant::now();
    if rect.is_none() {
        let (candidates, search_text) = if args.iter().any(|a| a == "--fast") {
            let result = ocr.locate_fast(&frame, &Catalog::builtin(), mode)?;
            (result.candidates, result.trace)
        } else {
            ocr.locate(&frame, &Catalog::builtin(), mode)?
        };
        let elapsed = start.elapsed().as_millis();
        let tracking_start = Instant::now();
        let tracking_signature = if candidates.len() == 1 {
            let c = &candidates[0];
            let small = crop(&frame, c.x, c.y, c.width, c.height);
            ocr.read_known(&small, &Catalog::builtin(), mode)?.1
        } else {
            None
        };
        return Ok(
            serde_json::json!({"candidates": candidates, "matches":tracking_signature.map(|v| Catalog::builtin().resolve(v, mode)), "mode":mode, "search_text":search_text, "elapsed_ms":elapsed, "tracking_signature":tracking_signature, "tracking_ms":tracking_start.elapsed().as_millis(), "search_rect":dims}),
        );
    }
    let (text, value) = ocr.read(&frame)?;
    Ok(
        serde_json::json!({"text": text, "signature": value, "matches": value.map(|v| Catalog::builtin().resolve(v, mode)), "mode":mode, "elapsed_ms": start.elapsed().as_millis()}),
    )
}

pub fn load_frame(path: &str) -> Result<Frame, String> {
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))
        .and_then(|op| op.join())
        .map_err(crate::errors::windows)?;
    let stream = file
        .OpenAsync(FileAccessMode::Read)
        .and_then(|op| op.join())
        .map_err(crate::errors::windows)?;
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .and_then(|op| op.join())
        .map_err(crate::errors::windows)?;
    let width = decoder.PixelWidth().map_err(crate::errors::windows)?;
    let height = decoder.PixelHeight().map_err(crate::errors::windows)?;
    if width < 8 || height < 8 || u64::from(width) * u64::from(height) > 24_000_000 {
        return Err("Image is too large or too small".into());
    }
    let transform = BitmapTransform::new().map_err(crate::errors::windows)?;
    let pixels = decoder
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Ignore,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )
        .and_then(|op| op.join())
        .and_then(|p| p.DetachPixelData())
        .map_err(crate::errors::windows)?;
    Ok(Frame {
        width,
        height,
        bgra: pixels.to_vec(),
    })
}
