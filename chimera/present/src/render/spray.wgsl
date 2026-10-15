// A player's spray (see spray.rs): the logo alpha blended over the wall,
// lit by the baked light where it lies, as the game draws a lightmapped
// translucent decal.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> light: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var logo: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var logo_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(logo, logo_sampler, in.uv);
    if texel.a < 0.004 {
        discard;
    }
    return vec4<f32>(texel.rgb * light.rgb, texel.a);
}
