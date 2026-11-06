struct Sky {
    inv_vp: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    sun: vec4<f32>,       // xyz sun direction, w night factor (0 day .. 1 night)
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    r_sun: vec4<f32>,     // atlas uv rect (x, y, w, h)
    r_moon: vec4<f32>,
    r_clouds: vec4<f32>,
    cloud_color: vec4<f32>,
    cam: vec4<f32>,       // camera position xyz, cloud scroll (blocks) in w
    p: vec4<f32>,         // x cloud height, y fog start, z fog end, w time angle (radians)
}
@group(0) @binding(0) var<uniform> sky: Sky;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct SOut { @builtin(position) clip: vec4<f32>, @location(0) ndc: vec2<f32> }

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SOut {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: SOut;
    o.clip = vec4<f32>(p[i], 1.0, 1.0);
    o.ndc = p[i];
    return o;
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn body(dir: vec3<f32>, s: vec3<f32>, half: f32, rect: vec4<f32>) -> vec4<f32> {
    let d = dot(dir, s);
    if (d < 0.05) { return vec4<f32>(0.0); }
    let t = vec3<f32>(0.0, 0.0, 1.0);
    let b = cross(s, t);
    let u = dot(dir, t) / d / half * 0.5 + 0.5;
    let v = 0.5 - dot(dir, b) / d / half * 0.5;
    if (u < 0.0 || u > 1.0 || v < 0.0 || v > 1.0) { return vec4<f32>(0.0); }
    let uv = vec2<f32>(rect.x + u * rect.z, rect.y + v * rect.w);
    return textureSampleLevel(tex, samp, uv, 0.0);
}

@fragment
fn fs_sky(i: SOut) -> @location(0) vec4<f32> {
    let w = sky.inv_vp * vec4<f32>(i.ndc, 1.0, 1.0);
    let dir = normalize(w.xyz / w.w);
    let night = sky.sun.w;
    let s = sky.sun.xyz;
    let h = max(dir.y, 0.0);
    var col = mix(sky.horizon.rgb, sky.zenith.rgb, smoothstep(0.0, 0.5, h));
    if (dir.y < 0.0) { col = sky.horizon.rgb; }
    // Sunrise / sunset glow around the horizon.
    let glow = (1.0 - smoothstep(0.0, 0.45, abs(s.y))) * (1.0 - night * 0.0);
    let toward = max(dot(normalize(vec3<f32>(dir.x, 0.0, dir.z) + vec3<f32>(1e-5, 0.0, 0.0)), normalize(vec3<f32>(s.x, 0.0, s.z) + vec3<f32>(1e-5, 0.0, 0.0))), 0.0);
    col = mix(col, vec3<f32>(1.0, 0.45, 0.2) * 0.9, glow * toward * toward * exp(-abs(dir.y) * 5.0) * 0.85);
    // Stars rotate with the sky.
    let c = cos(sky.p.w);
    let sn = sin(sky.p.w);
    let rd = vec3<f32>(c * dir.x + sn * dir.y, -sn * dir.x + c * dir.y, dir.z);
    let g = rd * 90.0;
    let cell = floor(g);
    let f = fract(g) - 0.5;
    let r = hash31(cell);
    if (dir.y > 0.0 && r > 0.9965 && length(f) < 0.28) {
        let a = clamp((night - 0.35) * 1.6, 0.0, 1.0) * (0.55 + 0.45 * fract(r * 311.7));
        col = mix(col, vec3<f32>(1.0), a);
    }
    let sun = body(dir, s, 0.15, sky.r_sun);
    col = mix(col, sun.rgb, sun.a);
    let moon = body(dir, -s, 0.1, sky.r_moon);
    col = mix(col, moon.rgb, moon.a);
    return vec4<f32>(col, 1.0);
}

struct COut {
    @builtin(position) clip: vec4<f32>,
    @location(0) rel: vec3<f32>,
}

@vertex
fn vs_cloud(@location(0) pos: vec3<f32>) -> COut {
    var o: COut;
    let rel = vec3<f32>(pos.x, sky.p.x - sky.cam.y, pos.z);
    o.clip = sky.view_proj * vec4<f32>(rel, 1.0);
    o.rel = rel;
    return o;
}

@fragment
fn fs_cloud(i: COut) -> @location(0) vec4<f32> {
    let wx = i.rel.x + sky.cam.x + sky.cam.w;
    let wz = i.rel.z + sky.cam.z;
    let cell = vec2<f32>(floor(wx / 12.0), floor(wz / 12.0));
    let t = vec2<f32>(((cell.x % 256.0) + 256.0) % 256.0, ((cell.y % 256.0) + 256.0) % 256.0);
    let size = vec2<f32>(textureDimensions(tex));
    let atlas_px = vec2<f32>(sky.r_clouds.x, sky.r_clouds.y) * size + t;
    let px = textureLoad(tex, vec2<i32>(atlas_px), 0);
    if (px.a < 0.5) { discard; }
    let dist = length(i.rel.xz);
    let fade = 1.0 - smoothstep(sky.p.y, sky.p.z * 1.6, dist);
    let a = 0.8 * fade;
    if (a <= 0.01) { discard; }
    return vec4<f32>(sky.cloud_color.rgb, a);
}
