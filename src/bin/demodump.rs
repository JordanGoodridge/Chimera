//! Development tool: read a Source demo (`.dem`, e.g. a SourceTV recording
//! of a CS:S dedicated server) and print what the server sent, one line per
//! item with its tick: game events, user messages, sounds, temp entities,
//! entity prop changes, console output. The parsing is
//! `source_formats::dem`; see docs/OBSERVABILITY.md ("CS:S demos").

use source_formats::dem::entities::ChangeKind;
use source_formats::dem::events::SoundLayout;
use source_formats::dem::sendtable::PropValue;
use source_formats::dem::{DemoReader, DemoState, Record};
use std::collections::{BTreeMap, HashMap};
use std::process::ExitCode;

const USAGE: &str = "\
usage: demodump <file.dem> [options]
  --summary                 header, server info, tick rate, string tables,
                            classes in use, players, event/message counts
  --events [names]          game events (comma-separated names or globs)
  --usermsgs [names]        user messages (TextMsg, HintText, Fade, ...)
  --sounds [glob]           svc_Sounds entries with their sound names
  --tempents [classes]      temp entities with their props
  --print                   svc_Print, console commands, convars, string cmds
  --entity <class|index>    entities to watch (class name or glob, or index)
  --props <glob[,glob]>     their props to print when they change (default *)
  --list-props <class>      the flattened props of a class (path, type, bits)
  --ticks <a..b>            only items with a <= tick <= b (either may be empty)
  --errors                  parse errors (also counted in --summary)
Prints `tick seconds kind ...` lines. Glob: * and ?, case-insensitive.";

/// `println!` that ends quietly when stdout is closed (`| head`).
macro_rules! out {
    ($($t:tt)*) => {{
        use std::io::Write;
        if writeln!(std::io::stdout().lock(), $($t)*).is_err() {
            std::process::exit(0);
        }
    }};
}

fn glob(pattern: &str, text: &str) -> bool {
    fn rec(p: &[u8], t: &[u8]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some(b'*'), _) => rec(&p[1..], t) || (!t.is_empty() && rec(p, &t[1..])),
            (Some(b'?'), Some(_)) => rec(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => rec(&p[1..], &t[1..]),
            _ => false,
        }
    }
    rec(pattern.as_bytes(), text.as_bytes())
}

/// A name filter: None = everything.
fn matches(filter: &Option<Vec<String>>, name: &str) -> bool {
    filter.as_ref().is_none_or(|f| f.iter().any(|p| glob(p, name)))
}

#[derive(Default)]
struct Options {
    path: Option<String>,
    summary: bool,
    events: Option<Option<Vec<String>>>,
    usermsgs: Option<Option<Vec<String>>>,
    sounds: Option<Option<Vec<String>>>,
    tempents: Option<Option<Vec<String>>>,
    print: bool,
    errors: bool,
    entity: Option<String>,
    props: Vec<String>,
    list_props: Option<String>,
    ticks: (i32, i32),
    sound_layout: Option<SoundLayout>,
}

fn parse_args() -> Result<Options, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut o = Options {
        ticks: (i32::MIN, i32::MAX),
        ..Default::default()
    };
    let mut i = 0;
    let list = |s: &str| {
        s.split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    // An optional value: the next argument unless it is an option or a demo.
    let optional = |i: &mut usize| -> Option<Vec<String>> {
        let next = args.get(*i + 1)?;
        if next.starts_with("--") || next.ends_with(".dem") {
            return None;
        }
        *i += 1;
        Some(list(next))
    };
    while i < args.len() {
        let a = args[i].as_str();
        let value = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i).cloned().ok_or(format!("{a} needs a value"))
        };
        match a {
            "--summary" => o.summary = true,
            "--events" => o.events = Some(optional(&mut i)),
            "--usermsgs" => o.usermsgs = Some(optional(&mut i)),
            "--sounds" => o.sounds = Some(optional(&mut i)),
            "--tempents" => o.tempents = Some(optional(&mut i)),
            "--print" => o.print = true,
            "--errors" => o.errors = true,
            "--sound-layout" => {
                let v = value(&mut i)?;
                let (bits, dsp) = v.split_once(",").ok_or("--sound-layout wants <flag bits>,<0|1>")?;
                o.sound_layout = Some(SoundLayout {
                    flag_bits: bits.parse().map_err(|_| "bad flag bits")?,
                    special_dsp: dsp == "1",
                });
            }
            "--entity" => o.entity = Some(value(&mut i)?),
            "--props" => o.props = list(&value(&mut i)?),
            "--list-props" => o.list_props = Some(value(&mut i)?),
            "--ticks" => {
                let v = value(&mut i)?;
                let (a, b) = v.split_once("..").ok_or("--ticks wants a..b")?;
                let num = |s: &str, d: i32| {
                    if s.is_empty() {
                        Ok(d)
                    } else {
                        s.parse().map_err(|_| format!("bad tick {s}"))
                    }
                };
                o.ticks = (num(a, i32::MIN)?, num(b, i32::MAX)?);
            }
            "-h" | "--help" => return Err(String::new()),
            _ if o.path.is_none() && !a.starts_with("--") => o.path = Some(a.to_string()),
            _ => return Err(format!("unknown argument {a}")),
        }
        i += 1;
    }
    if o.props.is_empty() {
        o.props.push("*".into());
    }
    Ok(o)
}

