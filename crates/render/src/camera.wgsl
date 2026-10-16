// The camera picture: a rounded box (bubble, side panel or the whole frame) with its shadow,
// mirror, border and an optional cross-dissolve to the other clip's picture. Output is
// premultiplied and blended over the composed frame.

struct U { v: array<vec4<f32>, 8> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var cy: texture_2d<f32>;
@group(0) @binding(2) var cu: texture_2d<f32>;
@group(0) @binding(3) var cv: texture_2d<f32>;
@group(0) @binding(4) var samp: sampler;
@group(0) @binding(5) var c2y: texture_2d<f32>;
@group(0) @binding(6) var c2u: texture_2d<f32>;
@group(0) @binding(7) var c2v: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}

fn sd_round_rect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let rr = min(r, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(rr, rr);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - rr;
}

fn rgb(y: f32, cb: f32, cr: f32) -> vec3<f32> {
    let yy = (y * 255.0 - 16.0) / 219.0;
    let pb = (cb * 255.0 - 128.0) / 224.0;
    let pr = (cr * 255.0 - 128.0) / 224.0;
    return clamp(vec3<f32>(yy + 1.5748 * pr, yy - 0.1873 * pb - 0.4681 * pr, yy + 1.8556 * pb), vec3<f32>(0.0), vec3<f32>(1.0));
}

// cover-crop the picture of size `isz` into a box of aspect `a`; (bx, by) in 0..1 across the box
fn cover(bx: f32, by: f32, isz: vec2<f32>, a: f32) -> vec2<f32> {
    let va = isz.x / isz.y;
    var fw = 1.0;
    var fh = 1.0;
    if (va > a) {
        fw = a / va;
    } else {
        fh = va / a;
    }
    return vec2<f32>(0.5 + (bx - 0.5) * fw, 0.5 + (by - 0.5) * fh);
}

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = frag.xy;
    let rect = u.v[1];
    let rad = u.v[2].x;
    let alpha = u.v[2].y;
    let shadow = u.v[2].z;
    let border = u.v[2].w;
    let mir = u.v[4].x;
    let s = u.v[4].y;
    let blend2 = u.v[4].z;
    let centre = rect.xy + rect.zw * 0.5;
    let d = sd_round_rect(p - centre, rect.zw * 0.5, rad);

    var out = vec4<f32>(0.0);
    // drop shadow
    if (shadow > 0.01) {
        let ds = sd_round_rect(p - centre - vec2<f32>(0.0, 8.0 * s), rect.zw * 0.5, rad);
        let sa = (1.0 - smoothstep(-28.0 * s, 28.0 * s, ds)) * 0.45 * shadow * (1.0 - clamp(0.5 - d, 0.0, 1.0));
        out = vec4<f32>(0.0, 0.0, 0.0, sa);
    }
    let mask = clamp(0.5 - d, 0.0, 1.0);
    if (mask > 0.0) {
        // black under the picture, then the picture (mirrored through zero width when flipping)
        var col = vec3<f32>(0.0);
        let m = select(-0.02, 0.02, mir >= 0.0);
        let msc = select(mir, m, abs(mir) < 0.02);
        let lx = (p.x - centre.x) / msc;
        if (abs(lx) <= rect.z * 0.5) {
            let bx = lx / rect.z + 0.5;
            let by = (p.y - rect.y) / rect.w;
            let a = rect.z / rect.w;
            let uv = cover(bx, by, u.v[0].zw, a);
            col = rgb(textureSampleLevel(cy, samp, uv, 0.0).r, textureSampleLevel(cu, samp, uv, 0.0).r, textureSampleLevel(cv, samp, uv, 0.0).r);
            if (blend2 > 0.003) {
                let uv2 = cover(bx, by, vec2<f32>(u.v[4].w, u.v[5].x), a);
                let c2 = rgb(textureSampleLevel(c2y, samp, uv2, 0.0).r, textureSampleLevel(c2u, samp, uv2, 0.0).r, textureSampleLevel(c2v, samp, uv2, 0.0).r);
                col = mix(col, c2, blend2);
            }
        }
        out = vec4<f32>(col * mask + out.rgb * (1.0 - mask), mask + out.a * (1.0 - mask));
    }
    // border
    if (border > 0.01) {
        let bw = 1.5 * s;
        let ring = clamp(bw + 0.5 - abs(d), 0.0, 1.0) * border;
        let bc = u.v[3].rgb;
        out = vec4<f32>(bc * ring + out.rgb * (1.0 - ring), ring + out.a * (1.0 - ring));
    }
    return out * alpha;
}
