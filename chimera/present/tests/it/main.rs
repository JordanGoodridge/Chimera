//! `chimera_present`'s integration tests, as one test binary (each file a
//! module): spec test cases of the renderer's and audio's CPU side that
//! need nothing above the presentation, and scenario tests of what it
//! draws on the headless harness (`chimera_world::harness::Sim`, the
//! engine's `core` and console as the base: prop LODs, prop shadows),
//! and the view-model camera's switch. `cargo test -p chimera_present
//! --features dev --test it`; where tests go: docs/OBSERVABILITY.md,
//! section 1.

mod prop_lods;
mod prop_shadows;
mod shader_spec;
mod shadow_spec;
mod view_model_camera;
mod wav_playback;
