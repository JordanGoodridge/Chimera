//! Development tool: which code takes the time, from a CPU profile or a
//! span trace, as a top-N table and pictures to compare before and after
//! a change (docs/performance.md, "Measuring").
//!
//! Input: `perf script` output (Linux: `perf record -g`, e.g.
//! tools/perf-profile.sh) or folded stacks (`a;b;c N` lines:
//! `tracesum --folded`, inferno's collapsers, any other profiler's
//! folded export). Each stack's first frame is its thread.
//!
//! Prints and writes into `--out <dir>`:
//! - `ours`: each sample charged to the deepest frame in this repo's code
//!   (crates named by `--ours`), so a `memcpy` or a hash map inside one of
//!   our systems counts for that system; samples with no frame of ours are
//!   charged to their outermost function past the plumbing (Bevy's own
//!   system, mostly: `(not ours) ..`);
//! - `self`: functions by their own samples (the leaf frame);
//! - `total`: functions by samples with them anywhere on the stack;
//! - per thread: its share, so a render thread at 90% and a main thread at
//!   30% says which one sets the frame rate;
//! - stacks without the plumbing between a thread and its systems (thread
//!   start, the event loop, executors, task futures; `--keep-plumbing`
//!   keeps it);
//! - `flame.svg` (a flame graph: a tower per thread, width = time, colour
//!   by crate family: ours blue, Bevy orange, wgpu and drivers green, the
//!   standard library and allocator grey; hover for the numbers), `top.svg`
//!   (bars of the `ours` top N; with `--baseline`, before and after side
//!   by side), `families.svg` (each thread's time as one bar split by
//!   whose code runs: ours, Bevy, wgpu and the driver, audio, the
//!   allocator), `top.json` (the numbers, for `--baseline` next time),
//!   `table.md` (the tables as Markdown).
//!
//! Units: samples become ms per frame with `--frames <n>` (frames the
//! profile covers) and `--freq <hz>` (perf's `-F`, default 999: one sample
//! a millisecond of a thread's CPU time); folded span stacks from
//! `tracesum` are in µs (`--unit us`).
//!
//! Known shortcuts: perf's frequency mode makes a sample's CPU time
//! approximate; samples are folded per thread name, so threads that share
//! one (a pool's) add up; inlined functions show only when perf's unwinding
//! names them (`--call-graph dwarf` on a build with line tables: the
//! `profiling` profile).

use std::{
    collections::HashMap,
    fmt::Write as _,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::ExitCode,
};

const USAGE: &str = "\
usage: profsum <perf-script.txt | stacks.folded | -> [options]
  --frames <n>        frames the profile covers (ms per frame columns)
  --freq <hz>         perf's sampling frequency (default 999)
  --unit <samples|us> what a folded stack's count is (default samples;
                      tracesum --folded writes us)
  --top <n>           rows per table (default 25)
  --ours <a,b,..>     crate name prefixes of this repo's code (default
                      chimera,cs_source,combat_arms,party_kit,source_,
                      originals,lucker_party,ultimate_gun_game,mashup)
  --thread <text>     only threads whose name contains <text>
  --focus <text>      only stacks with a frame containing <text> (who
                      calls it: the `ours` table)
  --keep-plumbing     keep the event loop's, executors' and task pools'
                      frames (left out by default)
  --out <dir>         write flame.svg, top.svg, families.svg, top.json,
                      table.md there
  --baseline <json>   an earlier top.json: before, after and delta columns
  --title <text>      the pictures' title";

const DEFAULT_OURS: &str =
    "chimera,cs_source,combat_arms,party_kit,source_,originals,lucker_party,ultimate_gun_game,mashup";

#[derive(Default, Clone, Copy)]
struct Row {
    ms: f64,
}

