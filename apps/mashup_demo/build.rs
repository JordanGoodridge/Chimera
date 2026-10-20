//! Stamps the build with its git commit (`CHIMERA_GIT`) and version
//! (`CHIMERA_VERSION`): `mashup_demo::BUILD` and `VERSION`, for bug
//! reports and the `version` command. The script every app shares
//! (chimera/core/build_stamp.rs), as the other apps.

include!("../../chimera/core/build_stamp.rs");

fn main() {
    stamp();
}
