//! Regenerates the doc lists built from the sources' `//!` docs
//! (`docs/modules.md`, the generated block of `docs/tech-debt.md`); see
//! `repo_checks::docs_index`.
//!
//! `cargo run -q -p repo_checks --bin docs_index` writes them;
//! `-- --check` writes nothing and exits 1 when they are stale. Run it from
//! anywhere in the repository.

use std::path::PathBuf;
use std::process::ExitCode;

/// The repository root: the nearest ancestor of the working directory with
/// both `Cargo.toml` and `docs/`.
fn root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("working directory");
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("docs").is_dir() {
            return dir;
        }
        assert!(dir.pop(), "run inside the repository");
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "usage: docs_index [--check]\n\
             Rebuilds docs/modules.md and docs/tech-debt.md's generated entries from the `//!` docs."
        );
        return ExitCode::SUCCESS;
    }
    let check = args.iter().any(|a| a == "--check");
    let root = root();
    let stale = match repo_checks::docs_index::stale(&root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    for (path, content) in &stale {
        if check {
            eprintln!("{path} is stale");
        } else {
            std::fs::write(root.join(path), content).expect("write");
            println!("wrote {path}");
        }
    }
    if check && !stale.is_empty() {
        eprintln!("run: {}", repo_checks::docs_index::COMMAND);
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