struct Opts {
    frames: Option<f64>,
    freq: f64,
    unit_us: bool,
    top: usize,
    ours: Vec<String>,
    thread: Option<String>,
    focus: Option<String>,
    keep_plumbing: bool,
    out: Option<PathBuf>,
    baseline: Option<PathBuf>,
    title: String,
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut input = None;
    let mut o = Opts {
        frames: None,
        freq: 999.0,
        unit_us: false,
        top: 25,
        ours: DEFAULT_OURS.split(',').map(String::from).collect(),
        thread: None,
        focus: None,
        keep_plumbing: false,
        out: None,
        baseline: None,
        title: "CPU profile".into(),
    };
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_default();
        match a.as_str() {
            "--frames" => o.frames = val().parse().ok(),
            "--freq" => o.freq = val().parse().unwrap_or(o.freq),
            "--unit" => o.unit_us = val() == "us",
            "--top" => o.top = val().parse().unwrap_or(o.top),
            "--ours" => o.ours = val().split(',').map(String::from).collect(),
            "--thread" => o.thread = Some(val()),
            "--focus" => o.focus = Some(val()),
            "--keep-plumbing" => o.keep_plumbing = true,
            "--out" => o.out = Some(val().into()),
            "--baseline" => o.baseline = Some(val().into()),
            "--title" => o.title = val(),
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ if input.is_none() => input = Some(a),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(input) = input else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let mut raw = Vec::new();
    let read = if input == "-" {
        std::io::stdin().read_to_end(&mut raw)
    } else {
        std::fs::File::open(&input).and_then(|mut f| f.read_to_end(&mut raw))
    };
    if let Err(e) = read {
        eprintln!("{input}: {e}");
        return ExitCode::FAILURE;
    }
    let folded = if looks_like_perf_script(&raw) { fold_perf_script(&raw) } else { raw };
    let mut stacks = parse_folded(&folded, o.thread.as_deref(), o.keep_plumbing);
    if let Some(f) = &o.focus {
        stacks.retain(|(frames, _)| frames.iter().any(|x| x.contains(f.as_str())));
    }
    if stacks.is_empty() {
        eprintln!("{input}: no stacks");
        return ExitCode::FAILURE;
    }
    match run(&o, &stacks) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// `perf script` prints a header line per sample and the frames indented
/// below it; folded stacks have neither.
fn looks_like_perf_script(raw: &[u8]) -> bool {
    BufReader::new(raw)
        .lines()
        .take(200)
        .map_while(Result::ok)
        .any(|l| {
            let t = l.trim_start();
            l.len() != t.len() && t.split(' ').next().is_some_and(|w| w.len() >= 4 && w.chars().all(|c| c.is_ascii_hexdigit()))
        })
}

/// `perf script` output as folded stacks (`thread;outer;..;leaf 1` per
/// sample, summed): a sample's header line names its thread (`comm`, which
/// may hold spaces) before `pid/tid time:`, its frames follow leaf first,
/// one per line (`addr symbol+0xoff (object)`), up to a blank line.
fn fold_perf_script(raw: &[u8]) -> Vec<u8> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut thread: Option<String> = None;
    let mut frames: Vec<String> = Vec::new();
    let mut flush = |thread: &mut Option<String>, frames: &mut Vec<String>| {
        if let Some(t) = thread.take() {
            let mut key = t;
            for f in frames.iter().rev() {
                key.push(';');
                key.push_str(f);
            }
            *counts.entry(key).or_default() += 1;
        }
        frames.clear();
    };
    for line in BufReader::new(raw).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            flush(&mut thread, &mut frames);
            continue;
        }
        if !line.starts_with(|c: char| c.is_whitespace()) {
            flush(&mut thread, &mut frames);
            let words: Vec<&str> = line.split_whitespace().collect();
            let pid = words.windows(2).position(|w| {
                w[0].split('/').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
                    && w[1].ends_with(':')
                    && w[1].contains('.')
            });
            thread = Some(match pid {
                Some(i) if i > 0 => words[..i].join(" "),
                _ => words.first().copied().unwrap_or("?").to_string(),
            });
            continue;
        }
        // `    55d5c0ffee symbol+0x1f (/path/object)`
        let t = line.trim();
        let rest = t.split_once(' ').map_or("", |(_, r)| r);
        let sym = match rest.rfind(" (") {
            Some(i) => &rest[..i],
            None => rest,
        };
        let sym = match sym.rfind("+0x") {
            Some(i) => &sym[..i],
            None => sym,
        };
        let sym = if sym.is_empty() || sym == "[unknown]" {
            // An address without a symbol: its object's file name.
            rest.rsplit_once(" (")
                .map(|(_, o)| o.trim_end_matches(')').rsplit(['/', '\\']).next().unwrap_or("?").to_string())
                .map_or("[unknown]".to_string(), |o| format!("[{o}]"))
        } else {
            sym.replace(';', ",")
        };
        frames.push(sym);
    }
    flush(&mut thread, &mut frames);
    let mut out = String::new();
    for (k, n) in counts {
        let _ = writeln!(out, "{k} {n}");
    }
    out.into_bytes()
}

/// Folded lines as (frames, count), names shortened, threads grouped.
fn parse_folded(raw: &[u8], thread: Option<&str>, keep_plumbing: bool) -> Vec<(Vec<String>, f64)> {
    let mut out: HashMap<Vec<String>, f64> = HashMap::new();
    for line in BufReader::new(raw).lines().map_while(Result::ok) {
        let Some((stack, n)) = line.rsplit_once(' ') else { continue };
        let Ok(n) = n.trim().parse::<f64>() else { continue };
        let mut frames: Vec<String> = stack.split(';').map(short_name).collect();
        if frames.is_empty() {
            continue;
        }
        frames[0] = thread_group(&frames[0]);
        if thread.is_some_and(|t| !frames[0].contains(t)) {
            continue;
        }
        // Drop the plumbing (thread start, the event loop, executors and
        // task wrappers) between the thread and the code it runs, so a
        // stack reads thread, system, what the system calls; the leaf stays.
        let leaf = frames.len() - 1;
        let mut i = 0;
        frames.retain(|f| {
            i += 1;
            !f.is_empty() && (i == 1 || i - 1 == leaf || keep_plumbing || !is_plumbing(f))
        });
        *out.entry(frames).or_default() += n;
    }
    out.into_iter().collect()
}

