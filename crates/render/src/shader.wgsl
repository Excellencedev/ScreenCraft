// ScreenCraft compositor, main pass: background (with motion and blur), shadow, device frame,
// the screen (cross-dissolve, region blur / pixelate, annotation layer, click effects, cursor).
// One full-screen triangle; every pixel is resolved here so preview and export agree exactly.
// `fs_post` is the cut-transition pass over the finished frame.

struct U { v: array<vec4<f32>, 48> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var tex_y: texture_2d<f32>;
@group(0) @binding(2) var tex_u: texture_2d<f32>;
@group(0) @binding(3) var tex_v: texture_2d<f32>;
@group(0) @binding(4) var cursors: texture_2d<f32>;
@group(0) @binding(5) var samp: sampler;
@group(0) @binding(6) var tex2_y: texture_2d<f32>;
@group(0) @binding(7) var tex2_u: texture_2d<f32>;
@group(0) @binding(8) var tex2_v: texture_2d<f32>;
@group(0) @binding(9) var under_layer: texture_2d<f32>;
@group(0) @binding(10) var content_layer: texture_2d<f32>;
@group(0) @binding(11) var bg_image: texture_2d<f32>;
@group(0) @binding(12) var work: texture_2d<f32>;

const CELL: f32 = 128.0;
const REF_H: f32 = 80.0;
const PI: f32 = 3.14159265;
//HOTSPOTS//

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn rnd(i: f32, k: f32) -> f32 {
    return fract(sin(i * 127.1 + k * 311.7) * 43758.5453);
}

fn sd_round_rect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let rr = min(r, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(rr, rr);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - rr;
}

fn ycc_to_rgb(y: f32, cb: f32, cr: f32) -> vec3<f32> {
    let yy = (y - 16.0) / 219.0;
    let pb = (cb - 128.0) / 224.0;
    let pr = (cr - 128.0) / 224.0;
    return clamp(
        vec3<f32>(yy + 1.5748 * pr, yy - 0.1873 * pb - 0.4681 * pr, yy + 1.8556 * pb),
        vec3<f32>(0.0), vec3<f32>(1.0));
}

fn yuv_to_rgb(uv: vec2<f32>) -> vec3<f32> {
    return ycc_to_rgb(
        textureSampleLevel(tex_y, samp, uv, 0.0).r * 255.0,
        textureSampleLevel(tex_u, samp, uv, 0.0).r * 255.0,
        textureSampleLevel(tex_v, samp, uv, 0.0).r * 255.0);
}

fn yuv2_to_rgb(uv: vec2<f32>) -> vec3<f32> {
    return ycc_to_rgb(
        textureSampleLevel(tex2_y, samp, uv, 0.0).r * 255.0,
        textureSampleLevel(tex2_u, samp, uv, 0.0).r * 255.0,
        textureSampleLevel(tex2_v, samp, uv, 0.0).r * 255.0);
}

// Box-filtered fetch: when the source is shown smaller than its pixel size, average a grid of
// taps across one output pixel's footprint instead of aliasing.
fn sample_source(uv: vec2<f32>, footprint_px: vec2<f32>) -> vec3<f32> {
    let n = i32(clamp(ceil(max(footprint_px.x, footprint_px.y)), 1.0, 4.0));
    if (n <= 1) {
        return yuv_to_rgb(uv);
    }
    let src = u.v[0].zw;
    var acc = vec3<f32>(0.0);
    for (var j = 0; j < n; j = j + 1) {
        for (var i = 0; i < n; i = i + 1) {
            let o = (vec2<f32>(f32(i), f32(j)) + 0.5) / f32(n) - 0.5;
            acc = acc + yuv_to_rgb(uv + o * footprint_px / src);
        }
    }
    return acc / f32(n * n);
}

fn sample_source2(uv: vec2<f32>, footprint_px: vec2<f32>) -> vec3<f32> {
    let n = i32(clamp(ceil(max(footprint_px.x, footprint_px.y)), 1.0, 4.0));
    if (n <= 1) {
        return yuv2_to_rgb(uv);
    }
    let src = u.v[0].zw;
    var acc = vec3<f32>(0.0);
    for (var j = 0; j < n; j = j + 1) {
        for (var i = 0; i < n; i = i + 1) {
            let o = (vec2<f32>(f32(i), f32(j)) + 0.5) / f32(n) - 0.5;
            acc = acc + yuv2_to_rgb(uv + o * footprint_px / src);
        }
    }
    return acc / f32(n * n);
}

fn wallpaper_base(p: vec2<f32>, out: vec2<f32>) -> vec3<f32> {
    let t = clamp(dot(p, out) / dot(out, out), 0.0, 1.0);
    var col = mix(u.v[7].rgb, u.v[8].rgb, t);
    let m = max(out.x, out.y);
    let time = u.v[2].w;
    let drift = u.v[6].z;
    for (var i = 0; i < 3; i = i + 1) {
        let b = u.v[9 + i];
        let fi = f32(i);
        let c = vec2<f32>(
            b.x + drift * sin(time * 0.22 + fi * 2.1) * 0.07,
            b.y + drift * cos(time * 0.17 + fi * 1.7) * 0.07) * out;
        let a = clamp(1.0 - length(p - c) / (b.z * m), 0.0, 1.0) * 0.9;
        col = mix(col, u.v[12 + i].rgb, a);
    }
    return col;
}

// the moving extras of a wallpaper: bokeh, waves, dust
fn wallpaper_motion(col_in: vec3<f32>, p: vec2<f32>, out: vec2<f32>) -> vec3<f32> {
    var col = col_in;
    let motion = u.v[26].z;
    let t = u.v[2].w;
    let m = max(out.x, out.y);
    if (motion > 1.5 && motion < 2.5) {
        for (var i = 0; i < 16; i = i + 1) {
            let fi = f32(i);
            let sp = 0.012 + rnd(fi, 1.0) * 0.02;
            let y = 1.15 - (t * sp + rnd(fi, 2.0)) % 1.3;
            let x = rnd(fi, 3.0) + sin(t * 0.3 + fi) * 0.02;
            let r = (0.02 + rnd(fi, 4.0) * 0.05) * m;
            let d = length(p - vec2<f32>(x * out.x, y * out.y)) / r;
            if (d < 1.0) {
                var a = 0.07 * (1.0 - (d - 0.7) / 0.3);
                if (d < 0.7) {
                    a = mix(0.16, 0.07, d / 0.7);
                }
                col = mix(col, vec3<f32>(1.0), a);
            }
        }
    } else if (motion > 2.5 && motion < 3.5) {
        for (var k = 0; k < 3; k = k + 1) {
            let fk = f32(k);
            let uu = p.x / out.x;
            let yw = out.y * (0.78 + fk * 0.07) + sin(uu * (3.0 + fk) * PI + t * (0.35 + fk * 0.12) + fk) * out.y * 0.035;
            if (p.y > yw) {
                col = mix(col, vec3<f32>(1.0), 0.05 - fk * 0.01);
            }
        }
    } else if (motion > 3.5) {
        let dust = u.v[12].rgb;
        for (var i = 0; i < 70; i = i + 1) {
            let fi = f32(i);
            let x = (rnd(fi, 1.0) + t * (0.004 + rnd(fi, 2.0) * 0.006)) % 1.0;
            let y = (rnd(fi, 3.0) + sin(t * 0.2 + fi) * 0.015 + 1.0) % 1.0;
            let a = 0.18 + 0.32 * (0.5 + 0.5 * sin(t * (0.6 + rnd(fi, 4.0)) + fi));
            let r = (0.7 + rnd(fi, 5.0) * 1.6) * (out.y / 540.0);
            let d = length(p - vec2<f32>(x * out.x, y * out.y));
            col = mix(col, dust, a * (1.0 - smoothstep(r - 0.7, r + 0.7, d)));
        }
    }
    return col;
}

fn background_at(p: vec2<f32>) -> vec3<f32> {
    let out = u.v[0].xy;
    let kind = u.v[6].x;
    var col = u.v[7].rgb;
    if (kind >= 0.5 && kind < 1.5) {
        // linear gradient at an angle (CSS convention: 0 = up, clockwise)
        let a = u.v[6].y;
        let d = vec2<f32>(sin(a), -cos(a));
        let len = abs(out.x * d.x) + abs(out.y * d.y);
        let t = clamp(dot(p - out * 0.5, d) / max(len, 1.0) + 0.5, 0.0, 1.0);
        col = mix(u.v[7].rgb, u.v[8].rgb, t);
    } else if (kind >= 2.5) {
        // picture, scaled to cover
        let isz = u.v[28].xy;
        let k = max(out.x / isz.x, out.y / isz.y);
        let uv = ((p - out * 0.5) / k + isz * 0.5) / isz;
        col = textureSampleLevel(bg_image, samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
    } else if (kind >= 1.5) {
        col = wallpaper_base(p, out);
        if (u.v[26].z > 1.5) {
            col = wallpaper_motion(col, p, out);
        }
    }
    return col;
}

fn background(p: vec2<f32>) -> vec3<f32> {
    let out = u.v[0].xy;
    let blur = u.v[26].w;
    var col: vec3<f32>;
    if (blur > 0.5 && u.v[6].x >= 1.5) {
        // the picture / wallpaper through a wide blur
        var acc = vec3<f32>(0.0);
        for (var j = -2; j <= 2; j = j + 1) {
            for (var i = -2; i <= 2; i = i + 1) {
                acc = acc + background_at(p + vec2<f32>(f32(i), f32(j)) * blur * 0.5);
            }
        }
        col = acc / 25.0;
    } else {
        col = background_at(p);
    }
    // dither against banding on smooth gradients
    return col + (hash(p) - 0.5) / 255.0;
}

fn over(dst: vec3<f32>, src: vec3<f32>, a: f32) -> vec3<f32> {
    return mix(dst, src, clamp(a, 0.0, 1.0));
}

fn over_premul(dst: vec3<f32>, s: vec4<f32>) -> vec3<f32> {
    return dst * (1.0 - s.a) + s.rgb;
}

// background, drop shadow and device frame: what sits behind the screen
fn static_layer(p: vec2<f32>) -> vec3<f32> {
    let out = u.v[0].xy;
    let shadow_blur = max(u.v[2].y, 0.5);
    let shadow_k = u.v[2].z;
    let srect = u.v[25]; // the shape the shadow follows (screen, plus its device frame)
    var col = background(p);
    if (shadow_k > 0.0) {
        let ds = sd_round_rect(p - (srect.xy + srect.zw * 0.5) - u.v[3].xy, srect.zw * 0.5, u.v[3].z);
        let a = (1.0 - smoothstep(-shadow_blur, shadow_blur, ds)) * min(1.0, shadow_k * 1.6);
        col = over(col, vec3<f32>(0.0), a);
    }
    if (u.v[27].x > 0.5) {
        col = over_premul(col, textureSampleLevel(under_layer, samp, p / out, 0.0));
    }
    return col;
}

// the recording at output pixel `pos`, through the camera (for the blur / pixelate regions)
fn screen_at(pos: vec2<f32>, foot: vec2<f32>) -> vec3<f32> {
    let content = u.v[1];
    let local = (pos - content.xy) / content.zw;
    let zoom = max(u.v[4].z, 0.01);
    let view = u.v[5].zw / zoom;
    let suv = u.v[4].xy + (local - 0.5) * view;
    if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
        return sample_source(suv, foot);
    }
    return vec3<f32>(0.0);
}

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = frag.xy;
    let out = u.v[0].xy;
    let content = u.v[1];
    let radius = u.v[2].x;
    let centre = content.xy + content.zw * 0.5;
    let half = content.zw * 0.5;

