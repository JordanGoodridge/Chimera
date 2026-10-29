//! Development tool: summarize a Chrome trace written by a
//! `--features profile` build (Bevy's `trace_chrome`; `TRACE_CHROME=<file>`
//! names the file). Prints, per top-level span (the main world's `frame`,
//! the render world's, ...), its count and average time, and the spans
//! with the most time per frame, total and self (without their children),
//! with their p99 over frames and share of the main world's frame.
//! `--folded <file>` also writes every span stack with its self time (µs),
//! one line per stack, for `profsum` (flame graph, top-N table, before and
//! after). See docs/performance.md ("Measuring").

use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    process::ExitCode,
};

const USAGE: &str = "\
usage: tracesum <trace.json> [options]
  --top <n>        spans to list (default 40)
  --skip <n>       ignore the first <n> frames (loading; default 120)
  --filter <text>  only spans whose name contains <text>
  --by-total       sort by total time instead of self time
  --folded <file>  write the span stacks with their self time in µs
                   (`thread;outer;..;span N` lines) for `profsum`";

#[derive(Default, Clone)]
struct Stat {
    count: u64,
    total_us: f64,
    self_us: f64,
    /// Self µs per counted frame (index: frame after the skipped ones).
    per_frame: Vec<f64>,
}

impl Stat {
    fn add_frame(&mut self, frame: usize, self_us: f64) {
        if self.per_frame.len() <= frame {
            self.per_frame.resize(frame + 1, 0.0);
        }
        self.per_frame[frame] += self_us;
    }

