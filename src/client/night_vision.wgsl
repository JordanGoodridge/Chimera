// Night vision (see night_vision.rs): the install's effects/nightvision
// material adds a copy of the frame buffer to it; drawn through a green
// filter (NIGHT_VISION_TINT, ours) that doubles the green of the 8-bit
// gamma values the game's frame buffer holds.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;

const TINT: vec3<f32> = vec3<f32>(0.0, 1.0, 0.0);

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(screen, screen_sampler, in.uv);
    let g = clamp(to_srgb(max(c.rgb, vec3<f32>(0.0))), vec3<f32>(0.0), vec3<f32>(1.0));
    let lit = clamp(g + g * TINT, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(to_linear(lit), c.a);
}
