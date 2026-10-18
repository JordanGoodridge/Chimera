// The build's version text. Shared with the apps' build scripts
// (`include!`d by chimera/core/build_stamp.rs), so plain Rust with no
// crate paths and no inner doc comments.

/// "<tag or describe> (<short hash>, <commit date>)", e.g.
/// "v0.1.0-playtest3 (feac156, 2026-10-11)"; with no tag reachable just
/// "feac156, 2026-10-11". Uncommitted changes mark the hash `-dirty`.
pub fn format_version(describe: Option<&str>, hash: &str, date: &str, dirty: bool) -> String {
    let hash = if dirty { format!("{hash}-dirty") } else { hash.to_string() };
    match describe.filter(|d| !d.is_empty()) {
        Some(d) => format!("{d} ({hash}, {date})"),
        None => format!("{hash}, {date}"),
    }
}
