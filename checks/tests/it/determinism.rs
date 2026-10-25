//! Determinism checks for the simulation crates (architecture review X2,
//! E1 step 4 and E2's check): the server and every client must run the
//! same simulation to the same result, so the simulation may not iterate
//! std's randomly seeded hash maps, and may not keep new process-wide
//! mutable state that outlives a `World`.
//!
//! Both rules are ratchets: each crate has a stored count of existing
//! uses (`BASELINE`); a count above it fails (a new use), a count below it
//! fails too until the number is lowered, so the counts only ever fall.
//! Skipped: code under `#[cfg(test)]` (from a column-0 attribute to the
//! end of the file), `tests.rs`, `tests/` folders and front ends (`ui`).

use std::{fs, path::Path};

use super::architecture::{root, rust_files};

/// Simulation crates (folder under the repository root) with their stored
/// counts: occurrences of std's `HashMap`/`HashSet`, then mutable statics
/// (`static mut`, statics of a `OnceLock`/`LazyLock`/`Mutex`/`RwLock`/
/// atomic/`Cell` type, `thread_local!`, `lazy_static!`). Lower a number
/// when you remove uses; never raise one.
const BASELINE: &[(&str, usize, usize)] = &[
    ("chimera/core/src", 1, 2),
    ("chimera/sim/src", 10, 0),
    ("chimera/world/src", 24, 1),
    ("chimera/net/src", 27, 0),
    ("source/logic/src", 9, 0),
    ("games/cs_source/src", 140, 6),
    ("games/combat_arms/src", 104, 22),
];

/// Type words that make a static mutable or lazily filled.
const STATE_TYPES: &[&str] = &[
    "OnceLock", "LazyLock", "OnceCell", "LazyCell", "Mutex", "RwLock", "RefCell", "Cell", "Atomic",
];

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The file's code without comments and without its test module.
fn code_of(file: &Path) -> String {
    let mut out = String::new();
    for line in fs::read_to_string(file).unwrap().lines() {
        if line.starts_with("#[cfg(test)]") {
            break;
        }
        out.push_str(line.split("//").next().unwrap_or(""));
        out.push('\n');
    }
    out
}

/// Occurrences of `word` as a whole word (`!` ends a macro name).
fn words(code: &str, word: &str) -> usize {
    code.match_indices(word)
        .filter(|(i, _)| {
            !code[..*i].chars().next_back().is_some_and(is_ident)
                && !code[i + word.len()..].chars().next().is_some_and(is_ident)
        })
        .count()
}

/// Whether the file takes `HashMap`/`HashSet` from std (Bevy's have a
/// fixed hasher): an import or a path through `std::collections::`.
fn uses_std_hash(code: &str) -> bool {
    let flat: String = code.split_whitespace().collect();
    let key = "std::collections::";
    let mut rest = flat.as_str();
    while let Some(i) = rest.find(key) {
        rest = &rest[i + key.len()..];
        let item = if rest.starts_with('{') {
            &rest[..rest.find('}').unwrap_or(rest.len())]
        } else {
            &rest[..rest.find([';', '(', ',', ')']).unwrap_or(rest.len())]
        };
        if item.starts_with('*') || item.contains("HashMap") || item.contains("HashSet") {
            return true;
        }
    }
    false
}

/// Every `HashMap`/`HashSet` word in a file that takes them from std.
fn std_hash_uses(code: &str) -> usize {
    if uses_std_hash(code) {
        words(code, "HashMap") + words(code, "HashSet")
    } else {
        0
    }
}

