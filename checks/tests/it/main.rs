//! `repo_checks`' integration tests, as one test binary: the architecture
//! tests (the repository's layering, asset, spec and docs rules). They run
//! on every selection of `affected`, docs-only changes included, and build
//! in seconds (no workspace crate under them). `cargo test -p repo_checks`.

mod architecture;
mod determinism;