/// Frames that only pass control on: the runtime's start, the window's
/// event loop, Bevy's app, schedule and executor wrappers, the task pools'
/// futures. (Not systems: those are what the reader is after.)
fn is_plumbing(f: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "std::rt::",
        "std::sys::",
        "std::thread::",
        "std::panic",
        "std::panicking",
        "core::panic::",
        "core::ops::function::",
        "_start",
        "__libc_start",
        "start_thread",
        "clone3",
        "__clone",
        "[libc.so.6]",
        "winit::",
        "bevy_winit::",
        "bevy_app::app::App::run",
        "chimera_client::client::app::run",
        "bevy_app::sub_app::",
        "bevy_ecs::schedule::executor::",
        "bevy_ecs::world::World::schedule_scope",
        "bevy_ecs::world::World::try_schedule_scope",
        "bevy_ecs::world::World::resource_scope",
        "bevy_ecs::world::World::run_schedule",
        "bevy_ecs::system::function_system::FunctionSystem::run",
        "bevy_ecs::system::exclusive_function_system::ExclusiveFunctionSystem::run",
        "bevy_ecs::system::system::System::run",
        "bevy_tasks::",
        "async_task::",
        "async_executor::",
        "futures_lite::",
        "futures_util::",
        "event_listener::",
    ];
    f == "main" || f.ends_with("::main") || PREFIXES.iter().any(|p| f.starts_with(p))
}

/// A thread's name without its index: the task pools' threads as one.
fn thread_group(t: &str) -> String {
    let t = t.trim();
    let t = t.split(" (").next().unwrap_or(t);
    let t = t.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '/');
    if t.starts_with("Compute") || t.starts_with("Async") || t.starts_with("IO Task") {
        return t.trim_end().to_string();
    }
    t.to_string()
}

