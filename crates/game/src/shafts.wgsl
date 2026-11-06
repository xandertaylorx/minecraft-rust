struct Sky {
    inv_vp: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    sun: vec4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    r_sun: vec4<f32>,
    r_moon: vec4<f32>,
    r_clouds: vec4<f32>,
    cloud_color: vec4<f32>,
    cam: vec4<f32>,
    p: vec4<f32>,
    light_vp: mat4x4<f32>,
    misc: vec4<f32>,
}
@group(0) @binding(0) var<uniform> sky: Sky;
@group(0) @binding(3) var shadow_map: texture_depth_2d;
@group(0) @binding(4) var shadow_samp: sampler_comparison;
@group(1) @binding(0) var shaft_tex: texture_2d<f32>;
@group(1) @binding(1) var shaft_samp: sampler;

struct SOut { @builtin(position) clip: vec4<f32>, @location(0) ndc: vec2<f32> }

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> SOut {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: SOut;
    o.clip = vec4<f32>(p[i], 0.0, 1.0);
    o.ndc = p[i];
    return o;
}

fn view_dir(ndc: vec2<f32>) -> vec3<f32> {
    let w = sky.inv_vp * vec4<f32>(ndc, 1.0, 1.0);
    return normalize(w.xyz / w.w);
}

fn ign(p: vec2<f32>) -> f32 { return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715)))); }

// Low-resolution volumetric march through the sun's shadow map.
@fragment
fn fs_gen(i: SOut) -> @location(0) vec4<f32> {
    let dir = view_dir(i.ndc);
    let maxd = 56.0;
    let j = ign(i.clip.xy);
    var lit = 0.0;
    for (var k = 0; k < 14; k++) {
        let t = (f32(k) + j) / 14.0 * maxd;
        let lp = sky.light_vp * vec4<f32>(dir * t, 1.0);
        let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || lp.z > 1.0) { lit += 1.0; }
        else { lit += textureSampleCompareLevel(shadow_map, shadow_samp, uv, lp.z - 0.002); }
    }
    let sigma = 0.012 + 0.025 * sky.misc.y;
    return vec4<f32>(lit / 14.0 * (1.0 - exp(-maxd * sigma)), 0.0, 0.0, 1.0);
}

fn hg(c: f32) -> f32 { let gg = 0.62; return (1.0 - gg * gg) / pow(1.0 + gg * gg - 2.0 * gg * c, 1.5); }

@fragment
fn fs_comp(i: SOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(i.ndc.x * 0.5 + 0.5, 0.5 - i.ndc.y * 0.5);
    let sc = textureSample(shaft_tex, shaft_samp, uv).r;
    let dir = view_dir(i.ndc);
    let s = sky.sun.xyz;
    let low = 1.0 - smoothstep(0.0, 0.7, s.y);
    let scol = mix(vec3<f32>(1.0, 0.93, 0.8), vec3<f32>(1.0, 0.6, 0.28), low);
    let k = (0.012 + 0.03 * hg(dot(dir, s))) * sky.misc.x * (1.0 - sky.sun.w);
    return vec4<f32>(scol * sc * k, 0.0);
}