/// Mutable or lazily filled statics: `static [mut] NAME: <type> =` with
/// `mut` or a state type, plus each `thread_local!` and `lazy_static!`
/// (a `static` of a state type inside one counts as well).
fn mutable_statics(code: &str) -> usize {
    let mut n = words(code, "thread_local!") + words(code, "lazy_static!");
    for (i, _) in code.match_indices("static") {
        if code[..i].chars().next_back().is_some_and(|c| c == '\'' || is_ident(c)) {
            continue;
        }
        let rest = &code[i + "static".len()..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim_start();
        let (is_mut, rest) = match rest.strip_prefix("mut") {
            Some(r) if r.starts_with(char::is_whitespace) => (true, r.trim_start()),
            _ => (false, rest),
        };
        let name: String = rest.chars().take_while(|c| is_ident(*c)).collect();
        let Some(ty) = rest[name.len()..].trim_start().strip_prefix(':') else {
            continue;
        };
        let ty = &ty[..ty.find('=').unwrap_or(ty.len())];
        if is_mut || STATE_TYPES.iter().any(|t| ty.contains(t)) {
            n += 1;
        }
    }
    n
}

fn counts(dir: &str) -> (usize, usize) {
    let src = root().join(dir);
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let (mut hash, mut statics) = (0, 0);
    for file in files {
        let rel = file.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        if rel.ends_with("tests.rs")
            || rel.ends_with("ui.rs")
            || rel.split('/').any(|p| p == "tests" || p == "ui")
        {
            continue;
        }
        let code = code_of(&file);
        hash += std_hash_uses(&code);
        statics += mutable_statics(&code);
    }
    (hash, statics)
}

fn ratchet(what: &str, dir: &str, now: usize, stored: usize, fix: &str, errors: &mut Vec<String>) {
    if now > stored {
        errors.push(format!("{dir}: {now} {what}, {stored} allowed (+{}). {fix}", now - stored));
    } else if now < stored {
        errors.push(format!(
            "{dir}: {now} {what}, but BASELINE in checks/tests/it/determinism.rs allows {stored}: \
             lower the stored number to {now} (the counts only fall)."
        ));
    }
}

#[test]
fn simulation_is_deterministic() {
    let mut errors = Vec::new();
    for (dir, hash, statics) in BASELINE {
        let (h, s) = counts(dir);
        ratchet(
            "uses of std's HashMap/HashSet",
            dir,
            h,
            *hash,
            "std's hasher is seeded per process, so iterating one orders differently on every run, \
             server and client (a flaky test, a misprediction). Use `BTreeMap`/`BTreeSet` (ordered), \
             or Bevy's `bevy::platform::collections::{HashMap, HashSet}` (fixed hasher) for lookups \
             only; never sort a hash map's iteration with ties.",
            &mut errors,
        );
        ratchet(
            "mutable statics (static mut, OnceLock/LazyLock/Mutex/atomic/thread_local state)",
            dir,
            s,
            *statics,
            "Process-wide state outlives the `World` and is shared by every app in the process (a \
             server and its clients in `NetSim`, two matches). Keep the state in a `Resource` the \
             game's plugin inserts (architecture review E2); read-only `static` tables are fine.",
            &mut errors,
        );
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// The scanners find what they should (so a quiet ratchet means no uses).
#[test]
fn determinism_scanners_work() {
    assert_eq!(std_hash_uses("use std::collections::HashMap;\nlet m: HashMap<u8, u8>;"), 2);
    assert_eq!(std_hash_uses("use std::collections::{BTreeMap, HashSet};\nHashSet::new()"), 2);
    assert_eq!(std_hash_uses("let s = std::collections::HashSet::<u8>::new();"), 1);
    assert_eq!(std_hash_uses("use bevy::platform::collections::HashMap;\nHashMap::new()"), 0);
    assert_eq!(std_hash_uses("use std::collections::BTreeMap;"), 0);
    assert_eq!(mutable_statics("static A: OnceLock<u8> = OnceLock::new();"), 1);
    assert_eq!(mutable_statics("static mut N: u32 = 0;"), 1);
    assert_eq!(mutable_statics("static N: Mutex<Vec<u8>> = Mutex::new(vec![]);"), 1);
    assert_eq!(mutable_statics("thread_local! { static X: Cell<u8> = Cell::new(0); }"), 2);
    assert_eq!(mutable_statics("static T: [u8; 2] = [1, 2];\nfn f(x: &'static str) {}"), 0);
    assert_eq!(mutable_statics("static I: GameInfo = GameInfo { id: \"x\" };"), 0);
}
