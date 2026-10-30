//! Particles (CS:S's tracers and impacts, Combat Arms' effect groups) are
//! depth tested: a wall between the eye and a tracer hides it
//! (specs/cs_source/tracers.md draws nothing over the world; only Combat
//! Arms' own tracer strips ignore depth, tested in that crate). The
//! pipeline needs a GPU, so this checks the depth state the material
//! specializes with.

use bevy::render::render_resource::{CompareFunction, DepthStencilState, TextureFormat};
use chimera_present::render::particles::ParticleDrawMaterial;

#[test]
fn particles_keep_the_depth_test() {
    let mut depth = DepthStencilState {
        format: TextureFormat::Depth32Float,
        depth_write_enabled: Some(true),
        depth_compare: Some(CompareFunction::GreaterEqual),
        stencil: Default::default(),
        bias: Default::default(),
    };
    ParticleDrawMaterial::apply_depth(&mut depth);
    assert_eq!(depth.depth_compare, Some(CompareFunction::GreaterEqual));
    assert_eq!(depth.depth_write_enabled, Some(false));
}