    var col = static_layer(p);

    // the screen itself
    let d = sd_round_rect(p - centre, half, radius);
    let mask = clamp(0.5 - d, 0.0, 1.0) * u.v[26].x;
    if (mask > 0.0) {
        let local = (p - content.xy) / content.zw;
        let zoom = max(u.v[4].z, 0.01);
        let view = u.v[5].zw / zoom; // visible part of the source, normalised
        let suv = u.v[4].xy + (local - 0.5) * view;
        var scr = vec3<f32>(0.0);
        let foot = view * u.v[0].zw / content.zw;
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            // source pixels covered by one output pixel
            scr = sample_source(suv, foot);
            // cross-dissolve: the other clip's picture, with the same camera, on top
            let dis = u.v[26].y;
            if (dis > 0.003) {
                scr = mix(scr, sample_source2(suv, foot), dis);
            }
        }

        // blur / pixelate regions
        let n_eff = i32(u.v[27].w);
        for (var e = 0; e < 8; e = e + 1) {
            if (e >= n_eff) {
                break;
            }
            let r = u.v[29 + e];
            let prm = u.v[37 + e];
            let ed = sd_round_rect(p - (r.xy + r.zw * 0.5), r.zw * 0.5, prm.z);
            let em = clamp(0.5 - ed, 0.0, 1.0);
            if (em > 0.0) {
                var fx = scr;
                if (prm.x < 1.5) {
                    var acc = vec3<f32>(0.0);
                    for (var j = -3; j <= 3; j = j + 1) {
                        for (var i = -3; i <= 3; i = i + 1) {
                            acc = acc + screen_at(p + vec2<f32>(f32(i), f32(j)) * prm.y * 0.34, vec2<f32>(1.0));
                        }
                    }
                    fx = acc / 49.0;
                } else {
                    let q = r.xy + (floor((p - r.xy) / prm.y) + 0.5) * prm.y;
                    fx = screen_at(q, vec2<f32>(max(1.0, prm.y / 2.0)));
                }
                scr = mix(scr, fx, em);
            }
        }

