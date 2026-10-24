// Particles: texture x vertex colour (the particle's colour and alpha),
// alpha-blended or added, then fogged: blended particles toward the fog
// colour, added ones toward black (Source's sprite-card fog). The fog is
// the world's range fog and, below the water's surface, the water's
// (render::fog).

import bevy_pbr::render::forward_io::VertexOutput;
import chimera_present::render::fog::source_fog;

struct FogUniform {
    color: vec4<f32>,
    range: vec4<f32>,
    water_color: vec4<f32>,
    water_range: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var base_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var base_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var<uniform> fog: FogUniform;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var c = textureSample(base_texture, base_sampler, in.uv);
    @if(VERTEX_COLORS) {
        c = c * in.color;
    }
    let f = source_fog(fog.color, fog.range, fog.water_color, fog.water_range, in.world_position.xyz);
    @if(ADDITIVE) {
        // Premultiplied blending with alpha 0 adds.
        return vec4<f32>(c.rgb * c.a * (1.0 - f.a), 0.0);
    } @else {
        return vec4<f32>(mix(c.rgb, f.rgb, f.a), c.a);
    }
}
