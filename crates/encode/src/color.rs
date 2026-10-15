//! Pixel format conversion (BT.709, limited range). Rows are split across scoped threads in
//! fixed bands, so the result never depends on how many threads ran.

use crate::types::I420;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelOrder {
    Bgra,
    Rgba,
}

fn bands(height: usize, threads: usize) -> Vec<(usize, usize)> {
    // Bands are multiples of two rows so chroma pairs never straddle a split.
    let threads = threads.clamp(1, 32).min((height / 2).max(1));
    let pairs = height / 2;
    let per = pairs.div_ceil(threads);
    let mut out = Vec::new();
    let mut p = 0;
    while p < pairs {
        let end = (p + per).min(pairs);
        out.push((p * 2, end * 2));
        p = end;
    }
    out
}

pub fn default_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8)
}

#[inline(always)]
fn luma(r: i32, g: i32, b: i32) -> u8 {
    // 0.1826 / 0.6142 / 0.0620 in 16.16, plus 16 and rounding.
    (((11966 * r + 40253 * g + 4063 * b + 32768) >> 16) + 16).clamp(0, 255) as u8
}

#[inline(always)]
fn chroma_u(r: i32, g: i32, b: i32) -> u8 {
    (((-6596 * r - 22189 * g + 28785 * b + 32768) >> 16) + 128).clamp(0, 255) as u8
}

#[inline(always)]
fn chroma_v(r: i32, g: i32, b: i32) -> u8 {
    (((28785 * r - 26145 * g - 2640 * b + 32768) >> 16) + 128).clamp(0, 255) as u8
}

/// Convert a packed 4-byte-per-pixel image into I420. `stride` is in bytes.
pub fn to_i420(src: &[u8], width: u32, height: u32, stride: usize, order: PixelOrder, threads: usize) -> I420 {
    let (w, h) = (width as usize, height as usize);
    assert!(w % 2 == 0 && h % 2 == 0, "I420 needs even dimensions");
    assert!(src.len() >= stride * (h - 1) + w * 4, "source too small");
    let mut out = I420::new(width, height);
    let (ri, bi) = match order {
        PixelOrder::Bgra => (2usize, 0usize),
        PixelOrder::Rgba => (0, 2),
    };

    let ranges = bands(h, threads);
    let mut y_rest: &mut [u8] = &mut out.y;
    let mut u_rest: &mut [u8] = &mut out.u;
    let mut v_rest: &mut [u8] = &mut out.v;
    std::thread::scope(|s| {
        for &(y0, y1) in &ranges {
            let rows = y1 - y0;
            let (yb, yr) = y_rest.split_at_mut(rows * w);
            let (ub, ur) = u_rest.split_at_mut(rows / 2 * (w / 2));
            let (vb, vr) = v_rest.split_at_mut(rows / 2 * (w / 2));
            y_rest = yr;
            u_rest = ur;
            v_rest = vr;
            s.spawn(move || {
                for ry in 0..rows {
                    let row = &src[(y0 + ry) * stride..(y0 + ry) * stride + w * 4];
                    let dst = &mut yb[ry * w..(ry + 1) * w];
                    for (x, d) in dst.iter_mut().enumerate() {
                        let p = &row[x * 4..x * 4 + 4];
                        *d = luma(p[ri] as i32, p[1] as i32, p[bi] as i32);
                    }
                }
                for cy in 0..rows / 2 {
                    let r0 = &src[(y0 + cy * 2) * stride..];
                    let r1 = &src[(y0 + cy * 2 + 1) * stride..];
                    for cx in 0..w / 2 {
                        let (mut r, mut g, mut b) = (0i32, 0i32, 0i32);
                        for (row, dx) in [(r0, 0), (r0, 1), (r1, 0), (r1, 1)] {
                            let i = (cx * 2 + dx) * 4;
                            r += row[i + ri] as i32;
                            g += row[i + 1] as i32;
                            b += row[i + bi] as i32;
                        }
                        let (r, g, b) = ((r + 2) >> 2, (g + 2) >> 2, (b + 2) >> 2);
                        ub[cy * (w / 2) + cx] = chroma_u(r, g, b);
                        vb[cy * (w / 2) + cx] = chroma_v(r, g, b);
                    }
                }
            });
        }
    });
    out
}

/// I420 -> packed RGBA (alpha 255). `dst` must hold `width * height * 4` bytes.
pub fn i420_to_rgba(frame: &I420, dst: &mut [u8], threads: usize) {
    let (w, h) = (frame.width as usize, frame.height as usize);
    assert!(dst.len() >= w * h * 4);
    let ranges = bands(h, threads);
    let mut rest: &mut [u8] = &mut dst[..w * h * 4];
    std::thread::scope(|s| {
        for &(y0, y1) in &ranges {
            let (chunk, r) = rest.split_at_mut((y1 - y0) * w * 4);
            rest = r;
            s.spawn(move || {
                for ry in 0..(y1 - y0) {
                    let y = y0 + ry;
                    for x in 0..w {
                        let yy = frame.y[y * w + x] as i32 - 16;
                        let u = frame.u[(y / 2) * (w / 2) + x / 2] as i32 - 128;
                        let v = frame.v[(y / 2) * (w / 2) + x / 2] as i32 - 128;
                        let c = 76309 * yy; // 1.16438 in 16.16
                        let r = (c + 117489 * v + 32768) >> 16; // 1.79274
                        let g = (c - 13975 * u - 34925 * v + 32768) >> 16; // 0.21325 / 0.53291
                        let b = (c + 138438 * u + 32768) >> 16; // 2.11240
                        let o = (ry * w + x) * 4;
                        chunk[o] = r.clamp(0, 255) as u8;
                        chunk[o + 1] = g.clamp(0, 255) as u8;
                        chunk[o + 2] = b.clamp(0, 255) as u8;
                        chunk[o + 3] = 255;
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_count_does_not_change_output() {
        let (w, h) = (64u32, 48u32);
        let mut src = vec![0u8; (w * h * 4) as usize];
        for (i, b) in src.iter_mut().enumerate() {
            *b = (i * 7 + i / 13) as u8;
        }
        let a = to_i420(&src, w, h, (w * 4) as usize, PixelOrder::Bgra, 1);
        let b = to_i420(&src, w, h, (w * 4) as usize, PixelOrder::Bgra, 5);
        assert_eq!(a, b);
    }

    #[test]
    fn round_trip_is_close() {
        let (w, h) = (32u32, 32u32);
        let mut src = vec![0u8; (w * h * 4) as usize];
        for px in src.chunks_mut(4) {
            px.copy_from_slice(&[200, 120, 40, 255]);
        }
        let yuv = to_i420(&src, w, h, (w * 4) as usize, PixelOrder::Rgba, 2);
        let mut back = vec![0u8; src.len()];
        i420_to_rgba(&yuv, &mut back, 2);
        for (a, b) in src.chunks(4).zip(back.chunks(4)) {
            for c in 0..3 {
                assert!((a[c] as i32 - b[c] as i32).abs() <= 3, "{a:?} vs {b:?}");
            }
        }
    }
}