        // annotations pinned to the recording
        if (u.v[27].y > 0.5) {
            scr = over_premul(scr, textureSampleLevel(content_layer, samp, p / out, 0.0));
        }

        // cursor halo, click effects, cursor: all live inside the screen
        let m = u.v[15].xy;
        let cpos = content.xy + ((m - u.v[4].xy) / view + 0.5) * content.zw;
        let c_alpha = u.v[16].x;
        let hl_r = u.v[17].x;
        if (hl_r > 0.0 && c_alpha > 0.0) {
            let h = (1.0 - smoothstep(hl_r * 0.7, hl_r, length(p - cpos))) * u.v[17].y * c_alpha;
            scr = over(scr, u.v[18].rgb, h);
        }
        let ripple_col = u.v[23].rgb;
        let pts_px = u.v[24].x;
        let line_px = u.v[24].y;
        for (var i = 0; i < 4; i = i + 1) {
            let r = u.v[19 + i];
            if (r.z >= 0.0) {
                let rc = content.xy + ((r.xy - u.v[4].xy) / view + 0.5) * content.zw;
                let dist = length(p - rc);
                let k = r.z;
                if (r.w < 1.5) {
                    // ripple: two expanding rings
                    for (var j = 0; j < 2; j = j + 1) {
                        let kk = k - f32(j) * 0.18;
                        if (kk > 0.0) {
                            let rad = (8.0 + 46.0 * kk) * pts_px;
                            let ring = 1.0 - smoothstep(line_px * 0.5, line_px * 0.5 + 1.0, abs(dist - rad));
                            scr = over(scr, ripple_col, ring * (1.0 - kk) * 0.7);
                        }
                    }
                } else {
                    let rad = (26.0 - 14.0 * k) * pts_px;
                    let disc = 1.0 - smoothstep(rad - 1.0, rad, dist);
                    scr = over(scr, ripple_col, disc * (1.0 - k) * 0.5);
                }
            }
        }
        if (c_alpha > 0.002) {
            let size = max(u.v[15].z, 1.0);
            let rot = u.v[15].w;
            let sc = max(u.v[16].y, 0.05);
            let kind = clamp(u.v[16].z, 0.0, 8.0);
            let style = u.v[16].w;
            var rel = p - cpos;
            let cs = cos(-rot);
            let sn = sin(-rot);
            rel = vec2<f32>(rel.x * cs - rel.y * sn, rel.x * sn + rel.y * cs) / sc;
            let hot = HOTSPOTS[i32(kind)];
            let cell = hot + rel * (REF_H / size);
            if (cell.x >= 0.0 && cell.y >= 0.0 && cell.x < CELL && cell.y < CELL) {
                let auv = vec2<f32>((floor(kind) * CELL + cell.x) / (9.0 * CELL), (floor(style) * CELL + cell.y) / (4.0 * CELL));
                let s = textureSampleLevel(cursors, samp, auv, 0.0);
                scr = over(scr, s.rgb, s.a * c_alpha);
            }
        }
        col = mix(col, scr, mask);
    }
    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

