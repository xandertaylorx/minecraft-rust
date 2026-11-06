struct ShadowU { light_vp: mat4x4<f32> }
@group(0) @binding(0) var<uniform> sh: ShadowU;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
struct Chunk { origin: vec4<f32> }
@group(1) @binding(0) var<uniform> chunk: Chunk;

struct VIn { @location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>, @location(3) light: vec2<f32> }
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> }

@vertex
fn vs_shadow(v: VIn) -> VOut {
    var o: VOut;
    o.clip = sh.light_vp * vec4<f32>(v.pos + chunk.origin.xyz, 1.0);
    o.uv = v.uv;
    return o;
}

@fragment
fn fs_shadow(i: VOut) {
    let t = textureSampleLevel(atlas, samp, i.uv, 0.0);
    if (t.a < 0.1) { discard; }
}