/// A player's name for an entity index (from `userinfo`), if any.
fn player_name(st: &DemoState, index: u32) -> Option<String> {
    let e = st.table("userinfo")?.get((index as usize).checked_sub(1)?)?;
    source_formats::dem::stringtable::parse_userinfo(&e.data).map(|p| p.0)
}

fn entity_label(st: &DemoState, index: u32) -> String {
    let class = st.entity(index).map(|e| st.class_name(e.class)).unwrap_or("?");
    match player_name(st, index).filter(|_| class == "CCSPlayer") {
        Some(n) => format!("{index} {class} ({n})"),
        None => format!("{index} {class}"),
    }
}

fn main() -> ExitCode {
    let o = match parse_args() {
        Ok(o) if o.path.is_some() => o,
        Ok(_) => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}");
            }
            eprintln!("{USAGE}");
            return if e.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
        }
    };
    let path = o.path.clone().unwrap();
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut reader = match DemoReader::new(&data) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(l) = o.sound_layout {
        reader.set_sound_layout(l);
    }

    let mut event_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut msg_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut sound_count = 0usize;
    let mut errors = Vec::new();
    let mut last_tick = 0;
    // Watched entity props: (entity, prop index) -> last printed value.
    let mut seen: HashMap<(u32, usize), PropValue> = HashMap::new();
    let mut list_done = false;

    while let Some(item) = reader.next_item() {
        let item = match item {
            Ok(i) => i,
            Err(e) => {
                errors.push((last_tick, e));
                break;
            }
        };
        let st = reader.state();
        let tick = item.tick;
        last_tick = last_tick.max(tick);
        let shown = tick >= o.ticks.0 && tick <= o.ticks.1;
        let t = tick as f32 * st.tick_interval();
        let head = format!("{tick:>6} {t:>8.3}");
        match &item.record {
            Record::DataTables => {
                if let Some(class) = &o.list_props
                    && !list_done
                {
                    list_done = true;
                    for (ci, c) in st.classes.iter().enumerate() {
                        if !glob(class, &c.name) {
                            continue;
                        }
                        out!("{} ({}, {} props)", c.name, c.table, st.flat[ci].len());
                        for (pi, p) in st.flat[ci].iter().enumerate() {
                            out!(
                                "  {pi:>4} {:<48} {:?} bits={} flags={:#x} [{}]",
                                p.path,
                                p.prop.kind,
                                p.prop.bits,
                                p.prop.flags,
                                p.owner
                            );
                        }
                    }
                }
            }
            Record::GameEvent(e) => {
                *event_counts.entry(e.name.clone()).or_default() += 1;
                if shown
                    && let Some(f) = &o.events
                    && matches(f, &e.name)
                {
                    out!("{head} event {e}");
                }
            }
            Record::UserMessage(m) => {
                *msg_counts.entry(m.name.clone()).or_default() += 1;
                if shown
                    && let Some(f) = &o.usermsgs
                    && matches(f, &m.name)
                {
                    out!("{head} usermsg {m}");
                }
            }
            Record::Sounds(sounds) => {
                sound_count += sounds.len();
                if shown && let Some(f) = &o.sounds {
                    for s in sounds {
                        let name = if s.sentence {
                            format!("sentence#{}", s.sound_index)
                        } else {
                            st.table_string("soundprecache", s.sound_index as usize)
                                .unwrap_or("?")
                                .to_string()
                        };
                        if !matches(f, &name) {
                            continue;
                        }
                        let stop = if s.flags & source_formats::dem::events::SND_STOP != 0 {
                            " STOP"
                        } else {
                            ""
                        };
                        out!(
                            "{head} sound {name}{stop} ent={} chan={} vol={:.2} level={} pitch={} flags={:#x} delay={:.3} origin=({}, {}, {}) seq={}",
                            entity_label(st, s.entity),
                            s.channel,
                            s.volume,
                            s.level,
                            s.pitch,
                            s.flags,
                            s.delay,
                            s.origin[0],
                            s.origin[1],
                            s.origin[2],
                            s.sequence
                        );
                    }
                }
            }
            Record::TempEntities(list) => {
                if shown && let Some(f) = &o.tempents {
                    for te in list {
                        let class = st.class_name(te.class);
                        if !matches(f, class) {
                            continue;
                        }
                        let flat = &st.flat[te.class as usize];
                        let props: Vec<String> =
                            te.props.iter().map(|(i, v)| format!("{}={v}", flat[*i].path)).collect();
                        out!("{head} tempent {class} delay={} {}", te.delay, props.join(" "));
                    }
                }
            }
            Record::Entities(changes) => {
                let Some(want) = &o.entity else { continue };
                for c in changes {
                    let watched = match want.parse::<u32>() {
                        Ok(i) => c.index == i,
                        Err(_) => {
                            let class = st.entity(c.index).map(|e| st.class_name(e.class));
                            // A deleted entity is gone from the state: forget its values.
                            class.is_some_and(|cl| glob(want, cl))
                                || (c.kind == ChangeKind::Delete && seen.keys().any(|(e, _)| *e == c.index))
                        }
                    };
                    if !watched {
                        continue;
                    }
                    match c.kind {
                        ChangeKind::Delete => {
                            seen.retain(|(e, _), _| *e != c.index);
                            if shown {
                                out!("{head} entity {} deleted", c.index);
                            }
                            continue;
                        }
                        ChangeKind::Leave => {
                            if shown {
                                out!("{head} entity {} left PVS", c.index);
                            }
                            continue;
                        }
                        ChangeKind::Enter => {
                            seen.retain(|(e, _), _| *e != c.index);
                            if shown {
                                out!("{head} entity {} entered", entity_label(st, c.index));
                            }
                        }
                        ChangeKind::Update => {}
                    }
                    let (Some(e), Some(flat)) = (st.entity(c.index), st.entity_props(c.index)) else {
                        continue;
                    };
                    // On enter, every known value (baseline included) counts as new.
                    let indices: Vec<usize> = if c.kind == ChangeKind::Enter {
                        (0..flat.len()).filter(|&i| e.props[i].is_some()).collect()
                    } else {
                        c.props.clone()
                    };
                    for pi in indices {
                        let p = &flat[pi];
                        if !o.props.iter().any(|g| glob(g, &p.path) || glob(g, p.name())) {
                            continue;
                        }
                        let Some(v) = &e.props[pi] else { continue };
                        if seen.get(&(c.index, pi)) == Some(v) {
                            continue;
                        }
                        seen.insert((c.index, pi), v.clone());
                        if shown {
                            out!("{head} ent {} {} = {v}", entity_label(st, c.index), p.path);
                        }
                    }
                }
            }
            Record::Print(s) if o.print && shown => out!("{head} print {}", s.trim_end()),
            Record::ConsoleCmd(s) if o.print && shown => out!("{head} consolecmd {s}"),
            Record::StringCmd(s) if o.print && shown => out!("{head} stringcmd {s}"),
            Record::SetConVar(v) if o.print && shown => {
                for (k, val) in v {
                    out!("{head} convar {k} = {val:?}");
                }
            }
            Record::Error(e) => {
                if o.errors && shown {
                    out!("{head} error {e}");
                }
                errors.push((tick, e.clone()));
            }
            _ => {}
        }
    }

    if o.summary {
        let st = reader.state();
        let h = &st.header;
        out!("file        {path}");
        out!(
            "demo        protocol {} network {} server {:?} client {:?}",
            h.demo_protocol,
            h.network_protocol,
            h.server_name,
            h.client_name
        );
        out!("map         {} ({})", h.map, h.game_dir);
        out!(
            "length      {} ticks, {} frames, {:.2} s",
            h.ticks,
            h.frames,
            h.playback_time
        );
        if let Some(si) = &st.server_info {
            out!(
                "server      {:?} tick interval {} ({:.2} Hz) max clients {} hltv {} dedicated {} os {}",
                si.host_name,
                si.tick_interval,
                1.0 / si.tick_interval,
                si.max_clients,
                si.hltv,
                si.dedicated,
                si.os
            );
        }
        out!("last tick   {last_tick}  server tick {}", st.server_tick);
        out!(
            "classes     {} server classes, {} send tables",
            st.classes.len(),
            st.send_tables.len()
        );
        out!("string tables:");
        for t in &st.string_tables {
            out!("  {:<24} {:>5} / {}", t.name, t.entries.len(), t.max_entries);
        }
        let mut live: BTreeMap<&str, usize> = BTreeMap::new();
        for e in st.entities.iter().flatten() {
            *live.entry(st.class_name(e.class)).or_default() += 1;
        }
        out!("entities at the end ({}):", live.values().sum::<usize>());
        for (c, n) in &live {
            out!("  {c:<32} {n}");
        }
        out!("players:");
        for (index, name, uid, bot) in st.players() {
            out!(
                "  ent {index:<3} userid {uid:<4} {name}{}",
                if bot { " (bot)" } else { "" }
            );
        }
        out!("game events ({} kinds in the list):", st.event_list.len());
        for (n, c) in &event_counts {
            out!("  {n:<32} {c}");
        }
        out!("user messages:");
        for (n, c) in &msg_counts {
            out!("  {n:<32} {c}");
        }
        out!("sounds      {sound_count}");
        out!("errors      {}", errors.len());
        for (t, e) in errors.iter().take(5) {
            out!("  tick {t}: {e}");
        }
    }
    if errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
