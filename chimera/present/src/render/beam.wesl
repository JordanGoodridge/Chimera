// Beams as Source's entity beams draw them (specs/source/visual_entities.md
// 1, 2.3): a strip from start to end through N points, each moved sideways
// by this frame's noise, widened sideways to face the camera (full width 2
// x the width), texture x colour, added to the scene; the texture repeats
// along the beam from its scroll offset; the colour shaded out along the
// beam over its fade length; a spotlight's beam also fades out as the eye
// comes near its axis.

import bevy_pbr::{
    render::forward_io::{Vertex, VertexOutput},
    render::mesh_view_bindings::view,
    render::view_transformations::position_world_to_clip,
};

struct BeamParams {
    start: vec4<f32>,
    end: vec4<f32>,
    // rgb x brightness; a: 1 shades out, 2 in, 3 both.
    color: vec4<f32>,
    // x: width at the start, y: at the end, z: fade length (0: the
    // whole beam), w: spotlight width W for the near-axis fade (0: none).
    widths: vec4<f32>,
    // x: points N, y: noise sample to sideways meters, z: texture offset
    // along the beam, w: its advance per segment.
    strip: vec4<f32>,
    // 129 noise samples, four to a vector.
    noise: array<vec4<f32>, 33>,
}

fn noise_sample(i: u32) -> f32 {
    return params.noise[i / 4u][i % 4u];
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> params: BeamParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var base_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var base_sampler: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    // uv.x: 0 or 1 across, uv.y: the point's number (past the last
    // point: the end).
    let n = max(params.strip.x, 2.0);
    let j = min(vertex.uv.y, n - 1.0);
    let f = j / (n - 1.0);
    let s = params.start.xyz;
    let e = params.end.xyz;
    let d = e - s;
    let len = max(length(d), 1e-6);
    let along = d / len;
    // Noise (1 step 5): sample int(127 f), across the beam and the view.
    let forward = -view.world_from_view[2].xyz;
    let across_raw = cross(forward, d);
    let across = across_raw / max(length(across_raw), 1e-6);
    let p = mix(s, e, f) + across * noise_sample(u32(127.0 * f)) * params.strip.y;
    let to_camera = view.world_position - p;
    let side_raw = cross(along, to_camera);
    let side = side_raw / max(length(side_raw), 1e-6);
    let width = mix(params.widths.x, params.widths.y, f);
    let world = p + side * (vertex.uv.x * 2.0 - 1.0) * width;

    // Shading along the beam (1.6).
    var fade = params.widths.z;
    if fade <= 0.0 {
        fade = len;
    }
    let ff = clamp(fade / len, 1e-6, 1.0);
    let mode = u32(params.color.a + 0.5);
    var b = 1.0;
    if mode == 1u {
        b = 1.0 - f / ff;
    } else if mode == 2u {
        b = f / ff;
    } else if mode == 3u {
        b = select(2.0 * (1.0 - f / ff), 2.0 * f / ff, f < 0.5);
    }
    b = clamp(b, 0.0, 1.0);
    // A spotlight's beam near its axis (2.3): 1 from 4W out, 0 within W.
    let w = params.widths.w;
    if w > 0.0 {
        let rel = view.world_position - s;
        let axis_dist = length(rel - along * dot(rel, along));
        b = b * clamp((axis_dist - w) / (3.0 * w), 0.0, 1.0);
    }

    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
    out.world_normal = normalize(to_camera);
    out.uv = vec2<f32>(vertex.uv.x, params.strip.z + j * params.strip.w);
    out.color = vec4<f32>(params.color.rgb * b, 1.0);
    @if(VERTEX_OUTPUT_INSTANCE_INDEX) {
        out.instance_index = vertex.instance_index;
    }
    @if(VISIBILITY_RANGE_DITHER) {
        out.visibility_range_dither = 0;
    }
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(base_texture, base_sampler, in.uv);
    return vec4<f32>(base.rgb * in.color.rgb, 1.0);
}