    /// The 99th percentile of self µs per frame over `frames` frames (a
    /// frame without the span counts as 0).
    fn p99(&self, frames: usize) -> f64 {
        let mut v = self.per_frame.clone();
        v.resize(frames.max(v.len()), 0.0);
        v.sort_by(f64::total_cmp);
        v.get(((v.len() as f64 * 0.99) as usize).min(v.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0)
    }
}

/// A span's label in the folded stacks: a system's or schedule's name
/// without the `system: name=` around it.
fn short_label(name: &str) -> String {
    let unquote = |v: &str| v.trim().trim_matches('"').to_string();
    if let Some(v) = name.strip_prefix("system: name=") {
        return unquote(v);
    }
    if let Some(v) = name.strip_prefix("schedule: name=") {
        return format!("schedule {}", unquote(v));
    }
    name.replace(';', ",")
}

fn thread_group(thread: &str) -> String {
    if thread.starts_with("Compute") || thread.starts_with("Async") || thread.starts_with("IO") {
        // "Compute Task Pool (3)" -> one group for the pool.
        thread.split(" (").next().unwrap_or(thread).to_string()
    } else {
        thread.to_string()
    }
}

struct Open {
    name: String,
    start: f64,
    child_us: f64,
    root: String,
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut path, mut top, mut skip, mut filter, mut by_total) = (None, 40usize, 120u64, None, false);
    let mut folded_out: Option<String> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--top" => top = args.next().and_then(|v| v.parse().ok()).unwrap_or(top),
            "--skip" => skip = args.next().and_then(|v| v.parse().ok()).unwrap_or(skip),
            "--filter" => filter = args.next(),
            "--by-total" => by_total = true,
            "--folded" => folded_out = args.next(),
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ if path.is_none() => path = Some(a),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(path) = path else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    // tracing-chrome writes one event per line ("{...},"), so a trace cut
    // short (a killed run) still reads.
    let mut stacks: HashMap<u64, Vec<Open>> = HashMap::new();
    let mut threads: HashMap<u64, String> = HashMap::new();
    let mut stats: HashMap<(String, String), Stat> = HashMap::new();
    let mut roots: HashMap<String, Stat> = HashMap::new();
    let mut frames = 0u64;
    let mut folded: HashMap<String, f64> = HashMap::new();
    // Read every event, then go through them in time order: Bevy's
    // `trace_chrome` writes them as they happen, `trace_capture` thread by
    // thread, and frames (for --skip and p99) are counted by the main
    // world's updates in time.
    let mut names: HashMap<String, std::sync::Arc<str>> = HashMap::new();
    let mut events: Vec<(f64, bool, u64, std::sync::Arc<str>)> = Vec::new();
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { break };
        let l = line
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_end_matches(',');
        if l.is_empty() {
            continue;
        }
        let Ok(e) = serde_json::from_str::<serde_json::Value>(l) else {
            continue;
        };
        let ph = e["ph"].as_str().unwrap_or("");
        let tid = e["tid"].as_u64().unwrap_or(0);
        let name = e["name"].as_str().unwrap_or("");
        if ph == "M" {
            threads.insert(tid, e["args"]["name"].as_str().unwrap_or("").to_string());
            continue;
        }
        if ph != "B" && ph != "E" {
            continue;
        }
        let name = match names.get(name) {
            Some(n) => n.clone(),
            None => {
                let n: std::sync::Arc<str> = name.into();
                names.insert(name.to_string(), n.clone());
                n
            }
        };
        events.push((e["ts"].as_f64().unwrap_or(0.0), ph == "B", tid, name));
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (ts, begin, tid, name) in events {
        let ph = if begin { "B" } else { "E" };
        let name = name.to_string();
        let stack = stacks.entry(tid).or_default();
        match ph {
            "B" => {
                let root = stack.first().map_or_else(|| name.clone(), |o| o.root.clone());
                stack.push(Open {
                    name,
                    start: ts,
                    child_us: 0.0,
                    root,
                });
            }
            "E" => {
                let Some(open) = stack.pop() else { continue };
                let dur = ts - open.start;
                if let Some(parent) = stack.last_mut() {
                    parent.child_us += dur;
                }
                // One main-world update per frame.
                if open.name.starts_with("main app") {
                    frames += 1;
                }
                if frames < skip {
                    continue;
                }
                if stack.is_empty() || open.name.starts_with("main app") || open.name.starts_with("sub app") {
                    let r = roots.entry(open.name.clone()).or_default();
                    r.count += 1;
                    r.total_us += dur;
                }
                let thread = threads.get(&tid).map_or("?", |t| t.as_str());
                let group = if thread.starts_with("Compute") || thread.starts_with("Async") || thread.starts_with("IO")
                {
                    "workers".to_string()
                } else {
                    format!("thread {thread}, in {}", open.root)
                };
                let self_us = dur - open.child_us;
                if folded_out.is_some() {
                    let mut key = thread_group(thread);
                    for o in stack.iter() {
                        key.push(';');
                        key.push_str(&short_label(&o.name));
                    }
                    key.push(';');
                    key.push_str(&short_label(&open.name));
                    *folded.entry(key).or_default() += self_us;
                }
                let s = stats.entry((group, open.name)).or_default();
                s.count += 1;
                s.total_us += dur;
                s.self_us += self_us;
                s.add_frame((frames - skip) as usize, self_us);
            }
            _ => {}
        }
    }
    let counted = frames.saturating_sub(skip).max(1) as f64;
    println!(
        "{frames} frames in the trace, {} after skipping {skip}",
        frames.saturating_sub(skip)
    );
    println!(
        "\ntop-level spans and the worlds' updates (main app: the main world; sub app RenderApp: the render world, pipelined beside it):"
    );
    let main_frame_us = roots
        .iter()
        .filter(|(n, _)| n.starts_with("main app"))
        .map(|(_, s)| s.total_us)
        .sum::<f64>()
        / counted;
    let mut r: Vec<_> = roots.into_iter().collect();
    r.sort_by(|a, b| b.1.total_us.total_cmp(&a.1.total_us));
    for (name, s) in r.iter().take(12) {
        println!(
            "  {:>8.3} ms avg  {:>8} x  {:>9.3} ms/frame  {}",
            s.total_us / s.count as f64 / 1e3,
            s.count,
            s.total_us / counted / 1e3,
            name
        );
    }
    let mut rows: Vec<_> = stats
        .into_iter()
        .filter(|((_, n), _)| filter.as_ref().is_none_or(|f| n.contains(f.as_str())))
        .collect();
    rows.sort_by(|a, b| {
        let key = |s: &Stat| if by_total { s.total_us } else { s.self_us };
        key(&b.1).total_cmp(&key(&a.1))
    });
    println!(
        "\nspans by {} time, ms per frame (thread and top-level span; systems run on the workers):",
        if by_total { "total" } else { "self" }
    );
    println!(
        "  (p99: self ms in the 99th-percentile frame; %: of the main world's {:.3} ms frame)",
        main_frame_us / 1e3
    );
    println!("  {:>8} {:>8} {:>6} {:>8} {:>8}  name", "self", "p99", "%", "total", "calls");
    let n_frames = counted as usize;
    for ((root, name), s) in rows.iter().take(top) {
        println!(
            "  {:>8.3} {:>8.3} {:>5.1}% {:>8.3} {:>8.1}  {name}  [{root}]",
            s.self_us / counted / 1e3,
            s.p99(n_frames) / 1e3,
            100.0 * s.self_us / counted / main_frame_us.max(1e-9),
            s.total_us / counted / 1e3,
            s.count as f64 / counted
        );
    }
    if let Some(out) = folded_out {
        let mut lines: Vec<_> = folded.into_iter().filter(|(_, us)| *us >= 1.0).collect();
        lines.sort_by(|a, b| a.0.cmp(&b.0));
        let text: String = lines.iter().map(|(k, us)| format!("{k} {}\n", us.round() as u64)).collect();
        if let Err(e) = std::fs::write(&out, text) {
            eprintln!("{out}: {e}");
            return ExitCode::FAILURE;
        }
        println!("\nwrote {out}: span stacks, self µs over {n_frames} frames (profsum {out} --unit us --frames {n_frames})");
    }
    ExitCode::SUCCESS
}
