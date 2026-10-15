//! An instructions-per-tick regression gate (docs/performance.md,
//! "Regression gate"): scenarios with fixed inputs, measured by hardware
//! counters (user-space instructions retired, summed over every thread),
//! compared with a stored baseline per build profile. Unlike frame times,
//! the counts barely move with the machine's load.
//!
//! The gate is game-neutral: an app or game names its scenarios and
//! builds each as something to tick (`Gate`, `main`); this module parses
//! the command line, measures, compares and updates the baseline. CS:S's
//! scenarios and binary: games/cs_source/tools/src/bin/perfgate.rs.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::Instant,
};

use chimera_core::metrics::{AllThreads, Counts, Percentiles, ThreadCounters, hw_status, short};
use serde::{Deserialize, Serialize};

/// Default allowed rise (percent): see docs/performance.md, "Regression
/// gate", for the measured run-to-run spread it is chosen from.
pub const DEFAULT_TOLERANCE: f64 = 3.0;

/// One scenario's measurements.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Measured {
    pub ticks: u64,
    /// Instructions per tick, all threads.
    pub instructions_total: u64,
    pub instructions_p50: f64,
    pub instructions_p99: f64,
    /// Instructions per tick on the main thread only.
    pub main_instructions_p50: f64,
    pub main_instructions_total: u64,
    /// CPU time per tick, all threads (ms), and wall time per tick: for
    /// reading, not gated (they follow the machine's load).
    pub cpu_ms_p50: f64,
    pub cpu_ms_p99: f64,
    pub wall_ms_p50: f64,
    pub wall_ms_p99: f64,
    pub cycles_total: u64,
    pub cache_misses_total: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct Baselines {
    /// Allowed rise, percent, and per scenario where its own run-to-run
    /// spread needs more (greybox: ticks of under a million instructions,
    /// where the task pools' overhead weighs more).
    tolerance_pct: f64,
    #[serde(default)]
    scenario_tolerance_pct: BTreeMap<String, f64>,
    /// By build profile (`playtest`, `debug`, ...), then scenario.
    profiles: BTreeMap<String, Profile>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Profile {
    /// The commit the numbers were taken at, and the CPU.
    build: String,
    cpu: String,
    scenarios: BTreeMap<String, Measured>,
}

/// What a gate measures: its scenarios' names, its baseline file and the
/// build it runs (the commit, recorded with an updated baseline).
pub struct Gate<'a> {
    pub name: &'a str,
    pub scenarios: &'a [&'a str],
    pub baseline: PathBuf,
    pub build: &'a str,
}

struct Args {
    scenarios: Vec<String>,
    ticks: u64,
    warmup: u64,
    tolerance: Option<f64>,
    baseline: PathBuf,
    update: bool,
    json: Option<PathBuf>,
}

