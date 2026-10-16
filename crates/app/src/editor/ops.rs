//! Edit operations on the project, ported one for one from the 0.3 editor (timemap.js, pace.js and
//! the add / split / duplicate / delete helpers of timeline.js). Pure functions over `Project`.

use screencraft_core::project::*;
use screencraft_core::timeline::slice_output_starts;

pub fn slice_len(s: &Slice) -> f64 {
    (s.source_end_ms - s.source_start_ms).max(0.0) / s.time_scale.max(1e-3)
}

pub fn out_duration(slices: &[Slice]) -> f64 {
    slices.iter().map(slice_len).sum()
}

/// Source time and slice index at output time `t` (clamped inside its slice).
pub fn out_to_src(slices: &[Slice], t: f64) -> (f64, usize) {
    let mut acc = 0.0;
    for (i, s) in slices.iter().enumerate() {
        let len = slice_len(s);
        if t < acc + len || i == slices.len() - 1 {
            let v = s.source_start_ms + (t - acc) * s.time_scale;
            return (v.clamp(s.source_start_ms, s.source_end_ms.max(s.source_start_ms)), i);
        }
        acc += len;
    }
    (0.0, 0)
}

/// Output time of a source time (first slice that contains it).
pub fn src_to_out(slices: &[Slice], t: f64) -> Option<f64> {
    let mut acc = 0.0;
    for s in slices {
        if t >= s.source_start_ms && t <= s.source_end_ms {
            return Some(acc + (t - s.source_start_ms) / s.time_scale.max(1e-3));
        }
        acc += slice_len(s);
    }
    None
}

/// One stretch of the timeline showing a source range: output start/end, source start/end.
pub type Span = [f64; 4];

pub fn spans_out(slices: &[Slice], a: f64, b: f64) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut acc = 0.0;
    let mut prev: Option<f64> = None;
    let mut prev_ramp = false;
    for s in slices {
        let (lo, hi) = (a.max(s.source_start_ms), b.min(s.source_end_ms));
        if hi > lo {
            let o0 = acc + (lo - s.source_start_ms) / s.time_scale.max(1e-3);
            let o1 = acc + (hi - s.source_start_ms) / s.time_scale.max(1e-3);
            let same = prev == Some(s.time_scale) || (s.ramp && prev_ramp);
            match out.last_mut() {
                Some(last) if same && (last[1] - o0).abs() < 0.5 && (last[3] - lo).abs() < 0.5 => {
                    last[1] = o1;
                    last[3] = hi;
                }
                _ => out.push([o0, o1, lo, hi]),
            }
            prev = Some(s.time_scale);
            prev_ramp = s.ramp;
        }
        acc += slice_len(s);
    }
    out
}

/// Where new regions go: from the playhead, up to `len` ms long, inside the current clip.
pub fn region_at(p: &Project, t_out: f64, len: f64) -> (f64, f64) {
    let (src, idx) = out_to_src(&p.scene.slices, t_out);
    let sl = &p.scene.slices[idx.min(p.scene.slices.len() - 1)];
    let start = src.min((sl.source_end_ms - 400.0).max(sl.source_start_ms));
    (start, (start + len).min(sl.source_end_ms))
}

pub const LAYOUT_NAMES: [(&str, &str); 6] = [
    ("bubble", "Bubble"),
    ("hidden", "Camera off"),
    ("large", "Large bubble"),
    ("camera", "Camera only"),
    ("side-right", "Side by side"),
    ("side-left", "Side by side ◂"),
];

pub fn layout_name(k: &str) -> &str {
    LAYOUT_NAMES.iter().find(|(id, _)| *id == k).map(|x| x.1).unwrap_or(k)
}

pub const NOTE_NAMES: [(&str, &str); 8] = [
    ("text", "Text"),
    ("box", "Box"),
    ("ellipse", "Ellipse"),
    ("arrow", "Arrow"),
    ("highlight", "Highlight"),
    ("blur", "Blur"),
    ("pixelate", "Pixelate"),
    ("spotlight", "Spotlight"),
];

pub fn note_name(k: &str) -> &str {
    NOTE_NAMES.iter().find(|(id, _)| *id == k).map(|x| x.1).unwrap_or(k)
}