// ---- cut transitions over the finished frame -------------------------------------------------

fn ease(t: f32) -> f32 {
    let c = clamp(t, 0.0, 1.0);
    return c * c * (3.0 - 2.0 * c);
}

@fragment
fn fs_post(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = frag.xy;
    let out = u.v[0].xy;
    let kind = u.v[44].x;
    let sp = u.v[44].y; // signed progress
    let k = ease(abs(sp));
    let uv = p / out;
    var col = textureSampleLevel(work, samp, uv, 0.0).rgb;
    if (kind < 1.5) {
        col = mix(col, vec3<f32>(0.0), k);
    } else if (kind < 2.5) {
        col = mix(col, vec3<f32>(1.0), pow(k, 1.4));
    } else if (kind < 3.5) {
        // blur dip: a wide blur mixed in
        let steps = clamp(round(1.0 + k * 4.0), 1.0, 5.0);
        let rad = pow(2.0, steps) * out.y / 1080.0 * 0.7;
        var acc = vec3<f32>(0.0);
        for (var j = -3; j <= 3; j = j + 1) {
            for (var i = -3; i <= 3; i = i + 1) {
                acc = acc + textureSampleLevel(work, samp, (p + vec2<f32>(f32(i), f32(j)) * rad / 3.0) / out, 0.0).rgb;
            }
        }
        col = mix(col, acc / 49.0, min(1.0, k * 1.4));
    } else {
        // slide / zoom: the picture moves over the backdrop
        let back = static_layer(p);
        if (kind < 4.5) {
            let dx = select(1.0, -1.0, sp >= 0.0) * k * out.x;
            let q = p - vec2<f32>(dx, 0.0);
            var pic = back;
            if (q.x >= 0.0 && q.x < out.x) {
                pic = textureSampleLevel(work, samp, q / out, 0.0).rgb;
            }
            col = pic;
        } else {
            let sc = 1.0 + k * 0.35;
            let q = (p - out * 0.5) / sc + out * 0.5;
            var pic = vec3<f32>(0.0);
            var a = 0.0;
            if (q.x >= 0.0 && q.x < out.x && q.y >= 0.0 && q.y < out.y) {
                pic = textureSampleLevel(work, samp, q / out, 0.0).rgb;
                a = 1.0 - k * 0.9;
            }
            col = mix(back, pic, a);
        }
    }
    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