fn usage(gate: &Gate) -> String {
    format!(
        "\
usage: {name} [--scenario <name>]... [--ticks N] [--warmup N] [--tolerance PCT]
                [--baseline <file>] [--update] [--json <file>]
  Runs headless scenarios with fixed inputs and counts user-space
  instructions per tick (all threads) with hardware counters; fails when a
  scenario's total rises more than the tolerance above the baseline for
  this build profile. Skips (exit 0) where counters are unavailable.
  --scenario <name>  {scenarios} (default: all
                     available)
  --ticks N          ticks measured per scenario (default 2000)
  --warmup N         ticks run first, not measured (default 300)
  --tolerance PCT    allowed rise in percent (default: the baseline's)
  --baseline <file>  default {baseline}
  --update           write this run's numbers as the baseline for this
                     build profile (after checking a change is wanted)
  --json <file>      also write this run's numbers there",
        name = gate.name,
        scenarios = gate.scenarios.join(", "),
        baseline = gate.baseline.display()
    )
}

fn parse_args(gate: &Gate) -> Result<Args, String> {
    let mut a = Args {
        scenarios: Vec::new(),
        ticks: 2000,
        warmup: 300,
        tolerance: None,
        baseline: gate.baseline.clone(),
        update: false,
        json: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--scenario" => a.scenarios.push(value()?),
            "--ticks" => a.ticks = value()?.parse().map_err(|e| format!("--ticks: {e}"))?,
            "--warmup" => a.warmup = value()?.parse().map_err(|e| format!("--warmup: {e}"))?,
            "--tolerance" => a.tolerance = Some(value()?.parse().map_err(|e| format!("--tolerance: {e}"))?),
            "--baseline" => a.baseline = value()?.into(),
            "--json" => a.json = Some(value()?.into()),
            "--update" => a.update = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    for s in &a.scenarios {
        if !gate.scenarios.contains(&s.as_str()) {
            return Err(format!("unknown scenario {s}"));
        }
    }
    Ok(a)
}

/// The gate's binary: parse the command line, build each wanted scenario
/// with `scenario` (None: skipped, e.g. its map isn't installed; Some: a
/// function running one tick), measure, then compare with the baseline or
/// (`--update`) write it. Exits the process with the verdict.
pub fn main<'s>(gate: Gate, mut scenario: impl FnMut(&str) -> Option<Box<dyn FnMut() + 's>>) {
    let args = match parse_args(&gate) {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}");
            }
            eprintln!("{}", usage(&gate));
            std::process::exit(2);
        }
    };
    // Counters first: without them there is nothing to gate.
    let mut probe = ThreadCounters::current();
    if probe.read().hw.is_none() {
        println!("{}: skipped, hardware counters {}", gate.name, hw_status());
        return;
    }
    let profile = build_profile();
    let wanted: Vec<&str> = if args.scenarios.is_empty() {
        gate.scenarios.to_vec()
    } else {
        args.scenarios.iter().map(String::as_str).collect()
    };
    let mut results = BTreeMap::new();
    for name in wanted {
        let Some(tick) = scenario(name) else {
            println!("{name}: skipped (map not installed or cached)");
            continue;
        };
        let r = measure(tick, args.warmup, args.ticks);
        println!(
            "{name}: {} ticks, instructions per tick p50 {} p99 {} total {} (main thread p50 {}); \
             cpu ms p50 {:.3} p99 {:.3}; wall ms p50 {:.3} p99 {:.3}",
            r.ticks,
            short(r.instructions_p50),
            short(r.instructions_p99),
            short(r.instructions_total as f64),
            short(r.main_instructions_p50),
            r.cpu_ms_p50,
            r.cpu_ms_p99,
            r.wall_ms_p50,
            r.wall_ms_p99
        );
        results.insert(name.to_string(), r);
    }
    let mut baselines: Baselines = std::fs::read_to_string(&args.baseline)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Baselines {
            tolerance_pct: DEFAULT_TOLERANCE,
            ..Default::default()
        });
    let this = Profile {
        build: gate.build.to_string(),
        cpu: cpu_model(),
        scenarios: results.clone(),
    };
    if let Some(path) = &args.json
        && let Err(e) = std::fs::write(path, serde_json::to_string_pretty(&this).unwrap())
    {
        eprintln!("{}: {e}", path.display());
    }
    if args.update {
        let entry = baselines.profiles.entry(profile.clone()).or_default();
        entry.build = this.build.clone();
        entry.cpu = this.cpu.clone();
        entry.scenarios.extend(results);
        if let Some(t) = args.tolerance {
            baselines.tolerance_pct = t;
        }
        if let Some(dir) = args.baseline.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(&args.baseline, serde_json::to_string_pretty(&baselines).unwrap() + "\n") {
            Ok(()) => println!("baseline for `{profile}` written to {}", args.baseline.display()),
            Err(e) => {
                eprintln!("{}: {e}", args.baseline.display());
                std::process::exit(1);
            }
        }
        return;
    }
    let Some(base) = baselines.profiles.get(&profile) else {
        println!(
            "no baseline for build profile `{profile}` in {}: nothing compared (--update writes one)",
            args.baseline.display()
        );
        return;
    };
    if base.cpu != this.cpu {
        println!("note: the baseline was taken on {:?}, this is {:?}", base.cpu, this.cpu);
    }
    let mut failed = Vec::new();
    for (name, r) in &this.scenarios {
        let Some(b) = base.scenarios.get(name) else {
            println!("{name}: no baseline");
            continue;
        };
        if b.ticks != r.ticks {
            println!(
                "{name}: baseline has {} ticks, this run {}: not compared",
                b.ticks, r.ticks
            );
            continue;
        }
        let tolerance = args
            .tolerance
            .or(baselines.scenario_tolerance_pct.get(name).copied())
            .unwrap_or(baselines.tolerance_pct);
        let change = (r.instructions_total as f64 / b.instructions_total.max(1) as f64 - 1.0) * 100.0;
        let verdict = if change > tolerance {
            failed.push(format!("{name} (tolerance {tolerance}%)"));
            "FAIL"
        } else if change < -tolerance {
            "faster (update the baseline if intended)"
        } else {
            "ok"
        };
        println!(
            "{name}: {} instructions vs baseline {} ({change:+.2}%, tolerance {tolerance}%): {verdict}",
            short(r.instructions_total as f64),
            short(b.instructions_total as f64)
        );
    }
    if !failed.is_empty() {
        eprintln!(
            "{}: {} rose by more than the tolerance in instructions: find the cause (tracesum, \
             mashup_hitch_ratio), or if the cost is wanted, `{} --update` (docs/performance.md)",
            gate.name,
            failed.join(", "),
            gate.name
        );
        std::process::exit(1);
    }
}

