use crate::capture::Frame;

#[derive(Clone, Copy, Debug)]
pub struct TextBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

// Find complete HUD text runs before invoking OCR. This is a pixel/geometry
// filter, never a digit classifier: only OCR and the catalog can accept a value.
pub fn text_regions(frame: &Frame) -> Vec<TextBox> {
    let step = if frame.width > 2600 { 2 } else { 1 };
    let w = (frame.width / step) as usize;
    let h = (frame.height / step) as usize;
    let mut mask = vec![0u8; w * h];
    let mut lit = 0;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * step as usize * frame.width as usize) + x * step as usize) * 4;
            let b = frame.bgra[i] as u16;
            let g = frame.bgra[i + 1] as u16;
            let r = frame.bgra[i + 2] as u16;
            // Gold and near-white scanner text. Reject green scenery/cyan MFDs.
            if r >= 90 && r * 10 >= g * 9 && g * 10 >= b * 8 {
                mask[y * w + x] = 1;
                lit += 1;
            }
        }
    }
    if lit == 0 || lit > w * h / 3 {
        return Vec::new();
    }
    let mut glyphs = Vec::new();
    let mut queue = Vec::new();
    for seed in 0..mask.len() {
        if mask[seed] == 0 {
            continue;
        }
        mask[seed] = 0;
        queue.clear();
        queue.push(seed);
        let (mut l, mut r, mut t, mut b) = (seed % w, seed % w, seed / w, seed / w);
        let mut cursor = 0;
        while cursor < queue.len() {
            let p = queue[cursor];
            cursor += 1;
            let (x, y) = (p % w, p / w);
            l = l.min(x);
            r = r.max(x);
            t = t.min(y);
            b = b.max(y);
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let n = ny * w + nx;
                    if mask[n] != 0 {
                        mask[n] = 0;
                        queue.push(n);
                    }
                }
            }
        }
        let (gw, gh) = (r - l + 1, b - t + 1);
        if queue.len() >= 3 && (4..=70).contains(&gh) && gw <= gh * 2 && gh <= gw * 12 {
            glyphs.push((l, t, r + 1, b + 1));
        }
        if glyphs.len() > 4000 {
            return Vec::new();
        }
    }
    glyphs.sort_unstable_by_key(|g| g.0);
    let mut runs: Vec<(usize, usize, usize, usize, usize)> = Vec::new();
    for (l, t, r, b) in glyphs {
        let gh = b - t;
        if let Some(run) = runs.iter_mut().rev().find(|run| {
            let rh = run.3 - run.1;
            let overlap = run.3.min(b).saturating_sub(run.1.max(t));
            let compatible_height = if rh.max(gh) <= 20 {
                rh.max(gh) <= rh.min(gh) * 2
            } else {
                rh.max(gh) * 10 <= rh.min(gh) * 14
            };
            l <= run.2 + rh.max(gh) && overlap * 2 >= rh.min(gh) && compatible_height
        }) {
            run.0 = run.0.min(l);
            run.1 = run.1.min(t);
            run.2 = run.2.max(r);
            run.3 = run.3.max(b);
            run.4 += 1;
        } else {
            runs.push((l, t, r, b, 1));
        }
    }
    let mut regions = Vec::new();
    for (l, t, r, b, count) in runs {
        let (rw, rh) = (r - l, b - t);
        if !(3..=12).contains(&count) || rw < rh * 3 / 2 || rw > rh * 10 {
            continue;
        }
        // Include nearby suffixes and punctuation. Never accept a run clipped by
        // the capture edge; otherwise a distance or longer number can look valid.
        let pad = (rh / 3).max(4);
        let right_pad = rh * 3;
        if l < pad || t < pad || r + right_pad >= w || b + pad >= h {
            continue;
        }
        regions.push(TextBox {
            x: (l - pad) as u32 * step,
            y: (t - pad) as u32 * step,
            width: (rw + pad + right_pad) as u32 * step,
            height: (rh + pad * 2) as u32 * step,
        });
    }
    // Near the scanner first; the complete wide envelope is still searched.
    regions.sort_unstable_by_key(|r| {
        (r.x + r.width / 2).abs_diff(frame.width / 2)
            + (r.y + r.height / 2).abs_diff(frame.height / 3)
    });
    regions.truncate(12);
    regions
}
