// A parallelogram of an image (or a flat colour) inside an unturned node
// (clip_quad.rs). The node is the shape's bounding box already cut to the
// clip rectangle; the fragment finds its place (a, b) on the
// parallelogram and discards what lies outside it. Bevy's own clip does
// not work on turned nodes, hence this.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct QuadParams {
    color: vec4<f32>,
    // The parallelogram's first corner (xy) and its edge u (zw), px from
    // the node's top-left.
    origin_u: vec4<f32>,
    // Its edge v (xy), unused (zw).
    v_pad: vec4<f32>,
    // The image part drawn: uv offset (xy) and size (zw), over (a, b).
    uv: vec4<f32>,
}

@group(1) @binding(0) var<uniform> params: QuadParams;
@group(1) @binding(1) var image: texture_2d<f32>;
@group(1) @binding(2) var image_sampler: sampler;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * in.size;
    let u = params.origin_u.zw;
    let v = params.v_pad.xy;
    let d = p - params.origin_u.xy;
    let det = u.x * v.y - u.y * v.x;
    let a = (d.x * v.y - d.y * v.x) / det;
    let b = (u.x * d.y - u.y * d.x) / det;
    if a < 0.0 || a > 1.0 || b < 0.0 || b > 1.0 {
        discard;
    }
    let t = params.uv.xy + vec2<f32>(a, b) * params.uv.zw;
    // Outside the image: clamp-to-edge, the outermost texel row or column
    // repeated (radar_minimap.md Corrections 2026-10-28, 2), not a border.
    let c = textureSample(image, image_sampler, clamp(t, vec2<f32>(0.0), vec2<f32>(1.0)));
    return vec4<f32>(params.color.rgb * c.rgb, params.color.a * c.a);
}
