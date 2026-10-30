//! `chimera_assets`: the local record store's command line
//! (docs/plans/active/asset-service.md). `status` lists each game build's
//! manifest, its files, sets and sizes; `verify` reads every record a
//! manifest refers to and checks its hash (exit 1 on a missing or corrupt
//! one); `gc` deletes records no manifest refers to (`--dry-run` lists
//! them). The store is `--store <dir>`, else `CHIMERA_ASSET_STORE`, else
//! the user's (`~/.cache/chimera/assets`, `%LOCALAPPDATA%\chimera\assets`);
//! `[assets] store` in chimera.local.toml is the games' setting, not read
//! here. `login` and `fetch` come with M2.

use std::{collections::BTreeSet, process::ExitCode};

use chimera_assets::{Manifest, Store, config};

const USAGE: &str = "usage: chimera_assets [--store <dir>] <status | verify | gc [--dry-run]>";

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

fn manifests(store: &Store) -> Vec<Manifest> {
    let mut out = Vec::new();
    for game in store.games() {
        for build in store.builds(&game) {
            match Manifest::load(&store.manifest_path(&game, &build)) {
                Ok(m) => out.push(m),
                Err(e) => eprintln!("warning: {e}"),
            }
        }
    }
    out
}

fn status(store: &Store) -> ExitCode {
    println!("store: {}", store.root.display());
    let ids = store.record_ids();
    let on_disk: u64 = ids
        .iter()
        .filter_map(|id| std::fs::metadata(store.record_path(id)).ok())
        .map(|m| m.len())
        .sum();
    println!("records: {} ({} on disk)", ids.len(), mb(on_disk));
    for m in manifests(store) {
        let records = m.records();
        let plain: u64 = records.values().map(|v| v.0).sum();
        let stored: u64 = records.values().map(|v| v.1).sum();
        let missing = records.keys().filter(|id| !store.has(id)).count();
        println!(
            "{} build {}{}: {} files, {} records, {} plain, {} stored ({:.0}%){}",
            m.game,
            m.build,
            m.app.map(|a| format!(" (app {a})")).unwrap_or_default(),
            m.files.len(),
            records.len(),
            mb(plain),
            mb(stored),
            100.0 * stored as f64 / plain.max(1) as f64,
            if missing > 0 { format!(", {missing} records missing") } else { String::new() },
        );
        for name in m.sets.keys() {
            let (n, plain, stored) = m.set_size(name);
            println!("  set {name}: {} files, {n} records, {} plain, {} stored", m.sets[name].len(), mb(plain), mb(stored));
        }
    }
    ExitCode::SUCCESS
}

fn verify(store: &Store) -> ExitCode {
    let mut checked = BTreeSet::new();
    let mut bad = 0;
    for m in manifests(store) {
        for (path, f) in &m.files {
            if !checked.insert(f.id.clone()) {
                continue;
            }
            if let Err(e) = store.get(&f.id, f.size) {
                bad += 1;
                println!("BAD {} {}: {path}: {e}", m.game, m.build);
            }
        }
    }
    println!("{} records checked, {bad} bad", checked.len());
    if bad == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn gc(store: &Store, dry_run: bool) -> ExitCode {
    let used: BTreeSet<String> =
        manifests(store).iter().flat_map(|m| m.files.values().map(|f| f.id.clone())).collect();
    let (mut n, mut bytes) = (0, 0);
    for id in store.record_ids().into_iter().filter(|id| !used.contains(id)) {
        let path = store.record_path(&id);
        bytes += std::fs::metadata(&path).map_or(0, |m| m.len());
        n += 1;
        if dry_run {
            println!("unused {id}");
        } else if let Err(e) = std::fs::remove_file(&path) {
            eprintln!("{}: {e}", path.display());
        }
    }
    println!("{} {n} unused records ({})", if dry_run { "would delete" } else { "deleted" }, mb(bytes));
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut store = None;
    if let Some(i) = args.iter().position(|a| a == "--store") {
        if i + 1 >= args.len() {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
        store = Some(Store::new(args.remove(i + 1)));
        args.remove(i);
    }
    let Some(store) = store.or_else(|| config::AssetsConfig::default().resolve().store) else {
        eprintln!("no store: pass --store <dir> or set CHIMERA_ASSET_STORE");
        return ExitCode::from(2);
    };
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["status"] => status(&store),
        ["verify"] => verify(&store),
        ["gc"] => gc(&store, false),
        ["gc", "--dry-run"] => gc(&store, true),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