/// Run `warmup` ticks, then measure `ticks` ticks one at a time (`tick`
/// runs one).
pub fn measure(mut tick: impl FnMut(), warmup: u64, ticks: u64) -> Measured {
    for _ in 0..warmup {
        tick();
    }
    let mut all = AllThreads::default();
    let mut main = ThreadCounters::current();
    let (mut ins, mut main_ins, mut cpu, mut wall) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut total = Counts::default();
    let mut main_total = 0u64;
    all.read();
    for _ in 0..ticks {
        let (a0, m0, t0) = (all.read(), main.read(), Instant::now());
        tick();
        let wall_ms = t0.elapsed().as_secs_f64() * 1e3;
        let (a, m) = (all.read() - a0, main.read() - m0);
        let hw = a.hw.unwrap_or_default();
        let mhw = m.hw.unwrap_or_default();
        ins.push(hw.instructions as f64);
        main_ins.push(mhw.instructions as f64);
        cpu.push(a.cpu_ns as f64 / 1e6);
        wall.push(wall_ms);
        total = if total.hw.is_none() { a } else { total + a };
        main_total += mhw.instructions;
    }
    let p = |v: &[f64]| Percentiles::of(v).unwrap_or_default();
    let (pi, pm, pc, pw) = (p(&ins), p(&main_ins), p(&cpu), p(&wall));
    let hw = total.hw.unwrap_or_default();
    Measured {
        ticks,
        instructions_total: hw.instructions,
        instructions_p50: pi.p50,
        instructions_p99: pi.p99,
        main_instructions_p50: pm.p50,
        main_instructions_total: main_total,
        cpu_ms_p50: pc.p50,
        cpu_ms_p99: pc.p99,
        wall_ms_p50: pw.p50,
        wall_ms_p99: pw.p99,
        cycles_total: hw.cycles,
        cache_misses_total: hw.cache_misses,
    }
}

/// Which build this binary is (`playtest`, `debug`, `release`): the
/// target folder it runs from.
fn build_profile() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent()?.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

fn cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| std::env::consts::ARCH.to_string())
}
