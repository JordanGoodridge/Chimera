//! `affected`: which workspace packages' tests a change can affect
//! (repo_checks' `affected` module has the rules), printed as arguments
//! for `cargo nextest run` (or `cargo test`), one per line on stdout; what
//! was selected and why goes to stderr. The git hooks run it
//! (.githooks/pre-commit on the staged files, pre-push on what's pushed).
//!
//! ```text
//! cargo run -q -p repo_checks --bin affected -- [CHANGES] [OUTPUT]
//! CHANGES (any number; together their union):
//!   --staged          the index against HEAD (a commit about to be made)
//!   --range A..B      what B changes against A (`git diff A B`)
//!   --new SHA         a new branch: SHA against its merge-base with
//!                     origin/main (everything without one)
//!   --base REV        HEAD against its merge-base with REV
//!   --files F...      these paths (relative to the repository root)
//! OUTPUT:
//!   --format nextest  (default) `cargo nextest run` arguments
//!   --format cargo    `cargo test` arguments (whole packages)
//!   --skip-heavy      leave out `heavy::` tests (nextest; the fast tier)
//!   --quiet           no summary on stderr
//! ```
//!
//! Example: `cargo nextest run --features dev $(cargo run -q -p
//! repo_checks --bin affected -- --base origin/main)` in a shell that
//! splits on newlines only (the filter has spaces).

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use repo_checks::affected::{Selection, Workspace};

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Changed paths from `git diff --name-only` with `args`.
fn diff(root: &Path, args: &[&str], out: &mut Vec<String>) -> Result<(), String> {
    let mut all = vec!["diff", "--name-only", "--no-renames"];
    all.extend_from_slice(args);
    out.extend(git(root, &all)?.lines().filter(|l| !l.is_empty()).map(String::from));
    Ok(())
}

fn usage() -> &'static str {
    "usage: affected [--staged] [--range A..B] [--new SHA] [--base REV] [--files F...] \
     [--format nextest|cargo] [--skip-heavy] [--quiet]"
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("affected: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let root = PathBuf::from(git(Path::new("."), &["rev-parse", "--show-toplevel"])?.trim());
    let mut changed = Vec::new();
    let mut everything: Option<String> = None;
    let (mut format, mut skip_heavy, mut quiet) = ("nextest".to_string(), false, false);
    let mut args = std::env::args().skip(1).peekable();
    let mut any = false;
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value\n{}", usage()));
        match arg.as_str() {
            "--staged" => {
                any = true;
                diff(&root, &["--cached"], &mut changed)?;
            }
            "--range" => {
                any = true;
                let range = value()?;
                let (a, b) = range.split_once("..").ok_or("--range wants A..B")?;
                diff(&root, &[a, b], &mut changed)?;
            }
            "--new" => {
                any = true;
                let sha = value()?;
                match git(&root, &["merge-base", &sha, "origin/main"]) {
                    Ok(base) => diff(&root, &[base.trim(), &sha], &mut changed)?,
                    Err(_) => everything = Some(format!("{sha} has no merge-base with origin/main")),
                }
            }
            "--base" => {
                any = true;
                let rev = value()?;
                let base = git(&root, &["merge-base", &rev, "HEAD"])?;
                diff(&root, &[base.trim(), "HEAD"], &mut changed)?;
            }
            "--files" => {
                any = true;
                while let Some(f) = args.next_if(|a| !a.starts_with("--")) {
                    changed.push(f.replace('\\', "/"));
                }
            }
            "--format" => format = value()?,
            "--skip-heavy" => skip_heavy = true,
            "--quiet" => quiet = true,
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
    }
    if !any {
        return Err(format!("say what changed\n{}", usage()));
    }
    changed.sort();
    changed.dedup();
    let ws = Workspace::load(&root)?;
    let mut sel = Selection::new(&ws, &root, &changed);
    if let Some(why) = everything {
        sel.all = Some(why);
    }
    if !quiet {
        eprint!("{}", sel.summary());
    }
    let out = match format.as_str() {
        "nextest" => sel.nextest_args(skip_heavy),
        "cargo" => sel.cargo_args(),
        other => return Err(format!("unknown format {other}\n{}", usage())),
    };
    for a in out {
        println!("{a}");
    }
    Ok(())
}
