// ScreenCraft compositor: background, shadow, rounded screen with camera, cursor, click effects.
// One full-screen triangle; every pixel is resolved here so preview and export agree exactly.

struct U { v: array<vec4<f32>, 25> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var tex_y: texture_2d<f32>;
@group(0) @binding(2) var tex_u: texture_2d<f32>;
@group(0) @binding(3) var tex_v: texture_2d<f32>;
@group(0) @binding(4) var cursors: texture_2d<f32>;
@group(0) @binding(5) var samp: sampler;

const CELL: f32 = 128.0;
const REF_H: f32 = 80.0;
//HOTSPOTS//

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn sd_round_rect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let rr = min(r, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(rr, rr);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - rr;
}

fn yuv_to_rgb(uv: vec2<f32>) -> vec3<f32> {
    let y = textureSampleLevel(tex_y, samp, uv, 0.0).r * 255.0;
    let cb = textureSampleLevel(tex_u, samp, uv, 0.0).r * 255.0;
    let cr = textureSampleLevel(tex_v, samp, uv, 0.0).r * 255.0;
    let yy = (y - 16.0) / 219.0;
    let pb = (cb - 128.0) / 224.0;
    let pr = (cr - 128.0) / 224.0;
    return clamp(
        vec3<f32>(yy + 1.5748 * pr, yy - 0.1873 * pb - 0.4681 * pr, yy + 1.8556 * pb),
        vec3<f32>(0.0), vec3<f32>(1.0));
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

fn background(p: vec2<f32>) -> vec3<f32> {
    let out = u.v[0].xy;
    let kind = u.v[6].x;
    let uv = p / out;
    var col = u.v[7].rgb;
    if (kind >= 0.5 && kind < 1.5) {
        // linear gradient at an angle (CSS convention: 0 = up, clockwise)
        let a = u.v[6].y;
        let d = vec2<f32>(sin(a), -cos(a));
        let len = abs(out.x * d.x) + abs(out.y * d.y);
        let t = clamp(dot(p - out * 0.5, d) / max(len, 1.0) + 0.5, 0.0, 1.0);
        col = mix(u.v[7].rgb, u.v[8].rgb, t);
    } else if (kind >= 1.5) {
        // wallpaper: diagonal base gradient plus soft blobs
        let t = clamp(dot(p, out) / dot(out, out), 0.0, 1.0);
        col = mix(u.v[7].rgb, u.v[8].rgb, t);
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
    }
    // dither against banding on smooth gradients
    col = col + (hash(p) - 0.5) / 255.0;
    return col;
}

fn over(dst: vec3<f32>, src: vec3<f32>, a: f32) -> vec3<f32> {
    return mix(dst, src, clamp(a, 0.0, 1.0));
}

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = frag.xy;
    let content = u.v[1];
    let radius = u.v[2].x;
    let shadow_blur = max(u.v[2].y, 0.5);
    let shadow_k = u.v[2].z;
    let centre = content.xy + content.zw * 0.5;
    let half = content.zw * 0.5;

    var col = background(p);

    // drop shadow of the screen rectangle
    if (shadow_k > 0.0) {
        let ds = sd_round_rect(p - centre - u.v[3].xy, half, radius);
        let a = (1.0 - smoothstep(-shadow_blur, shadow_blur, ds)) * min(1.0, shadow_k * 1.6);
        col = over(col, vec3<f32>(0.0), a);
    }

    // the screen itself
    let d = sd_round_rect(p - centre, half, radius);
    let mask = clamp(0.5 - d, 0.0, 1.0);
    if (mask > 0.0) {
        let local = (p - content.xy) / content.zw;
        let zoom = max(u.v[4].z, 0.01);
        let view = u.v[5].zw / zoom; // visible part of the source, normalised
        let suv = u.v[4].xy + (local - 0.5) * view;
        var scr = vec3<f32>(0.0);
        if (suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0) {
            // source pixels covered by one output pixel
            let foot = view * u.v[0].zw / content.zw;
            scr = sample_source(suv, foot);
        }

        // cursor halo, click effects, cursor: all live inside the screen
        let to_px = content.zw / view; // normalised source -> output px
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
