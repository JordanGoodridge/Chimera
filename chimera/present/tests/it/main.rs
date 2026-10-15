//! `chimera_present`'s integration tests, as one test binary (each file a
//! module): spec test cases of the renderer's and audio's CPU side that
//! need nothing above the presentation. `cargo test -p chimera_present
//! --features dev --test it`; where tests go: docs/OBSERVABILITY.md,
//! section 1.

mod shader_spec;
mod shadow_spec;
mod wav_playback;
