struct Globals {
    view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    // x: day brightness, y: fog start, z: fog end, w: gamma
    params: vec4<f32>,
}
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
struct Chunk { origin: vec4<f32> }
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

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, samp, i.uv);
    if (t.a < 0.1) { discard; }
    var rgb = t.rgb * i.color.rgb * lightmap(i.light.x, i.light.y);
    let fog = clamp((i.dist - g.params.y) / (g.params.z - g.params.y), 0.0, 1.0);
    rgb = mix(rgb, g.fog_color.rgb, fog);
    return vec4<f32>(rgb, t.a * i.color.a);
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
    return o;
}