pub fn new_annotation(kind: &str, id: String, start: f64, end: f64) -> Annotation {
    let mut a = Annotation {
        id,
        kind: kind.into(),
        space: "content".into(),
        source_start_ms: start,
        source_end_ms: end,
        x: 0.4,
        y: 0.4,
        w: 0.3,
        h: 0.1,
        x2: 0.6,
        y2: 0.6,
        text: String::new(),
        color: "#ffffff".into(),
        fill: String::new(),
        font_size: 44.0,
        thickness: 5.0,
        strength: 0.6,
        anim: "fade".into(),
        anim_ms: 250.0,
    };
    let set = |a: &mut Annotation, space: &str, x: f64, y: f64, w: f64, h: f64| {
        a.space = space.into();
        a.x = x;
        a.y = y;
        a.w = w;
        a.h = h;
    };
    match kind {
        "text" => {
            set(&mut a, "screen", 0.1, 0.8, 0.6, 0.1);
            a.text = "Your text".into();
            a.font_size = 48.0;
            a.fill = "#000000b3".into();
        }
        "box" => {
            set(&mut a, "content", 0.35, 0.35, 0.3, 0.2);
            a.color = "#ffd60a".into();
        }
        "ellipse" => {
            set(&mut a, "content", 0.4, 0.35, 0.2, 0.2);
            a.color = "#ffd60a".into();
        }
        "highlight" => {
            set(&mut a, "content", 0.3, 0.4, 0.4, 0.06);
            a.color = "#ffe14a".into();
        }
        "arrow" => {
            set(&mut a, "content", 0.3, 0.3, 0.3, 0.1);
            a.x2 = 0.5;
            a.y2 = 0.5;
            a.color = "#ff453a".into();
            a.thickness = 6.0;
        }
        "blur" => {
            set(&mut a, "content", 0.3, 0.4, 0.3, 0.1);
            a.strength = 0.7;
            a.anim = "none".into();
        }
        "pixelate" => {
            set(&mut a, "content", 0.3, 0.4, 0.3, 0.1);
            a.anim = "none".into();
        }
        "spotlight" => {
            set(&mut a, "content", 0.3, 0.3, 0.4, 0.3);
            a.strength = 0.65;
        }
        _ => {}
    }
    a
}

/// Fast-forward the given source ranges. Returns the number of stretches changed.
pub fn apply_idle(slices: &mut Vec<Slice>, segs: &[[f64; 2]], speed: f64, new_id: &mut dyn FnMut() -> String) -> usize {
    let mut count = 0;
    let mut cur = std::mem::take(slices);
    for seg in segs {
        let (a, b) = (seg[0], seg[1]);
        let mut next = Vec::new();
        for s in cur.into_iter() {
            if s.freeze || s.source_end_ms <= a || s.source_start_ms >= b {
                next.push(s);
                continue;
            }
            let (lo, hi) = (a.max(s.source_start_ms), b.min(s.source_end_ms));
            if hi - lo < 250.0 {
                next.push(s);
                continue;
            }
            count += 1;
            if lo > s.source_start_ms + 1.0 {
                let mut left = s.clone();
                left.id = new_id();
                left.source_end_ms = lo;
                next.push(left);
            }
            let mut mid = s.clone();
            mid.id = new_id();
            mid.source_start_ms = lo;
            mid.source_end_ms = hi;
            mid.time_scale = ((s.time_scale * speed) * 1000.0).round() / 1000.0;
            if lo > s.source_start_ms + 1.0 {
                mid.transition = None;
            }
            next.push(mid);
            if hi < s.source_end_ms - 1.0 {
                let mut right = s.clone();
                right.id = new_id();
                right.source_start_ms = hi;
                right.transition = None;
                next.push(right);
            }
        }
        cur = next;
    }
    *slices = cur;
    count
}

