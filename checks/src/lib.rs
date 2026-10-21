//! The repository's own checks (crate `repo_checks`, checks/): the
//! architecture tests (checks/tests/it/architecture.rs: layering, the asset, spec
//! and docs rules), `affected`, the test selection the git hooks run, and
//! `docs_index` (`docs_index.rs`), which generates the module and debt lists
//! from the sources' `//!` docs.
//!
//! `affected`: from the files a change touches, the workspace packages
//! whose tests it can affect, through cargo's crate graph (`cargo
//! metadata`): a package's tests depend on the package, its dependencies
//! and its dev-dependencies, so a change to a crate selects the tests of
//! every package whose tests depend on it (docs/plans/active/engine-split.md,
//! "Test selection": no coverage-based selection, the crate graph picks).
//! `cargo run -p repo_checks --bin affected -- --help` for the command.

pub mod affected;
pub mod docs_index;
