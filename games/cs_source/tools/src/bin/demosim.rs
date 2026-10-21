//! Dev tool: play a CS:S client demo's recorded input (or a probe run's)
//! again through our movement and weapons and compare, tick by tick, with
//! what the recording says happened (`games::cs_source::resim`; see
//! docs/OBSERVABILITY.md, "Re-simulating a client demo").
//!
//! ```text
//! demosim <file.dem> [--player N] [--offset N] [--limit N] [--tolerance U]
//! demosim --probe <name> [--map de_dust2]    # the probe server's last run of <name>
//! ```
//!
//! A client demo (`record <name>` in a game client) holds the recording
//! player's commands; SourceTV demos have none. The probe's logs (the
//! server's `cstrike/mashup/<name>.in` and `.out`, written by movecmp's
//! and weapcmp's runs) hold the real game's states per tick. Prints the
//! report; the per-tick CSV goes to the dump directory (`demosim/`).

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use cs_source_app::{
    games::{
        self,
        cs_source::{
            demo,
            movement::SourceMovementPlugin,
            resim::{self, Options, Recording},
            weapons::CsWeaponsPlugin,
        },
    },
    harness::{CsSim, Sim},
    map::{MapData, MapDebugView, MapPlugin},
    mount::config::{LocalConfig, default_dump_dir},
};

struct Args {
    demo: Option<PathBuf>,
    probe: Option<String>,
    map: Option<String>,
    player: Option<u32>,
    opts: Options,
}

const USAGE: &str = "usage: demosim <file.dem> [--player N] [--offset N] [--limit N] [--tolerance UNITS]\n       \
                     demosim --probe <name> [--map de_dust2] [--limit N] [--tolerance UNITS]";

fn parse() -> Result<Option<Args>, String> {
    let mut a = Args {
        demo: None,
        probe: None,
        map: None,
        player: None,
        opts: Options::default(),
    };
    let mut it = std::env::args().skip(1);
    let num = |v: Option<String>, what: &str| -> Result<f64, String> {
        v.and_then(|v| v.parse().ok()).ok_or(format!("{what} needs a number"))
    };
    while let Some(x) = it.next() {
        match x.as_str() {
            "-h" | "--help" => return Ok(None),
            "--probe" => a.probe = Some(it.next().ok_or("--probe <name>")?),
            "--map" => a.map = Some(it.next().ok_or("--map <name>")?),
            "--player" => a.player = Some(num(it.next(), "--player")? as u32),
            "--offset" => a.opts.offset = Some(num(it.next(), "--offset")? as i64),
            "--limit" => a.opts.limit = Some(num(it.next(), "--limit")? as usize),
            "--tolerance" => a.opts.tolerance.position = num(it.next(), "--tolerance")? as f32,
            other if !other.starts_with("--") && a.demo.is_none() => a.demo = Some(other.into()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if a.demo.is_none() && a.probe.is_none() {
        return Err("name a demo file or --probe <name>".into());
    }
    Ok(Some(a))
}

fn recording(a: &Args) -> Result<Recording, String> {
    if let Some(path) = &a.demo {
        let d = demo::load(path)?;
        if !d.timeline.errors.is_empty() {
            eprintln!(
                "{} parse errors, first: {}",
                d.timeline.errors.len(),
                d.timeline.errors[0]
            );
        }
        let mut rec = Recording::from_demo(&d, a.player)?;
        rec.name = path.file_name().map_or(rec.name, |n| n.to_string_lossy().into_owned());
        return Ok(rec);
    }
    let name = a.probe.as_deref().expect("checked");
    let server = LocalConfig::load()?
        .game_path("cs_source_server")
        .ok_or("no [games.cs_source_server] path in chimera.local.toml (see chimera.local.example.toml)")?;
    let dir = server.join("cstrike").join("mashup");
    let read = |ext: &str| {
        let p = dir.join(format!("{name}.{ext}"));
        std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))
    };
    // The probe's runs are on de_dust2 unless named after another map.
    let map = a.map.clone().unwrap_or_else(|| {
        ["de_nuke", "de_aztec"]
            .into_iter()
            .find(|m| name.contains(m))
            .unwrap_or("de_dust2")
            .to_string()
    });
    Recording::from_probe(name, &map, &read("in")?, &read("out")?)
}

fn run(a: &Args) -> Result<(), String> {
    let rec = recording(a)?;
    println!(
        "{}: {} commands, {} recorded states, {} shots on {} ({:.0} ticks/s)",
        rec.name,
        rec.commands.len(),
        rec.states.len(),
        rec.shots.len(),
        rec.map,
        1.0 / rec.tick_interval
    );
    let map: Arc<MapData> = Arc::new(games::load_map(&format!("cs_source:{}", rec.map))?);
    let make = || {
        Sim::new(
            CsSim,
            (
                MapPlugin {
                    data: Some(map.clone()).into(),
                    view: MapDebugView::Normal,
                },
                SourceMovementPlugin,
                CsWeaponsPlugin,
            ),
        )
    };
    let report = resim::resimulate(&make, &rec, a.opts)?;
    print!("{report}");
    if let Some(dir) = default_dump_dir("demosim") {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(format!("{}.csv", rec.name.trim_end_matches(".dem")));
        std::fs::write(&file, report.csv()).map_err(|e| e.to_string())?;
        println!("per-tick CSV: {}", file.display());
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(Some(a)) => a,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
