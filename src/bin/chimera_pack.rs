//! `chimera_pack`: a game install's files into the asset store
//! (docs/plans/active/asset-service.md, M1). A host tool: `--features tools`,
//! never in releases.
//!
//! `chimera_pack pack <game> [--set <name> --from <vfs log>]` reads files
//! through the game's own mount of its install (its search order; Combat
//! Arms' archives decrypted here with this host's keys, which are never
//! written anywhere), stores each as a record (SHA-256 id, zstd level 3)
//! and enters it in the manifest of the install's build
//! (`manifests/<game>/<build>.txt`). With `--from`, only the files of that
//! game in a `CHIMERA_VFS_LOG` log, entered as set `<name>` too; without,
//! every file the mount shows. Records already stored are skipped, so
//! packing again or a new build stores only what changed. The store is
//! `--store`, else `[assets] store`/`CHIMERA_ASSET_STORE`, else the user's
//! (`~/.cache/chimera/assets`).

use std::{path::PathBuf, process::ExitCode};

use chimera_assets::{Manifest, Store, pack, read_log};
use chimera_core::{
    game_registry::GameInfo,
    mount::{
        Mount,
        config::LocalConfig,
        install::{self, read_manifest},
    },
};

const USAGE: &str = "\
usage: chimera_pack pack <game> [options]
  games: cs_source, combat_arms
  --set <name>      enter the packed files as set <name> (needs --from)
  --from <log>      only the game's files in a CHIMERA_VFS_LOG log
  --store <dir>     the store (default: [assets] store, CHIMERA_ASSET_STORE, ~/.cache/chimera/assets)
  --install <dir>   the install (default: as the game finds it)
  --build <id>      the build id (default: the install's Steam app manifest)
  --threads <n>     reading and packing threads (default 4)";

/// A game the packer knows: its info, its install's own mount (without the
/// store or the log) and its Steam app.
struct Game {
    info: &'static GameInfo,
    open_install: fn(&std::path::Path) -> std::io::Result<Mount>,
    app: u32,
}

fn games() -> [Game; 2] {
    [
        Game { info: &cs_source::game::INFO, open_install: cs_source::game::mount::open_install, app: 240 },
        Game {
            info: &combat_arms::game::INFO,
            open_install: combat_arms::game::mount::open_install,
            // Combat Arms: the Classic.
            app: 1_263_550,
        },
    ]
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

fn run(args: &[String]) -> Result<(), String> {
    let mut it = args.iter();
    if it.next().map(String::as_str) != Some("pack") {
        return Err(USAGE.into());
    }
    let game_id = it.next().ok_or(USAGE)?;
    let (mut set, mut from, mut store, mut install, mut build, mut threads) = (None, None, None, None, None, 4);
    while let Some(arg) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--set" => set = Some(value()?),
            "--from" => from = Some(PathBuf::from(value()?)),
            "--store" => store = Some(Store::new(value()?)),
            "--install" => install = Some(PathBuf::from(value()?)),
            "--build" => build = Some(value()?),
            "--threads" => threads = value()?.parse().map_err(|_| "--threads: a number")?,
            _ => return Err(format!("unknown option {arg}\n{USAGE}")),
        }
    }
    if set.is_some() != from.is_some() {
        return Err("--set and --from go together".into());
    }
    let games = games();
    let game = games.iter().find(|g| g.info.id == game_id).ok_or_else(|| format!("unknown game {game_id}\n{USAGE}"))?;
    let config = LocalConfig::load()?;
    let store = store.or_else(|| config.assets.resolve().store).ok_or("no store folder: pass --store")?;
    // The real install, whatever `[assets] use_install` says.
    let install = match install {
        Some(p) => p,
        None => install::resolve(&config, game.info.id, game.info.steam.as_ref())
            .install
            .map(|i| i.path)
            .ok_or_else(|| format!("no {} install found: pass --install", game.info.id))?,
    };
    let build = match build {
        Some(b) => b,
        None => build_id(&install, game.app)
            .ok_or_else(|| format!("{}: no Steam app manifest for app {}: pass --build", install.display(), game.app))?,
    };
    let mount = (game.open_install)(&install).map_err(|e| format!("{}: {e}", install.display()))?;
    let paths: Vec<String> = match &from {
        Some(log) => {
            let lines = read_log(log)?;
            let mut paths: Vec<String> =
                lines.into_iter().filter(|l| l.game == game.info.id).map(|l| l.path).collect();
            paths.sort();
            paths.dedup();
            if paths.is_empty() {
                return Err(format!("{}: no {} files in the log", log.display(), game.info.id));
            }
            paths
        }
        None => mount.entries().into_iter().map(|(e, _)| e.path).collect(),
    };
    let manifest_path = store.manifest_path(game.info.id, &build);
    let mut manifest = if manifest_path.is_file() {
        Manifest::load(&manifest_path)?
    } else {
        Manifest::new(game.info.id, &build, Some(game.app))
    };
    println!(
        "packing {} files of {} build {} from {} into {}",
        paths.len(),
        game.info.id,
        build,
        install.display(),
        store.root.display()
    );
    let report = pack(&mount, &paths, &store, &mut manifest, set.as_deref(), threads).map_err(|e| e.to_string())?;
    manifest.save(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    println!(
        "{} files, {} read, {} records ({} new), {} stored ({:.0}% of what was read), {:.2} s",
        report.files,
        mb(report.bytes_read),
        report.records,
        report.new_records,
        mb(report.stored),
        100.0 * report.stored as f64 / report.bytes_read.max(1) as f64,
        report.took.as_secs_f64()
    );
    if !report.missing.is_empty() {
        println!("{} paths not in the install (left out):", report.missing.len());
        for p in &report.missing {
            println!("  {p}");
        }
    }
    println!("manifest: {}", manifest_path.display());
    Ok(())
}

/// The install's Steam build: its own `steamapps/` (an install made with
/// SteamCMD's `force_install_dir`), else its Steam library's.
fn build_id(install: &std::path::Path, app: u32) -> Option<String> {
    read_manifest(&install.join("steamapps").join(format!("appmanifest_{app}.acf")))
        .and_then(|m| m.build_id)
        .or_else(|| install::build_id_near(install, &install::SteamGame { app_id: app, name: "", required: &[] }))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
