//! Stamps the build with its git commit (`CHIMERA_GIT`) and version
//! (`CHIMERA_VERSION`): `core::BUILD` and `core::VERSION`, for bug reports
//! and the `version` command. The script every app shares
//! (chimera/core/build_stamp.rs: what it runs, when it reruns).

include!("../../chimera/core/build_stamp.rs");

fn main() {
    stamp();
}
