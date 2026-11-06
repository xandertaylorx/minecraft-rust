struct Globals {
    view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    // x: day brightness, y: fog start, z: fog end, w: gamma
    params: vec4<f32>,
    light_vp: mat4x4<f32>,
    sun: vec4<f32>,      // xyz real sun direction, w time (seconds)
    ldir: vec4<f32>,     // xyz light direction used for shadows, w sun strength
    cam: vec4<f32>,      // xyz camera world pos, w underwater (0/1)
    zenith: vec4<f32>,
}
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
struct Chunk { origin: vec4<f32> }
@group(0) @binding(3) var shadow_map: texture_depth_2d;
@group(0) @binding(4) var shadow_samp: sampler_comparison;
@group(1) @binding(0) var<uniform> chunk: Chunk;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) light: vec2<f32>,
}
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) light: vec2<f32>,
    @location(3) dist: f32,
    @location(4) rel: vec3<f32>,
}

@vertex
fn vs(v: VIn) -> VOut {
    var o: VOut;
    let rel = v.pos + chunk.origin.xyz;
    o.clip = g.view_proj * vec4<f32>(rel, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.light = v.light;
    o.dist = length(rel);
    o.rel = rel;
    return o;
}

// Mirrors vanilla's lightmap curve: v / (4 - 3v), warm block light, cool sky light.
fn lightmap(sky: f32, blk: f32) -> vec3<f32> {
    let s = sky / 15.0;
    let b = blk / 15.0;
    let sf = (s / (4.0 - 3.0 * s)) * g.params.x;
    let bf = b / (4.0 - 3.0 * b);
    let bc = vec3<f32>(bf, bf * ((bf * 0.6 + 0.4) * 0.6 + 0.4), bf * (bf * bf * 0.6 + 0.4));
    let sc = mix(vec3<f32>(sf), vec3<f32>(0.75, 0.75, 1.0) * sf, 0.0) * vec3<f32>(0.92, 0.96, 1.0);
    var c = max(bc, sc);
    c = max(c, vec3<f32>(0.04, 0.04, 0.06));
    let inv = vec3<f32>(1.0) - c;
    let boosted = vec3<f32>(1.0) - inv * inv * inv * inv;
    return clamp(mix(c, boosted, g.params.w), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn sun_color(sy: f32) -> vec3<f32> {
    // Golden-hour curve: the sun is warm for most of its arc and deep orange near the horizon.
    let low = 1.0 - smoothstep(0.0, 0.7, sy);
    let vlow = 1.0 - smoothstep(0.0, 0.25, sy);
    return mix(mix(vec3<f32>(1.0, 0.95, 0.84), vec3<f32>(1.0, 0.72, 0.4), low), vec3<f32>(1.0, 0.45, 0.16), vlow);
}

fn sky_col(dir: vec3<f32>) -> vec3<f32> {
    let h = max(dir.y, 0.0);
    var col = mix(g.fog_color.rgb, g.zenith.rgb, smoothstep(0.0, 0.5, h));
    let s = g.sun.xyz;
    let glow = 1.0 - smoothstep(0.0, 0.6, abs(s.y));
    let toward = max(dot(normalize(vec3<f32>(dir.x, 0.0, dir.z) + vec3<f32>(1e-5, 0.0, 0.0)), normalize(vec3<f32>(s.x, 0.0, s.z) + vec3<f32>(1e-5, 0.0, 0.0))), 0.0);
    col = mix(col, sun_color(s.y) * 0.95, glow * toward * toward * exp(-abs(dir.y) * 4.5) * 0.8);
    return col;
}

fn grade(c0: vec3<f32>) -> vec3<f32> {
    let l = dot(c0, vec3<f32>(0.2126, 0.7152, 0.0722));
    let x = mix(vec3<f32>(l), c0 * 1.1, 1.28);
    let a = x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14);
    return clamp(mix(x, a, 0.5) * vec3<f32>(1.03, 1.0, 0.95), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q += dot(q, q + 45.32);
    return fract(q.x * q.y);
}
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}
fn wave_h(p: vec2<f32>, t: f32) -> f32 {
    return vnoise(p * 0.9 + vec2<f32>(t * 0.35, t * 0.2)) * 0.5 + vnoise(p * 2.1 - vec2<f32>(t * 0.5, -t * 0.3)) * 0.3 + vnoise(p * 4.7 + vec2<f32>(t * 0.9, t * 0.7)) * 0.2;
}

fn ign(p: vec2<f32>) -> f32 { return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715)))); }

