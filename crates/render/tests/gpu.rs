use screencraft_encode::color::{to_i420, PixelOrder};
use screencraft_render::{plain_scene, CursorDraw, Compositor, Ripple};

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> screencraft_encode::I420 {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for p in px.chunks_mut(4) {
        p.copy_from_slice(&rgba);
    }
    to_i420(&px, w, h, (w * 4) as usize, PixelOrder::Rgba, 2)
}

fn compositor() -> Option<Compositor> {
    match Compositor::headless() {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

fn px(buf: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let o = ((y * w + x) * 4) as usize;
    [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]
}

fn close(a: [u8; 4], b: [u8; 3], tol: i32) -> bool {
    (0..3).all(|i| (a[i] as i32 - b[i] as i32).abs() <= tol)
}

#[test]
fn screen_is_framed_by_background_and_shows_the_source() {
    let Some(mut c) = compositor() else { return };
    c.upload(&solid(64, 36, [220, 30, 40, 255]));
    let mut s = plain_scene((320, 240), (64, 36));
    s.content = [20.0, 20.0, 280.0, 200.0];
    let _ = c.render(&s);
    let img = c.read_rgba().unwrap();
    assert_eq!(img.len(), 320 * 240 * 4);
    // background: (0.1 * 255 = 25)
    assert!(close(px(&img, 320, 4, 4), [25, 25, 25], 3), "{:?}", px(&img, 320, 4, 4));
    // screen content: the red source
    assert!(close(px(&img, 320, 160, 120), [220, 30, 40], 8), "{:?}", px(&img, 320, 160, 120));
    // exactly at the padding edge the screen starts
    assert!(close(px(&img, 320, 19, 100), [25, 25, 25], 3));
    assert!(close(px(&img, 320, 22, 100), [220, 30, 40], 8));
}

#[test]
fn corners_are_rounded() {
    let Some(mut c) = compositor() else { return };
    c.upload(&solid(16, 16, [255, 255, 255, 255]));
    let mut s = plain_scene((200, 200), (16, 16));
    s.content = [20.0, 20.0, 160.0, 160.0];
    s.radius_px = 40.0;
    let _ = c.render(&s);
    let img = c.read_rgba().unwrap();
    // the corner pixel of the rectangle is background, the middle of an edge is screen
    assert!(close(px(&img, 200, 21, 21), [25, 25, 25], 6));
    assert!(close(px(&img, 200, 100, 22), [255, 255, 255], 8));
}

#[test]
fn zoom_crops_into_the_source() {
    let Some(mut c) = compositor() else { return };
    // left half black, right half white
    let (w, h) = (64u32, 64u32);
    let mut px_data = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let v = if x >= w / 2 { 255 } else { 0 };
            let o = ((y * w + x) * 4) as usize;
            px_data[o..o + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    c.upload(&to_i420(&px_data, w, h, (w * 4) as usize, PixelOrder::Rgba, 1));
    let mut s = plain_scene((128, 128), (64, 64));
    s.content = [0.0, 0.0, 128.0, 128.0];
    // 4x zoom centred on the right half: everything is white
    s.camera = [0.75, 0.5, 4.0];
    let _ = c.render(&s);
    let img = c.read_rgba().unwrap();
    assert!(close(px(&img, 128, 5, 5), [255, 255, 255], 10));
    assert!(close(px(&img, 128, 120, 120), [255, 255, 255], 10));
    // 1x: left half black
    s.camera = [0.5, 0.5, 1.0];
    let _ = c.render(&s);
    let img = c.read_rgba().unwrap();
    assert!(close(px(&img, 128, 10, 64), [16, 16, 16], 10) || close(px(&img, 128, 10, 64), [0, 0, 0], 10));
    assert!(close(px(&img, 128, 118, 64), [255, 255, 255], 10));
}

#[test]
fn cursor_and_click_effect_draw_on_top() {
    let Some(mut c) = compositor() else { return };
    c.upload(&solid(64, 64, [60, 120, 200, 255]));
    let mut s = plain_scene((256, 256), (64, 64));
    s.content = [0.0, 0.0, 256.0, 256.0];
    let base = {
        let _ = c.render(&s);
        c.read_rgba().unwrap()
    };
    s.cursor = Some(CursorDraw { pos: [0.5, 0.5], size_px: 80.0, rotation: 0.0, alpha: 1.0, scale: 1.0, kind: 0, style: 0 });
    let with_cursor = {
        let _ = c.render(&s);
        c.read_rgba().unwrap()
    };
    // A dark arrow body sits just below-right of the hotspot.
    let changed = (0..256 * 256).filter(|i| base[i * 4..i * 4 + 3] != with_cursor[i * 4..i * 4 + 3]).count();
    assert!(changed > 400, "cursor changed only {changed} pixels");
    // far away from the cursor nothing changed
    assert_eq!(px(&base, 256, 10, 10), px(&with_cursor, 256, 10, 10));

    s.cursor = None;
    s.ripples.push(Ripple { pos: [0.5, 0.5], progress: 0.3, ring: true });
    s.points_to_px = 2.0;
    s.ripple_line_px = 3.0;
    let _ = c.render(&s);
    let rip = c.read_rgba().unwrap();
    let changed = (0..256 * 256).filter(|i| base[i * 4..i * 4 + 3] != rip[i * 4..i * 4 + 3]).count();
    assert!(changed > 200, "ripple changed only {changed} pixels");
}

#[test]
fn same_input_renders_identical_bytes() {
    let Some(mut c) = compositor() else { return };
    c.upload(&solid(48, 28, [10, 200, 90, 255]));
    let s = plain_scene((160, 90), (48, 28));
    let _ = c.render(&s);
    let a = c.read_rgba().unwrap();
    let _ = c.render(&s);
    let b = c.read_rgba().unwrap();
    assert_eq!(a, b);
}
