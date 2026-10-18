//! `dump` (the root package's install inspector) against a real CS:S
//! install. Skipped (passes with a note) when chimera.local.toml has no
//! usable `[games.cs_source]` path.

use std::{path::PathBuf, process::Command};

use mashup::{games::cs_source, mount::config::LocalConfig};

fn install() -> Option<PathBuf> {
    let path = LocalConfig::load().ok()?.game_path(cs_source::GAME)?;
    if path.join("cstrike").is_dir() {
        Some(path)
    } else {
        eprintln!("skipping: no CS:S install at {}", path.display());
        None
    }
}

#[test]
fn dump_extracts_outside_the_repo_and_refuses_inside() {
    let Some(install) = install() else { return };
    let dump = env!("CARGO_BIN_EXE_dump");
    let run = |out: &PathBuf| {
        Command::new(dump)
            .args(["cs_source", "--filter", "maps/de_dust2.bsp", "--extract", "--install"])
            .arg(&install)
            .arg("--out")
            .arg(out)
            .output()
            .unwrap()
    };

    let outside = std::env::temp_dir().join(format!("chimera-dump-test-{}", std::process::id()));
    let ok = run(&outside);
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    let extracted = std::fs::metadata(outside.join("maps/de_dust2.bsp")).unwrap().len();
    assert_eq!(
        extracted,
        std::fs::metadata(install.join("cstrike/maps/de_dust2.bsp"))
            .unwrap()
            .len()
    );
    std::fs::remove_dir_all(&outside).unwrap();

    let inside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/dump-test-must-refuse");
    let refused = run(&inside);
    assert!(!refused.status.success(), "extracting into the repo must fail");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("inside the repository"));
    let _ = std::fs::remove_dir_all(&inside);
}
