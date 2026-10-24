// Depth prepass for opaque props inside their fade band: Bevy's depth-only
// prepass has no fragment stage, so without this the dithered-out pixels
// would still write depth (prop_material.rs, `specialize`).

import bevy_pbr::prepass::io::VertexOutput;
import chimera_present::render::dither::dithered_out;

@fragment
fn fragment(in: VertexOutput) {
    @if(VISIBILITY_RANGE_DITHER) {
        if dithered_out(in.position, in.visibility_range_dither) {
            discard;
        }
    }
}
