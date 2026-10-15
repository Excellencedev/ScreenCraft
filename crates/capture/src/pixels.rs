//! Small pixel helpers shared by the capture backends.

/// Crop `(cx, cy, cw, ch)` out of a BGRA image and scale to `(ow, oh)` (bilinear when scaling).
pub fn crop_scale(src: &[u8], stride: usize, crop: (u32, u32, u32, u32), out: (u32, u32)) -> Vec<u8> {
    let (cx, cy, cw, ch) = crop;
    let (ow, oh) = out;
    let mut dst = vec![0u8; ow as usize * oh as usize * 4];
    if (cw, ch) == (ow, oh) {
        for y in 0..oh as usize {
            let s = (cy as usize + y) * stride + cx as usize * 4;
            dst[y * ow as usize * 4..(y + 1) * ow as usize * 4].copy_from_slice(&src[s..s + ow as usize * 4]);
        }
        return dst;
    }
    let (sx, sy) = (cw as f32 / ow as f32, ch as f32 / oh as f32);
    for y in 0..oh as usize {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, ch as f32 - 1.0);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(ch as usize - 1);
        let ty = fy - y0 as f32;
        for x in 0..ow as usize {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, cw as f32 - 1.0);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(cw as usize - 1);
            let tx = fx - x0 as f32;
            let at = |xx: usize, yy: usize, c: usize| src[(cy as usize + yy) * stride + (cx as usize + xx) * 4 + c] as f32;
            for c in 0..4 {
                let top = at(x0, y0, c) * (1.0 - tx) + at(x1, y0, c) * tx;
                let bot = at(x0, y1, c) * (1.0 - tx) + at(x1, y1, c) * tx;
                dst[(y * ow as usize + x) * 4 + c] = (top * (1.0 - ty) + bot * ty + 0.5) as u8;
            }
        }
    }
    dst
}