// Volumetric light shafts: march the view ray through the shadow map and accumulate sunlit in-scattering.
fn shafts(dir: vec3<f32>, dist: f32, px: vec2<f32>, mist: f32) -> f32 {
    let maxd = min(dist, 52.0);
    let j = ign(px);
    var lit = 0.0;
    for (var k = 0; k < 10; k++) {
        let t = (f32(k) + j) / 10.0 * maxd;
        let lp = g.light_vp * vec4<f32>(dir * t, 1.0);
        let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || lp.z > 1.0) { lit += 1.0; }
        else { lit += textureSampleCompareLevel(shadow_map, shadow_samp, uv, lp.z - 0.002); }
    }
    let sigma = 0.01 + 0.02 * mist;
    return lit / 10.0 * (1.0 - exp(-maxd * sigma));
}

fn hg(c: f32) -> f32 { let gg = 0.62; return (1.0 - gg * gg) / pow(1.0 + gg * gg - 2.0 * gg * c, 1.5); }

fn shadow_at(rel: vec3<f32>, n: vec3<f32>) -> f32 {
    let ndl0 = max(dot(n, g.ldir.xyz), 0.0);
    let p = rel + n * (0.12 + 0.08 * (1.0 - ndl0)) + g.ldir.xyz * 0.04;
    let lp = g.light_vp * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || lp.z > 1.0) { return 1.0; }
    let texel = 1.0 / 2048.0;
    let o = texel * 0.75;
    let s = (textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(-o, -o), lp.z - 0.0012)
           + textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(o, -o), lp.z - 0.0012)
           + textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(-o, o), lp.z - 0.0012)
           + textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(o, o), lp.z - 0.0012)) * 0.25;
    let edge = max(abs(uv.x - 0.5), abs(uv.y - 0.5)) * 2.0;
    return mix(s, 1.0, smoothstep(0.82, 1.0, edge));
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    if (abs(i.color.a - 0.33) < 0.006) {
        // Light-source halo: soft radial glow, flickering, strongest at night and indoors.
        let d = length(i.uv * 2.0 - 1.0);
        let fall = pow(clamp(1.0 - d, 0.0, 1.0), 2.4);
        let fl = 0.85 + 0.15 * vnoise(vec2<f32>(g.sun.w * 9.0, floor(i.rel.x + g.cam.x) * 1.7 + floor(i.rel.z + g.cam.z) * 2.3));
        let a = fall * fl * mix(0.62, 0.2, g.params.x) * smoothstep(0.25, 1.6, i.dist);
        let fogh = clamp((i.dist - g.params.y) / (g.params.z - g.params.y), 0.0, 1.0);
        return vec4<f32>(mix(i.color.rgb * vec3<f32>(1.0, 0.95, 0.9), g.fog_color.rgb, fogh), a * (1.0 - fogh));
    }
    let t = textureSample(atlas, samp, i.uv);
    if (t.a < 0.1) { discard; }
    let is_hand = g.params.y > 9000.0;
    let vdir = normalize(i.rel);
    var n = normalize(cross(dpdx(i.rel), dpdy(i.rel)));
    if (dot(n, vdir) > 0.0) { n = -n; }
    let wpf = i.rel + g.cam.xyz;
    let flick = 0.93 + 0.1 * vnoise(vec2<f32>(g.sun.w * 8.0, floor(wpf.x * 0.2) * 3.1 + floor(wpf.z * 0.2) * 5.7 + floor(wpf.y * 0.2)));
    let lm = lightmap(i.light.x, min(i.light.y * flick, 15.0));
    let time = g.sun.w;
    let wp = i.rel + g.cam.xyz;
    let is_water = abs(i.color.a - 0.72) < 0.006;
    let sun_str = g.ldir.w;
    let scol = sun_color(g.sun.y);
    let sunlight = smoothstep(7.0, 15.0, i.light.x);

    var rgb: vec3<f32>;
    var alpha = t.a * i.color.a;
    if (is_water) {
        var wn = n;
        if (abs(n.y) > 0.9) {
            let e = 0.12;
            let h0 = wave_h(wp.xz, time);
            let hx = wave_h(wp.xz + vec2<f32>(e, 0.0), time);
            let hz = wave_h(wp.xz + vec2<f32>(0.0, e), time);
            wn = normalize(vec3<f32>(-(hx - h0) / e * 0.09, sign(n.y), -(hz - h0) / e * 0.09));
        }
        let base = t.rgb * i.color.rgb * lm;
        let cosv = clamp(dot(-vdir, wn), 0.0, 1.0);
        let fres = clamp(0.03 + 0.97 * pow(1.0 - cosv, 5.0), 0.0, 1.0);
        var surface: vec3<f32>;
        if (wn.y > 0.0) {
            // Seen from above: sky reflection, sun glint, with the water body underneath.
            var r = reflect(vdir, wn);
            r.y = abs(r.y);
            var refl = sky_col(r);
            let sd = max(dot(r, g.sun.xyz), 0.0);
            refl += scol * (pow(sd, 700.0) * 14.0 + pow(sd, 60.0) * 0.5) * sun_str * smoothstep(0.0, 0.1, g.sun.y);
            // Cheap cloud-like sparkle band on the swell crests.
            refl *= 0.92 + 0.16 * vnoise(wp.xz * 0.35 + vec2<f32>(time * 0.05, 0.0));
            let deep = base * vec3<f32>(0.55, 0.75, 0.9);
            surface = mix(deep, refl, fres);
            alpha = mix(0.66, 0.97, fres);
        } else {
            // Seen from below: Snell's window shows the sky, everything else reflects the dark depths.
            let rf = refract(vdir, wn, 1.33);
            let body = vec3<f32>(0.03, 0.16, 0.24) * (0.4 + 0.6 * g.params.x);
            if (dot(rf, rf) > 0.001) {
                let sk = sky_col(normalize(rf)) * 1.0;
                let sd = max(dot(normalize(rf), g.sun.xyz), 0.0);
                surface = mix(body, sk + scol * pow(sd, 200.0) * 6.0, 1.0 - fres);
            } else { surface = body; }
            alpha = 0.94;
        }
        rgb = surface;
    } else {
        var albedo = t.rgb * i.color.rgb;
        var light = lm;
        if (!is_hand) {
            let ndl = max(dot(n, g.ldir.xyz), 0.0);
            var sh = 1.0;
            if (sunlight * sun_str * ndl > 0.01) { sh = shadow_at(i.rel, n); }
            let direct = sunlight * sun_str * ndl * sh;
            let amt = sunlight * sun_str;
            let cool = mix(vec3<f32>(1.0), vec3<f32>(0.80, 0.89, 1.08), amt);
            light = lm * cool * mix(1.0, 0.68, amt) + scol * direct * 0.95 * g.params.x * mix(vec3<f32>(1.0), lm, 0.5);
            if (g.cam.w > 0.5) {
                // Underwater caustics.
                let c1 = vnoise(wp.xz * 1.3 + vec2<f32>(time * 0.6, time * 0.4) + wp.y * 0.3);
                let c2 = vnoise(wp.xz * 1.9 - vec2<f32>(time * 0.5, -time * 0.5) - wp.y * 0.2);
                let caustic = pow(1.0 - abs(c1 - c2), 6.0);
                light += vec3<f32>(0.25, 0.55, 0.6) * caustic * 0.55 * lm.g;
            }
        }
        rgb = albedo * light;
    }
    if (!is_hand) {
        // Ground mist: thicker low down, in forests/swamps and when the sun is low.
        let low_sun = 1.0 - smoothstep(0.0, 0.55, g.sun.y);
        let dens = (0.0025 + 0.011 * g.zenith.w) * (1.0 + 1.2 * low_sun);
        let hf = exp(-max(wp.y - 64.0, 0.0) * 0.05);
        let fm = (1.0 - exp(-i.dist * dens * hf)) * (1.0 - g.cam.w);
        let mcol = mix(g.fog_color.rgb, scol * 0.85 + vec3<f32>(0.1), 0.18 * g.ldir.w);
        rgb = mix(rgb, mcol, fm * 0.65);
    }
    var fogc = g.fog_color.rgb;
    if (!is_hand) { fogc = mix(fogc, scol * 1.05, pow(max(dot(vdir, g.sun.xyz), 0.0), 5.0) * 0.4 * g.params.x * (1.0 - g.cam.w)); }
    let fog = clamp((i.dist - g.params.y) / (g.params.z - g.params.y), 0.0, 1.0);
    rgb = mix(rgb, fogc, fog);
    if (!is_hand) { rgb = grade(rgb); }
    return vec4<f32>(rgb, alpha);
}

// ---- block outline (lines) ----
@vertex
fn vs_line(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return g.view_proj * vec4<f32>(pos + chunk.origin.xyz, 1.0);
}
@fragment
fn fs_line() -> @location(0) vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, 0.4); }

// ---- first-person hand: depth squeezed in front of the world
@vertex
fn vs_hand(v: VIn) -> VOut {
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.clip.z = o.clip.z * 0.01;
    o.uv = v.uv;
    o.color = v.color;
    o.light = v.light;
    o.dist = 0.0;
    o.rel = v.pos;
    return o;
}