/// A Rust symbol without its hash and generic arguments, and with
/// `<T as Trait>::f` as `T::f`: what the reader needs to find the code.
fn short_name(name: &str) -> String {
    let name = name.trim();
    let name = strip_hash(name);
    let mut out = String::with_capacity(name.len());
    let chars: Vec<char> = name.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            let prev = out.chars().last();
            let qualified = prev.is_none_or(|p| p == ':' || p == ' ' || p == '(' || p == ',');
            // The matching '>'.
            let mut depth = 0;
            let mut j = i;
            while j < chars.len() {
                match chars[j] {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if qualified {
                // `<A as B>` -> `A`; `<A>` -> `A`.
                let inner: String = chars[i + 1..j.min(chars.len())].iter().collect();
                let inner = inner.split(" as ").next().unwrap_or(&inner).to_string();
                let inner = short_name(&inner);
                if out.ends_with("::") && prev == Some(':') {
                    // `foo::<T>`: a turbofish, dropped.
                    out.truncate(out.len() - 2);
                } else {
                    out.push_str(&inner);
                }
            }
            i = j + 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out.replace("{{closure}}", "{closure}")
}

fn strip_hash(name: &str) -> &str {
    if let Some(i) = name.rfind("::h") {
        let h = &name[i + 3..];
        if h.len() == 16 && h.chars().all(|c| c.is_ascii_hexdigit()) {
            return &name[..i];
        }
    }
    name
}

fn crate_of(frame: &str) -> &str {
    let f = frame.trim_start_matches("&mut ").trim_start_matches('&').trim_start_matches('<');
    f.split("::").next().unwrap_or(f)
}

fn is_ours(frame: &str, ours: &[String]) -> bool {
    let c = crate_of(frame);
    ours.iter().any(|p| c.starts_with(p.as_str()))
}

struct Tables {
    ours: Vec<(String, f64)>,
    selfs: Vec<(String, f64)>,
    totals: Vec<(String, f64)>,
    threads: Vec<(String, f64)>,
    /// `thread|family` (ours, Bevy, wgpu and the driver, ...) by owner.
    families: Vec<(String, f64)>,
    total: f64,
}

/// The families of `families.svg`, in drawing order, with their colours.
const FAMILIES: &[(&str, &str)] = &[
    ("our code", "#3a78c3"),
    ("Bevy", "#e8963c"),
    ("wgpu and the driver", "#5aaa64"),
    ("audio", "#9a6fc0"),
    ("allocator and memcpy (callers unknown)", "#a8a8a8"),
    ("other and unsymbolized", "#d8d0c0"),
];

/// A sample's family by its owner (`ours` table row).
fn family_of(owner: &str, ours: &[String]) -> &'static str {
    let o = owner.trim_start_matches("(not ours) ");
    let c = crate_of(o);
    if !owner.starts_with('(') && !owner.starts_with('[') && is_ours(owner, ours) {
        FAMILIES[0].0
    } else if c.starts_with("bevy") {
        FAMILIES[1].0
    } else if c.starts_with("wgpu") || c.starts_with("naga") || c.starts_with("ash") || o.contains("libnvidia") || o.contains("vulkan") {
        FAMILIES[2].0
    } else if c.starts_with("cpal") || c.starts_with("rodio") || c.starts_with("alsa") {
        FAMILIES[3].0
    } else if o.contains("libc.so") || ["malloc", "cfree", "realloc", "free", "__libc", "memcpy", "memmove"].iter().any(|p| c.starts_with(p)) {
        FAMILIES[4].0
    } else {
        FAMILIES[5].0
    }
}

fn tables(o: &Opts, stacks: &[(Vec<String>, f64)]) -> Tables {
    let mut ours: HashMap<String, f64> = HashMap::new();
    let mut selfs: HashMap<String, f64> = HashMap::new();
    let mut totals: HashMap<String, f64> = HashMap::new();
    let mut threads: HashMap<String, f64> = HashMap::new();
    let mut families: HashMap<String, f64> = HashMap::new();
    let mut total = 0.0;
    for (frames, n) in stacks {
        let thread = &frames[0];
        total += n;
        *threads.entry(thread.clone()).or_default() += n;
        let key = |f: &str| format!("{f}  [{thread}]");
        let leaf = frames.last().map(String::as_str).unwrap_or("");
        *selfs.entry(key(leaf)).or_default() += n;
        let mut seen: Vec<&str> = Vec::new();
        for f in &frames[1..] {
            if !seen.contains(&f.as_str()) {
                seen.push(f);
                *totals.entry(key(f)).or_default() += n;
            }
        }
        // Span stacks (tracesum) are all ours: their leaf is the system.
        let owner = if o.unit_us {
            leaf.to_string()
        } else {
            frames[1..]
                .iter()
                .rev()
                .find(|f| is_ours(f, &o.ours))
                .cloned()
                .unwrap_or_else(|| match frames.get(1) {
                    // Only the thread and a frame without a symbol: the
                    // stack couldn't be walked (no frame pointers there).
                    Some(f) if frames.len() == 2 && f.starts_with('[') => format!("{f} (callers unknown)"),
                    Some(_) => format!("(not ours) {}", outermost_known(&frames[1..])),
                    None => "(no frames)".into(),
                })
        };
        *families.entry(format!("{thread}|{}", family_of(&owner, &o.ours))).or_default() += n;
        *ours.entry(key(&owner)).or_default() += n;
    }
    let sorted = |m: HashMap<String, f64>| {
        let mut v: Vec<_> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    };
    Tables {
        ours: sorted(ours),
        selfs: sorted(selfs),
        totals: sorted(totals),
        threads: sorted(threads),
        families: sorted(families),
        total,
    }
}

/// The first frame below the thread's start-up that names a crate (not
/// `std`, `core` or an address), for samples outside our code.
fn outermost_known(frames: &[String]) -> String {
    const SKIP: &[&str] = &["std", "core", "alloc", "[unknown]", "_start", "__libc_start", "start_thread", "clone3", "__clone"];
    frames
        .iter()
        .find(|f| !SKIP.iter().any(|s| crate_of(f).starts_with(s)) && !f.starts_with("0x"))
        .or(frames.last())
        .cloned()
        .unwrap_or_default()
}

fn run(o: &Opts, stacks: &[(Vec<String>, f64)]) -> Result<(), String> {
    let t = tables(o, stacks);
    // A count in ms per frame (or ms in all, without --frames).
    let to_ms = |n: f64| {
        let ms = if o.unit_us { n / 1e3 } else { n / o.freq * 1e3 };
        ms / o.frames.unwrap_or(1.0)
    };
    let unit = if o.frames.is_some() { "ms/frame" } else { "ms" };
    let baseline = match &o.baseline {
        Some(p) => Some(read_baseline(p)?),
        None => None,
    };
    let mut md = String::new();
    let _ = writeln!(md, "# {}\n", o.title);
    let _ = writeln!(
        md,
        "{} {} in all{}\n",
        fmt_n(t.total),
        if o.unit_us { "µs of spans" } else { "samples" },
        o.frames.map_or(String::new(), |f| format!(" over {f} frames: {:.3} ms of CPU a frame", to_ms(t.total)))
    );
    let _ = writeln!(md, "## Threads\n\n| thread | {unit} | % |\n|---|---|---|");
    for (name, n) in &t.threads {
        let _ = writeln!(md, "| {name} | {:.3} | {:.1}% |", to_ms(*n), 100.0 * n / t.total);
    }
    let sections = [
        ("Our code (deepest frame of ours, callees included)", "ours", &t.ours),
        ("Functions by self time (leaf frame)", "self", &t.selfs),
        ("Functions by total time (anywhere on the stack)", "total", &t.totals),
    ];
    for (title, key, rows) in sections {
        let _ = writeln!(md, "\n## {title}\n");
        let base = baseline.as_ref().and_then(|b| b.get(key));
        if base.is_some() {
            let _ = writeln!(md, "| # | before {unit} | after {unit} | delta | % | name [thread] |\n|---|---|---|---|---|---|");
        } else {
            let _ = writeln!(md, "| # | {unit} | % | name [thread] |\n|---|---|---|---|");
        }
        for (i, (name, n)) in rows.iter().take(o.top).enumerate() {
            let ms = to_ms(*n);
            let pct = 100.0 * n / t.total;
            match base {
                Some(b) => {
                    let before = b.get(name).map(|r| r.ms);
                    let _ = writeln!(
                        md,
                        "| {} | {} | {ms:.3} | {} | {pct:.1}% | `{}` |",
                        i + 1,
                        before.map_or("-".into(), |v| format!("{v:.3}")),
                        before.map_or("new".into(), |v| format!("{:+.3}", ms - v)),
                        name.replace('|', "\\|")
                    );
                }
                None => {
                    let _ = writeln!(md, "| {} | {ms:.3} | {pct:.1}% | `{}` |", i + 1, name.replace('|', "\\|"));
                }
            }
        }
        if let Some(b) = base {
            // What dropped out of the top N since the baseline.
            let now: HashMap<&str, f64> = rows.iter().map(|(n, v)| (n.as_str(), *v)).collect();
            let mut gone: Vec<_> = b
                .iter()
                .filter(|(n, r)| r.ms >= to_ms(rows.get(o.top.saturating_sub(1)).map_or(0.0, |r| r.1)) && !now.contains_key(n.as_str()))
                .collect();
            gone.sort_by(|a, b| b.1.ms.total_cmp(&a.1.ms));
            for (name, r) in gone.iter().take(5) {
                let _ = writeln!(md, "| - | {:.3} | 0 | {:+.3} | gone | `{}` |", r.ms, -r.ms, name.replace('|', "\\|"));
            }
        }
    }
    print!("{md}");
    let Some(dir) = &o.out else { return Ok(()) };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let write = |name: &str, bytes: &[u8]| {
        let p = dir.join(name);
        std::fs::write(&p, bytes).map_err(|e| format!("{}: {e}", p.display()))
    };
    write("table.md", md.as_bytes())?;
    // top.json: every row of each table, ms in the chosen unit.
    let mut json = serde_json::Map::new();
    json.insert("unit".into(), unit.into());
    json.insert("frames".into(), o.frames.into());
    json.insert("total_ms".into(), to_ms(t.total).into());
    for (key, rows) in [
        ("ours", &t.ours),
        ("self", &t.selfs),
        ("total", &t.totals),
        ("threads", &t.threads),
        ("families", &t.families),
    ] {
        let m: serde_json::Map<_, _> = rows.iter().map(|(n, v)| (n.clone(), serde_json::json!(to_ms(*v)))).collect();
        json.insert(key.into(), m.into());
    }
    write("top.json", serde_json::to_string_pretty(&json).unwrap_or_default().as_bytes())?;
    // The flame graph: shortened names, one tower per thread.
    let subtitle = match o.frames {
        Some(f) => format!("{:.3} ms of CPU a frame over {f} frames; width = share of the time", to_ms(t.total)),
        None => "width = share of the time".into(),
    };
    let svg = flame_svg(&o.title, &subtitle, stacks, &o.ours, |n| format!("{:.3} {unit}, {:.1}%", to_ms(n), 100.0 * n / t.total));
    write("flame.svg", svg.as_bytes())?;
    // With a baseline: the rows largest before or after, so what a fix
    // removed shows too (a grey bar with no blue one).
    let before_ours = baseline.as_ref().and_then(|b| b.get("ours"));
    let mut bars: Vec<(String, f64, Option<f64>)> = t
        .ours
        .iter()
        .map(|(n, v)| (n.clone(), to_ms(*v), before_ours.map(|b| b.get(n).map_or(0.0, |r| r.ms))))
        .collect();
    if let Some(b) = before_ours {
        for (n, r) in b {
            if !bars.iter().any(|(m, ..)| m == n) {
                bars.push((n.clone(), 0.0, Some(r.ms)));
            }
        }
    }
    bars.sort_by(|a, b| b.1.max(b.2.unwrap_or(0.0)).total_cmp(&a.1.max(a.2.unwrap_or(0.0))));
    bars.truncate(o.top.min(20));
    write("top.svg", bars_svg(&o.title, unit, &bars).as_bytes())?;
    let fam_now: HashMap<String, f64> = t.families.iter().map(|(k, v)| (k.clone(), to_ms(*v))).collect();
    let fam_before: Option<HashMap<String, f64>> = baseline
        .as_ref()
        .and_then(|b| b.get("families"))
        .map(|m| m.iter().map(|(k, r)| (k.clone(), r.ms)).collect());
    let threads: Vec<String> = t.threads.iter().filter(|(_, n)| 100.0 * n / t.total >= 1.0).map(|(k, _)| k.clone()).collect();
    write("families.svg", families_svg(&o.title, unit, &threads, &fam_now, fam_before.as_ref()).as_bytes())?;
    println!("\nwrote {}: flame.svg, top.svg, families.svg, top.json, table.md", dir.display());
    Ok(())
}

type Baseline = HashMap<String, HashMap<String, Row>>;

fn read_baseline(p: &PathBuf) -> Result<Baseline, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut out = Baseline::new();
    for key in ["ours", "self", "total", "threads", "families"] {
        let rows = v[key]
            .as_object()
            .map(|m| m.iter().map(|(n, ms)| (n.clone(), Row { ms: ms.as_f64().unwrap_or(0.0) })).collect())
            .unwrap_or_default();
        out.insert(key.into(), rows);
    }
    Ok(out)
}