/// Replace each abrupt change of speed between neighbouring clips by a staircase of speeds.
pub fn ease_speed_changes(slices: &mut Vec<Slice>, ramp_ms: f64, only_index: Option<usize>, new_id: &mut dyn FnMut() -> String) -> usize {
    let mut made = 0;
    if slices.len() < 2 {
        return 0;
    }
    let mut i = slices.len() as isize - 2;
    while i >= 0 {
        let iu = i as usize;
        i -= 1;
        if let Some(o) = only_index {
            if iu + 1 != o {
                continue;
            }
        }
        let (a_s, b_s) = (slices[iu].clone(), slices[iu + 1].clone());
        if a_s.freeze || b_s.freeze {
            continue;
        }
        let (a, b) = (a_s.time_scale, b_s.time_scale);
        if (b / a).ln().abs() < 0.25 {
            continue;
        }
        const N: usize = 8;
        let ks: Vec<f64> = (0..N).map(|j| a * (b / a).powf((j as f64 + 0.5) / N as f64)).collect();
        let cost = |r: f64| {
            let per = r / N as f64;
            let (mut sa, mut sb) = (0.0, 0.0);
            for (j, k) in ks.iter().enumerate() {
                if j < N / 2 {
                    sa += k * per;
                } else {
                    sb += k * per;
                }
            }
            (sa, sb)
        };
        let mut r = ramp_ms;
        let (sa, sb) = cost(r);
        let (len_a, len_b) = (a_s.source_end_ms - a_s.source_start_ms, b_s.source_end_ms - b_s.source_start_ms);
        let f = 1f64.min(len_a * 0.45 / sa).min(len_b * 0.45 / sb);
        if f < 0.25 {
            continue;
        }
        r *= f;
        let (sa, sb) = cost(r);
        let per = r / N as f64;
        let (mut cur_a, mut cur_b) = (a_s.source_end_ms - sa, b_s.source_start_ms);
        let mut ramp = Vec::new();
        for (j, k) in ks.iter().enumerate() {
            let len = k * per;
            let from_a = j < N / 2;
            let start = if from_a { cur_a } else { cur_b };
            let mut piece = if from_a { a_s.clone() } else { b_s.clone() };
            piece.id = new_id();
            piece.source_start_ms = start;
            piece.source_end_ms = start + len;
            piece.time_scale = (k * 1000.0).round() / 1000.0;
            piece.transition = None;
            piece.freeze = false;
            piece.ramp = true;
            if from_a {
                cur_a += len;
            } else {
                cur_b += len;
            }
            ramp.push(piece);
        }
        slices[iu].source_end_ms -= sa;
        slices[iu + 1].source_start_ms += sb;
        for (n, piece) in ramp.into_iter().enumerate() {
            slices.insert(iu + 1 + n, piece);
        }
        made += 1;
    }
    made
}

/// Hold the frame under `t_out` for `hold_ms`. Returns the id of the new frozen slice.
pub fn freeze_at(slices: &mut Vec<Slice>, t_out: f64, hold_ms: f64, new_id: &mut dyn FnMut() -> String) -> Option<String> {
    let (src, idx) = out_to_src(slices, t_out);
    let sl = slices.get(idx)?.clone();
    if sl.freeze {
        return None;
    }
    let at = src.max(sl.source_start_ms + 5.0).min(sl.source_end_ms - 5.0);
    let mut frozen = sl.clone();
    frozen.id = new_id();
    frozen.freeze = true;
    frozen.transition = None;
    frozen.volume = 0.0;
    frozen.source_start_ms = at;
    frozen.source_end_ms = at + hold_ms / 1000.0;
    frozen.time_scale = 0.001;
    let mut right = sl.clone();
    right.id = new_id();
    right.source_start_ms = at;
    right.transition = None;
    slices[idx].source_end_ms = at;
    let fid = frozen.id.clone();
    slices.insert(idx + 1, frozen);
    slices.insert(idx + 2, right);
    Some(fid)
}

pub fn output_starts(slices: &[Slice]) -> Vec<f64> {
    slice_output_starts(slices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(a: f64, b: f64, scale: f64) -> Slice {
        Slice { id: format!("s{a}"), source_start_ms: a, source_end_ms: b, time_scale: scale, ..Default::default() }
    }

    #[test]
    fn spans_merge_back_to_back_pieces() {
        let sl = vec![s(0.0, 1000.0, 1.0), s(1000.0, 2000.0, 1.0)];
        let sp = spans_out(&sl, 500.0, 1500.0);
        assert_eq!(sp.len(), 1);
        assert_eq!(sp[0], [500.0, 1500.0, 500.0, 1500.0]);
    }

    #[test]
    fn idle_speeds_up_the_middle() {
        let mut sl = vec![s(0.0, 10_000.0, 1.0)];
        let mut n = 0;
        let mut id = || {
            n += 1;
            format!("n{n}")
        };
        let c = apply_idle(&mut sl, &[[2000.0, 6000.0]], 4.0, &mut id);
        assert_eq!(c, 1);
        assert_eq!(sl.len(), 3);
        assert_eq!(sl[1].time_scale, 4.0);
        assert!((out_duration(&sl) - (2000.0 + 1000.0 + 4000.0)).abs() < 1e-6);
    }

    #[test]
    fn ramps_split_between_speeds() {
        let mut sl = vec![s(0.0, 5000.0, 1.0), s(5000.0, 15_000.0, 4.0)];
        let mut n = 0;
        let mut id = || {
            n += 1;
            format!("r{n}")
        };
        assert_eq!(ease_speed_changes(&mut sl, 400.0, None, &mut id), 1);
        assert_eq!(sl.len(), 10);
        assert!(sl[1..9].iter().all(|x| x.ramp));
    }
}
