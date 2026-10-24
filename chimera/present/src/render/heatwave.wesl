// A heat-wave steam puff (env_steam Type 1, `sprites/heatwave`): a quad in
// the view plane that shows the scene behind it shifted by a random
// direction, strongest at the middle and fading to nothing at the edge, so
// no seam shows. The direction (red and green, 0..1 for -1..1) is picked per
// puff each frame by the emitter; alpha is the puff's opacity, which scales
// the shift. Reads the view transmission texture (the opaque scene), so it
// draws in Bevy's transmissive phase.

import bevy_pbr::{
    render::forward_io::VertexOutput,
    render::mesh_view_bindings as vb,
    render::mesh_view_bindings::view,
};

// x: the largest shift, as a fraction of the screen.
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let screen = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    let d = in.uv * 2.0 - 1.0;
    let falloff = max(1.0 - dot(d, d), 0.0);
    var shift = vec2<f32>(0.0);
    @if(VERTEX_COLORS) {
        shift = (in.color.rg * 2.0 - 1.0) * in.color.a * falloff * params.x;
    }
    let at = clamp(screen + shift, vec2<f32>(0.0), vec2<f32>(1.0));
    let c = textureSampleLevel(vb::view_transmission_texture, vb::view_transmission_sampler, at, 0.0).rgb;
    return vec4<f32>(c, 1.0);
}