fn fmt_n(n: f64) -> String {
    if n >= 1e6 {
        format!("{:.2}M", n / 1e6)
    } else if n >= 1e3 {
        format!("{:.1}k", n / 1e3)
    } else {
        format!("{n:.0}")
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[derive(Default)]
struct Node {
    n: f64,
    children: Vec<(String, Node)>,
}

impl Node {
    fn add(&mut self, frames: &[String], n: f64) {
        self.n += n;
        if let Some((first, rest)) = frames.split_first() {
            let i = match self.children.iter().position(|(name, _)| name == first) {
                Some(i) => i,
                None => {
                    self.children.push((first.clone(), Node::default()));
                    self.children.len() - 1
                }
            };
            self.children[i].1.add(rest, n);
        }
    }

    fn depth(&self) -> usize {
        1 + self.children.iter().map(|(_, c)| c.depth()).max().unwrap_or(0)
    }
}

/// A frame's colour by its crate family (and a little hash so neighbours
/// differ).
fn frame_colour(name: &str, ours: &[String]) -> String {
    let c = crate_of(name.trim_start_matches('<'));
    let h = name.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let v = (h % 40) as f64 / 40.0;
    let (r, g, b) = if is_ours(name, ours) {
        (70.0 + 40.0 * v, 130.0 + 40.0 * v, 220.0)
    } else if c.starts_with("bevy") {
        (235.0, 150.0 + 50.0 * v, 60.0)
    } else if c.starts_with("wgpu") || c.starts_with("naga") || c.starts_with("ash") || c.starts_with('[') && name.contains(".so") {
        (90.0 + 40.0 * v, 190.0, 100.0)
    } else if ["std", "core", "alloc", "hashbrown", "__", "_int_", "malloc", "free", "mem", "cfree", "realloc"]
        .iter()
        .any(|p| c.starts_with(p))
    {
        (170.0 + 30.0 * v, 170.0 + 30.0 * v, 170.0 + 30.0 * v)
    } else {
        (220.0, 100.0 + 60.0 * v, 90.0)
    };
    format!("rgb({r:.0},{g:.0},{b:.0})")
}

/// A flame graph: the root at the bottom, each thread a tower, callees
/// above their callers, width = time.
fn flame_svg(title: &str, subtitle: &str, stacks: &[(Vec<String>, f64)], ours: &[String], label: impl Fn(f64) -> String) -> String {
    let mut root = Node::default();
    let mut sorted: Vec<_> = stacks.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (frames, n) in sorted {
        root.add(frames, *n);
    }
    // Wider towers first.
    fn sort(n: &mut Node) {
        n.children.sort_by(|a, b| b.1.n.total_cmp(&a.1.n));
        for (_, c) in &mut n.children {
            sort(c);
        }
    }
    sort(&mut root);
    let w = 1800.0;
    let fh = 17.0;
    let top = 70.0;
    let depth = root.depth().min(80);
    let h = top + fh * depth as f64 + 20.0;
    let mut s = String::new();
    let _ = writeln!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="DejaVu Sans Mono, Consolas, monospace" font-size="11">"##
    );
    let _ = writeln!(s, r##"<rect width="100%" height="100%" fill="#fbfaf7"/>"##);
    let _ = writeln!(s, r##"<text x="10" y="26" font-family="DejaVu Sans, Verdana, sans-serif" font-size="18" font-weight="bold" fill="#1a1a1a">{}</text>"##, esc(title));
    let _ = writeln!(
        s,
        r##"<text x="10" y="46" font-family="DejaVu Sans, Verdana, sans-serif" font-size="13" fill="#555">{} (blue: our code, orange: Bevy, green: wgpu and the driver, grey: std and the allocator)</text>"##,
        esc(subtitle)
    );
    struct Ctx<'a, F: Fn(f64) -> String> {
        s: &'a mut String,
        scale: f64,
        base_y: f64,
        fh: f64,
        max_depth: usize,
        ours: &'a [String],
        label: F,
    }
    fn draw<F: Fn(f64) -> String>(c: &mut Ctx<'_, F>, name: &str, node: &Node, x: f64, d: usize) {
        let width = node.n * c.scale;
        if width < 0.4 || d >= c.max_depth {
            return;
        }
        let y = c.base_y - c.fh * (d as f64 + 1.0);
        let colour = if d == 0 { "rgb(200,200,200)".to_string() } else { frame_colour(name, c.ours) };
        let _ = write!(
            c.s,
            r##"<g><title>{} ({})</title><rect x="{x:.1}" y="{y:.1}" width="{:.1}" height="{:.1}" fill="{colour}" rx="1"/>"##,
            esc(name),
            esc(&(c.label)(node.n)),
            width.max(0.5) - 0.3,
            c.fh - 1.0
        );
        let chars = ((width - 6.0) / 6.7) as usize;
        if chars >= 3 {
            let text: String = if name.chars().count() > chars {
                format!("{}…", name.chars().take(chars.saturating_sub(1)).collect::<String>())
            } else {
                name.to_string()
            };
            let _ = write!(c.s, r##"<text x="{:.1}" y="{:.1}" fill="#111">{}</text>"##, x + 3.0, y + c.fh - 5.0, esc(&text));
        }
        c.s.push_str("</g>\n");
        let mut cx = x;
        for (n, child) in &node.children {
            draw(c, n, child, cx, d + 1);
            cx += child.n * c.scale;
        }
    }
    let mut c = Ctx {
        s: &mut s,
        scale: (w - 20.0) / root.n.max(1e-9),
        base_y: h - 10.0,
        fh,
        max_depth: depth,
        ours,
        label,
    };
    draw(&mut c, "all", &root, 10.0, 0);
    s.push_str("</svg>\n");
    s
}

/// Each thread's ms per frame as one stacked bar by family (with a
/// baseline: before above after).
fn families_svg(
    title: &str,
    unit: &str,
    threads: &[String],
    now: &HashMap<String, f64>,
    before: Option<&HashMap<String, f64>>,
) -> String {
    let rows: Vec<(String, &HashMap<String, f64>)> = threads
        .iter()
        .flat_map(|t| {
            let mut v = Vec::new();
            if let Some(b) = before {
                v.push((format!("{t} (before)"), b));
                v.push((format!("{t} (after)"), now));
            } else {
                v.push((t.clone(), now));
            }
            v
        })
        .collect();
    let thread_of = |label: &str| label.trim_end_matches(" (before)").trim_end_matches(" (after)").to_string();
    let sum = |t: &str, m: &HashMap<String, f64>| FAMILIES.iter().map(|(f, _)| m.get(&format!("{t}|{f}")).copied().unwrap_or(0.0)).sum::<f64>();
    let max = rows.iter().map(|(l, m)| sum(&thread_of(l), m)).fold(0.0f64, f64::max).max(1e-9);
    let (label_w, bar_w, row_h, top) = (230.0, 820.0, 30.0, 92.0);
    let w = label_w + bar_w + 110.0;
    let h = top + row_h * rows.len() as f64 + 30.0;
    let mut s = String::new();
    let _ = writeln!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="DejaVu Sans, Verdana, sans-serif" font-size="13">"##
    );
    let _ = writeln!(s, r##"<rect width="100%" height="100%" fill="#ffffff"/>"##);
    let _ = writeln!(s, r##"<text x="16" y="28" font-size="18" font-weight="bold" fill="#1a1a1a">{}</text>"##, esc(title));
    let _ = writeln!(s, r##"<text x="16" y="48" fill="#555">CPU {} per thread, by whose code it runs</text>"##, esc(unit));
    let mut lx = 16.0;
    for (f, c) in FAMILIES {
        let _ = writeln!(s, r##"<rect x="{lx}" y="60" width="12" height="12" fill="{c}"/><text x="{}" y="71" fill="#333">{}</text>"##, lx + 16.0, esc(f));
        lx += 30.0 + 7.2 * f.len() as f64;
    }
    for (i, (label, m)) in rows.iter().enumerate() {
        let y = top + row_h * i as f64;
        let t = thread_of(label);
        let _ = writeln!(s, r##"<text x="{}" y="{}" text-anchor="end" fill="#1a1a1a">{}</text>"##, label_w - 8.0, y + 17.0, esc(label));
        let mut x = label_w;
        for (f, c) in FAMILIES {
            let v = m.get(&format!("{t}|{f}")).copied().unwrap_or(0.0);
            let bw = bar_w * v / max;
            if bw > 0.2 {
                let _ = writeln!(s, r##"<g><title>{}: {v:.2}</title><rect x="{x:.1}" y="{}" width="{bw:.1}" height="{}" fill="{c}"/></g>"##, esc(f), y + 3.0, row_h - 8.0);
                if bw > 34.0 {
                    let _ = writeln!(s, r##"<text x="{:.1}" y="{}" fill="#fff" font-size="11">{v:.2}</text>"##, x + 4.0, y + 17.0);
                }
            }
            x += bw;
        }
        let _ = writeln!(s, r##"<text x="{:.1}" y="{}" fill="#1a1a1a">{:.2}</text>"##, x + 6.0, y + 17.0, sum(&t, m));
    }
    s.push_str("</svg>\n");
    s
}

/// Horizontal bars of the top rows (after in blue, before in grey above
/// it when there is a baseline).
fn bars_svg(title: &str, unit: &str, rows: &[(String, f64, Option<f64>)]) -> String {
    let compare = rows.iter().any(|r| r.2.is_some());
    let label_w = 620.0;
    let bar_w = 520.0;
    let row_h = if compare { 34.0 } else { 24.0 };
    let top = 64.0;
    let w = label_w + bar_w + 120.0;
    let h = top + row_h * rows.len() as f64 + 40.0;
    let max = rows
        .iter()
        .map(|r| r.1.max(r.2.unwrap_or(0.0)))
        .fold(0.0f64, f64::max)
        .max(1e-9);
    let mut s = String::new();
    let _ = writeln!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="DejaVu Sans, Verdana, sans-serif" font-size="13">"##
    );
    let _ = writeln!(s, r##"<rect width="100%" height="100%" fill="#ffffff"/>"##);
    let _ = writeln!(s, r##"<text x="16" y="28" font-size="18" font-weight="bold" fill="#1a1a1a">{}</text>"##, esc(title));
    let legend = if compare {
        format!("{unit}, our code (deepest frame of ours); grey: before, blue: after")
    } else {
        format!("{unit}, our code (deepest frame of ours, callees included)")
    };
    let _ = writeln!(s, r##"<text x="16" y="48" fill="#555">{}</text>"##, esc(&legend));
    for (i, (name, after, before)) in rows.iter().enumerate() {
        let y = top + row_h * i as f64;
        let mut label = name.clone();
        if label.chars().count() > 86 {
            label = format!("…{}", label.chars().skip(label.chars().count() - 85).collect::<String>());
        }
        let _ = writeln!(
            s,
            r##"<text x="{}" y="{}" text-anchor="end" fill="#1a1a1a">{}</text>"##,
            label_w - 8.0,
            y + row_h / 2.0 + 4.0,
            esc(&label)
        );
        let mut by = y + 3.0;
        let bh = if compare { (row_h - 8.0) / 2.0 } else { row_h - 6.0 };
        if compare {
            let b = before.unwrap_or(0.0);
            let _ = writeln!(
                s,
                r##"<rect x="{label_w}" y="{by}" width="{:.1}" height="{bh}" fill="#b8b8b8"/>"##,
                bar_w * b / max
            );
            by += bh + 1.0;
        }
        let _ = writeln!(
            s,
            r##"<rect x="{label_w}" y="{by}" width="{:.1}" height="{bh}" fill="#3a78c3"/>"##,
            bar_w * after / max
        );
        let text = match before {
            Some(b) if compare && *after == 0.0 => format!("0 (was {b:.2})"),
            Some(b) if compare => format!("{after:.2} ({:+.2})", after - b),
            None if compare => format!("{after:.2} (new)"),
            _ => format!("{after:.2}"),
        };
        let _ = writeln!(
            s,
            r##"<text x="{}" y="{}" fill="#1a1a1a">{}</text>"##,
            label_w + bar_w * after.max(before.unwrap_or(0.0)) / max + 6.0,
            y + row_h / 2.0 + 4.0,
            esc(&text)
        );
    }
    s.push_str("</svg>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lose_hashes_and_generics() {
        assert_eq!(
            short_name("<bevy_ecs::system::function_system::FunctionSystem<Marker,F> as bevy_ecs::system::system::System>::run_unsafe::h0123456789abcdef"),
            "bevy_ecs::system::function_system::FunctionSystem::run_unsafe"
        );
        assert_eq!(short_name("hashbrown::map::HashMap<K,V,S,A>::insert"), "hashbrown::map::HashMap::insert");
        assert_eq!(short_name("core::ptr::drop_in_place<alloc::string::String>"), "core::ptr::drop_in_place");
        assert_eq!(short_name("a::b::{{closure}}"), "a::b::{closure}");
        assert_eq!(thread_group("Compute Task Pool (3)"), "Compute Task Pool");
        assert_eq!(thread_group("lucker_party"), "lucker_party");
        assert!(is_ours("&mut chimera_client::console::overlay_text::call_mut", &["chimera".into()]));
    }

    #[test]
    fn perf_script_folds_by_thread_leaf_last() {
        let script = "Compute Task Pool (1) 12/34 100.5:     1001 cpu-clock:u: \n\
            \t    55d5c0ffee memcpy+0x10 (/usr/lib/libc.so.6)\n\
            \t    55d5c0ff00 chimera_x::sys+0x1f (/bin/game)\n\
            \t    55d5c0ff01 [unknown] (/bin/game)\n\n\
            main 12/12 100.6:     1001 cpu-clock:u: \n\
            \t    55d5c0ff00 chimera_x::sys+0x1f (/bin/game)\n\n";
        assert!(looks_like_perf_script(script.as_bytes()));
        let folded = String::from_utf8(fold_perf_script(script.as_bytes())).unwrap();
        let mut lines: Vec<_> = folded.lines().collect();
        lines.sort();
        assert_eq!(lines, ["Compute Task Pool (1);[game];chimera_x::sys;memcpy 1", "main;chimera_x::sys 1"]);
        let stacks = parse_folded(folded.as_bytes(), None, false);
        assert!(stacks.iter().any(|(f, _)| f[0] == "Compute Task Pool"));
        let svg = flame_svg("t", "s", &stacks, &["chimera".into()], |n| format!("{n}"));
        assert!(svg.contains("chimera_x::sys"));
    }

    #[test]
    fn samples_charge_the_deepest_frame_of_ours() {
        let o = Opts {
            frames: Some(10.0),
            freq: 1000.0,
            unit_us: false,
            top: 10,
            ours: vec!["chimera".into()],
            thread: None,
            focus: None,
            keep_plumbing: false,
            out: None,
            baseline: None,
            title: String::new(),
        };
        let stacks = vec![
            (vec!["main".into(), "bevy_app::run".into(), "chimera_x::sys".into(), "memcpy".into()], 30.0),
            (vec!["main".into(), "bevy_app::run".into(), "bevy_render::other".into()], 10.0),
        ];
        let t = tables(&o, &stacks);
        assert_eq!(t.ours[0], ("chimera_x::sys  [main]".to_string(), 30.0));
        assert_eq!(t.selfs[0].0, "memcpy  [main]");
        assert!(t.ours.iter().any(|(n, _)| n.starts_with("(not ours) bevy_app::run")));
        assert_eq!(t.total, 40.0);
    }
}
