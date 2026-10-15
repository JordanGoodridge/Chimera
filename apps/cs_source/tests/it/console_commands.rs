//! What only the app has: its build's version (`core::VERSION`, stamped
//! by its build.rs), as `version`, the main menu and bug reports show it.

/// The build's version (`version`, the main menu, bug reports): the tag
/// or `git describe`, then the short hash and the commit's UTC date; with
/// no tag just the hash and date; uncommitted changes marked.
#[test]
fn version_names_the_tag_hash_and_date() {
    use cs_source_app::core::{VERSION, version::format_version};
    assert_eq!(
        format_version(Some("v0.1.0-playtest3"), "feac156", "2026-10-11", false),
        "v0.1.0-playtest3 (feac156, 2026-10-11)"
    );
    assert_eq!(format_version(None, "feac156", "2026-10-11", false), "feac156, 2026-10-11");
    assert_eq!(format_version(None, "feac156", "2026-10-11", true), "feac156-dirty, 2026-10-11");
    // This build's (from git; "unknown" without it): ends in a date.
    if VERSION != "unknown" {
        let date = VERSION.trim_end_matches(')').rsplit(", ").next().unwrap();
        let digits: Vec<&str> = date.split('-').collect();
        assert!(
            digits.len() == 3 && digits.iter().all(|d| d.parse::<u32>().is_ok()),
            "{VERSION}"
        );
    }
    let text = cs_source_app::net::version_text();
    assert!(text.contains(VERSION) && text.contains(cs_source_app::net::NET_VERSION), "{text}");
}
