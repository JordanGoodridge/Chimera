//! Server capacity (docs/plans/active/multiplayer.md, "Capacity"): a
//! server on de_dust2 full of players (32 by default, CS:S's most), a few
//! of them clients over `NetSim`'s in-memory link and the rest bots, all
//! in deathmatch (everyone alive and moving, the worst case), measured
//! after a warm-up:
//!
//! - the server's CPU time per tick (every thread of the process while
//!   the server's frame runs; the clients run after it, one at a time),
//!   median and p99, against the 66-tick budget (15 ms) with margin, and
//!   its instructions per tick (all threads; stable under load);
//! - bytes sent to each client a second (its link's count), and the
//!   updates its `rate` choked;
//! - each client's CPU time per frame (headless: simulation, prediction,
//!   interpolation; no drawing);
//! - the process's memory;
//! - the share of character pairs in each other's PVS (what Source-style
//!   culling of bodies would keep).
//!
//! `MASHUP_CAPACITY_PLAYERS` (32), `MASHUP_CAPACITY_CLIENTS` (4: clients
//! cost the test time, the bots the server time), `MASHUP_CAPACITY_MAP`
//! (de_dust2; any map of the install or the content cache),
//! `MASHUP_CAPACITY_SECONDS` (20 game seconds measured) for bigger runs,
//! `MASHUP_CAPACITY_CONSOLE` server console lines (e.g. `bot_stop 1`, to
//! tell what costs);
//! the budget is asserted only at the default size. Prints a table:
//! `cargo test --features dev --test it heavy::net_capacity -- --nocapture`.
//! Skipped without a CS:S install.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use bevy::{
    ecs::message::{MessageCursor, Messages},
    prelude::*,
};
use crate::stack::{
    console::Console,
    core::{Health, Intent, LocalPlayer, MovementState, Team},
    games::{
        self,
        cs_source::{
            self,
            movement::{self as source, SourceMovementPlugin},
            weapons::CsWeaponsPlugin,
        },
    },
    greybox::GreyboxMapPlugin,
    harness::{CsSim, NetSim},
    map::{MapData, MapPlugin},
    metrics::AllThreads,
    mount::config::LocalConfig,
    net::{NetCharacter, NetEvent, NetSettings, client::Joined, maps::MapFiles, memory::LinkConditions},
    rules::Dead,
    slots::Loadout,
};

/// The 66-tick budget (CS:S's tick), ms.
const TICK_BUDGET_MS: f64 = 15.0;

fn installed() -> bool {
    let ok = LocalConfig::load()
        .ok()
        .and_then(|c| c.game_path(cs_source::GAME))
        .is_some_and(|p| p.join("cstrike").is_dir());
    if !ok {
        eprintln!("skipping: no CS:S install configured");
    }
    ok
}

fn env<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok()?.parse().ok()
}

/// The map, loaded once and handed to every world as a copy.
#[derive(Resource, Clone)]
struct OneMap(Arc<Mutex<Option<(String, MapData)>>>);

fn load_requested(world: &mut World, mut cursor: Local<MessageCursor<NetEvent>>) {
    let events: Vec<NetEvent> = cursor.read(world.resource::<Messages<NetEvent>>()).cloned().collect();
    for e in events {
        if let NetEvent::LoadMap { map, .. } = e {
            let data = world
                .resource::<OneMap>()
                .0
                .lock()
                .unwrap()
                .as_ref()
                .map(|(_, d)| d.clone());
            crate::stack::swap_map(world, &map, data, Duration::from_secs_f64(cs_source::TICK_INTERVAL));
        }
    }
}

/// Resident plus swapped-out memory of this process, MB (Linux).
fn rss_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb = |key: &str| -> Option<f64> {
        status
            .lines()
            .find(|l| l.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    Some((kb("VmRSS:")? + kb("VmSwap:").unwrap_or(0.0)) / 1000.0)
}

fn percentile(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}

/// A scripted player: walks about, turns now and then, shoots at the
/// nearest enemy in sight range.
struct Walker {
    rng: u64,
    axis: Vec2,
    yaw: f32,
    next_turn: u64,
}

impl Walker {
    fn next(&mut self) -> f32 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn drive(&mut self, world: &mut World, step: u64) {
        let Some(me) = world.query_filtered::<Entity, With<LocalPlayer>>().iter(world).next() else {
            return;
        };
        let Some(team) = world.get::<Team>(me).map(|t| t.0) else {
            return;
        };
        if world.get::<Dead>(me).is_some() {
            return;
        }
        let eye = world.get::<Transform>(me).map_or(Vec3::ZERO, |t| t.translation)
            + world.get::<MovementState>(me).map_or(Vec3::Y * 0.6, |m| m.eye_offset);
        let enemy = world
            .query::<(Entity, &Team, &Health, &Transform, &NetCharacter)>()
            .iter(world)
            .filter(|(e, t, h, ..)| *e != me && t.0 != team && t.0 != 0 && h.current > 0.0)
            .map(|(.., tr, _)| tr.translation)
            .filter(|p| p.distance(eye) < 30.0)
            .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
        if step >= self.next_turn {
            self.next_turn = step + 30 + (self.next() * 120.0) as u64;
            let a = self.next() * std::f32::consts::TAU;
            self.axis = Vec2::new(a.cos(), a.sin());
            self.yaw = (self.yaw + (self.next() - 0.5) * 2.5).rem_euclid(std::f32::consts::TAU);
        }
        let jump = self.next() < 0.01;
        let mut i = world.get_mut::<Intent>(me).unwrap();
        i.move_axis = self.axis;
        i.jump = jump;
        match enemy {
            Some(at) => {
                let d = at + Vec3::Y * 0.4 - eye;
                i.yaw = (-d.x).atan2(-d.z).rem_euclid(std::f32::consts::TAU);
                i.pitch = d.y.atan2(d.xz().length());
                i.fire = !i.fire;
            }
            None => {
                i.yaw = self.yaw;
                i.pitch = 0.0;
                i.fire = false;
            }
        }
    }
}

struct Results {
    players: usize,
    clients: usize,
    tick_p50: f64,
    tick_p99: f64,
    kb_per_client: Vec<f64>,
    client_p50: f64,
    client_p99: f64,
    rss: Option<f64>,
    /// Mean instructions per tick, millions (None without counters).
    ins_mean: Option<f64>,
}

fn run(map: &str, players: usize, clients: usize, seconds: f64) -> Results {
    let id = if map.contains(':') {
        map.to_string()
    } else {
        format!("{}:{map}", cs_source::GAME)
    };
    let data = games::load_map(&id).unwrap_or_else(|e| panic!("{id}: {e}"));
    let vis = data.visibility.clone();
    let one = OneMap(Arc::new(Mutex::new(Some((id.clone(), data.clone())))));
    let setup = {
        let one = one.clone();
        move |app: &mut App| {
            // A `--features profile` build: spans in a Chrome trace
            // (`TRACE_CHROME=<file>`; docs/performance.md, "Measuring");
            // profile with `MASHUP_CAPACITY_CLIENTS=0` (one world).
            #[cfg(feature = "profile")]
            app.add_plugins(bevy::log::LogPlugin::default());
            app.add_plugins((
                GreyboxMapPlugin,
                MapPlugin::empty(),
                SourceMovementPlugin,
                CsWeaponsPlugin,
            ))
            .insert_resource(Loadout { movement: source::ID })
            .insert_resource(MapFiles {
                read: Arc::new(games::map_file_bytes),
                cache: None,
            })
            .insert_resource(one.clone())
            .add_systems(Update, load_requested);
        }
    };
    let mut sim = NetSim::new(CsSim, LinkConditions::default(), 7, 0, setup);
    // As the game and the dedicated server run (`MASHUP_EXECUTOR=multi`:
    // Bevy's default, for comparison).
    let single = std::env::var("MASHUP_EXECUTOR").as_deref() != Ok("multi");
    if single {
        crate::stack::single_threaded_schedules(&mut sim.server.app);
    }
    {
        let w = sim.server.app.world_mut();
        w.resource_mut::<NetSettings>().maxplayers = players as u32;
        crate::stack::swap_map(w, &id, Some(data), Duration::from_secs_f64(cs_source::TICK_INTERVAL));
        w.resource_mut::<Console>().submit(format!(
            "mashup_rounds 0; bot_join_after_player 0; bot_quota_mode fill; bot_quota {players}; {}",
            std::env::var("MASHUP_CAPACITY_CONSOLE").unwrap_or_default()
        ));
    }
    let link = LinkConditions {
        latency: Duration::from_millis(25),
        jitter: Duration::from_millis(3),
        loss: 0.0,
    };
    for _ in 0..clients {
        let i = sim.add_client(|_| {});
        sim.set_conditions(i, link);
        if single {
            crate::stack::single_threaded_schedules(&mut sim.clients[i].app);
        }
    }
    let mut walkers: Vec<Walker> = (0..clients)
        .map(|i| Walker {
            rng: 0x9E37_79B9_7F4A_7C15 ^ (i as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03),
            axis: Vec2::ZERO,
            yaw: 0.0,
            next_turn: 0,
        })
        .collect();
    let ok = sim.until(4000, |s| {
        (0..s.clients.len()).all(|i| s.clients[i].app.world().contains_resource::<Joined>())
    });
    assert!(ok, "the clients joined");

    let mut server_cpu = AllThreads::default();
    let mut client_cpu = AllThreads::default();
    let mut tick_ms: Vec<f64> = Vec::new();
    let mut main_ms: Vec<f64> = Vec::new();
    // Instructions per tick, millions (all threads; Linux with hardware
    // counters): hardly move with the machine's load.
    let mut ins: Vec<f64> = Vec::new();
    let mut client_ms: Vec<f64> = Vec::new();
    let warm = (10.0 * crate::stack::DEFAULT_TICK_HZ) as u64;
    let measured = (seconds * crate::stack::DEFAULT_TICK_HZ) as u64;
    let frame = Duration::from_secs_f64(1.0 / crate::stack::DEFAULT_TICK_HZ);
    let mut bytes_at_start: Vec<u64> = Vec::new();
    // Pairs (a character, another living one) and those in the first's
    // PVS (its cluster sees the other's): what Source-style culling would
    // keep of the bodies sent.
    let (mut pairs, mut in_pvs) = (0u64, 0u64);
    let (mut alive_sum, mut alive_samples) = (0u64, 0u64);
    let started = Instant::now();
    for step in 0..warm + measured {
        if step == warm {
            let l = sim.link.lock().unwrap();
            bytes_at_start = (0..clients)
                .map(|i| l.bytes_to.get(&sim.client_id(i)).copied().unwrap_or(0))
                .collect();
        }
        for (i, w) in walkers.iter_mut().enumerate() {
            w.drive(sim.clients[i].app.world_mut(), step);
        }
        sim.link.lock().unwrap().advance(frame);
        let before = sim.server.tick();
        let m0 = crate::stack::metrics::counters::thread_cpu_ns().unwrap_or(0);
        let c0 = server_cpu.read();
        sim.server.app.update();
        let c1 = server_cpu.read();
        let m1 = crate::stack::metrics::counters::thread_cpu_ns().unwrap_or(0);
        let ticks = sim.server.tick() - before;
        if step >= warm && ticks > 0 {
            let ms = (c1.cpu_ns - c0.cpu_ns) as f64 / 1e6 / ticks as f64;
            tick_ms.extend(std::iter::repeat_n(ms, ticks as usize));
            // The counter can read lower when the kernel rescales a
            // multiplexed counter under load: skip that sample.
            if let (Some(a), Some(b)) = (c0.hw, c1.hw)
                && let Some(d) = b.instructions.checked_sub(a.instructions)
            {
                ins.push(d as f64 / 1e6 / ticks as f64);
            }
            main_ms.extend(std::iter::repeat_n(
                (m1 - m0) as f64 / 1e6 / ticks as f64,
                ticks as usize,
            ));
        }
        if step >= warm
            && step % 16 == 0
            && let Some(vis) = &vis
        {
            let w = sim.server.app.world_mut();
            let living = w
                .query_filtered::<&Health, With<NetCharacter>>()
                .iter(w)
                .filter(|h| h.current > 0.0)
                .count();
            alive_sum += living as u64;
            alive_samples += 1;
            let at: Vec<Option<u32>> = w
                .query_filtered::<(&Transform, &Health), With<NetCharacter>>()
                .iter(w)
                .filter(|(_, h)| h.current > 0.0)
                .map(|(t, _)| vis.cluster_at(t.translation + Vec3::Y * 0.5))
                .collect();
            for (i, a) in at.iter().enumerate() {
                for (j, b) in at.iter().enumerate() {
                    if i != j {
                        pairs += 1;
                        if let (Some(a), Some(b)) = (a, b) {
                            in_pvs += u64::from(vis.sees(*a, *b));
                        } else {
                            in_pvs += 1;
                        }
                    }
                }
            }
        }
        for c in &mut sim.clients {
            let a = client_cpu.read();
            c.app.update();
            let b = client_cpu.read();
            if step >= warm {
                client_ms.push((b.cpu_ns - a.cpu_ns) as f64 / 1e6);
            }
        }
    }
    let world = sim.server.app.world_mut();
    // Updates each client was sent and those its `rate` choked.
    let paced: Vec<(u64, u64)> = world
        .query::<&crate::stack::net::server::UpdatePacing>()
        .iter(world)
        .map(|p| (p.sent, p.choked))
        .collect();
    if !paced.is_empty() {
        eprintln!("[capacity] updates sent / choked per client: {paced:?}");
    }
    let characters = world
        .query_filtered::<(), (With<NetCharacter>, With<Intent>)>()
        .iter(world)
        .count();
    eprintln!(
        "[capacity] {id}: {characters} characters, {clients} clients, {:.0} s real for {:.0} game s",
        started.elapsed().as_secs_f64(),
        (warm + measured) as f64 / crate::stack::DEFAULT_TICK_HZ
    );
    assert_eq!(characters, players, "the server holds the players");
    let l = sim.link.lock().unwrap();
    let kb_per_client = (0..clients)
        .map(|i| {
            let now = l.bytes_to.get(&sim.client_id(i)).copied().unwrap_or(0);
            (now - bytes_at_start[i]) as f64 / 1000.0 / seconds
        })
        .collect();
    eprintln!(
        "[capacity] others in a character's PVS: {:.0}% of {pairs} pairs; {:.1} living on average",
        in_pvs as f64 * 100.0 / pairs.max(1) as f64,
        alive_sum as f64 / alive_samples.max(1) as f64
    );
    eprintln!(
        "[capacity] server main thread CPU ms a tick p50 {:.2} p99 {:.2}; instructions a tick (M, all threads) p50 {:.2} mean {:.2}",
        percentile(&main_ms, 0.5),
        percentile(&main_ms, 0.99),
        percentile(&ins, 0.5),
        ins.iter().sum::<f64>() / ins.len().max(1) as f64
    );
    Results {
        players: characters,
        clients,
        tick_p50: percentile(&tick_ms, 0.5),
        tick_p99: percentile(&tick_ms, 0.99),
        kb_per_client,
        client_p50: percentile(&client_ms, 0.5),
        client_p99: percentile(&client_ms, 0.99),
        rss: rss_mb(),
        ins_mean: (!ins.is_empty()).then(|| ins.iter().sum::<f64>() / ins.len() as f64),
    }
}

#[test]
fn a_full_server_keeps_its_tick() {
    if !installed() {
        return;
    }
    let players: usize = env("MASHUP_CAPACITY_PLAYERS").unwrap_or(32);
    let clients: usize = env("MASHUP_CAPACITY_CLIENTS").unwrap_or(4).min(players);
    let seconds: f64 = env("MASHUP_CAPACITY_SECONDS").unwrap_or(20.0);
    let map: String = env("MASHUP_CAPACITY_MAP").unwrap_or_else(|| "de_dust2".into());
    let r = run(&map, players, clients, seconds);
    let kb_mean = r.kb_per_client.iter().sum::<f64>() / r.kb_per_client.len().max(1) as f64;
    let kb_max = r.kb_per_client.iter().copied().fold(0.0, f64::max);
    eprintln!(
        "| map | players | clients | server tick CPU ms p50 / p99 | KB/s a client mean / max | upload KB/s at this size | client frame CPU ms p50 / p99 | memory MB |"
    );
    eprintln!(
        "| {map} | {} | {} | {:.2} / {:.2} | {kb_mean:.1} / {kb_max:.1} | {:.0} | {:.2} / {:.2} | {} |",
        r.players,
        r.clients,
        r.tick_p50,
        r.tick_p99,
        kb_mean * (r.players.saturating_sub(1)) as f64,
        r.client_p50,
        r.client_p99,
        r.rss.map_or("?".into(), |m| format!("{m:.0}")),
    );
    let default_size = std::env::var("MASHUP_CAPACITY_PLAYERS").is_err() && map == "de_dust2";
    if default_size && !cfg!(feature = "profile") {
        // CPU time (the box's load moves it less than wall time, but
        // still: 2.8-4.4 ms p50 in the playtest build from run to run):
        // two thirds of the 66-tick budget for the median, all of it for
        // the p99, in the dev build. Before the capacity pass (multiplayer.md
        // "Capacity") the p50 was ~13 ms in the playtest build.
        assert!(
            r.tick_p50 < TICK_BUDGET_MS * 2.0 / 3.0,
            "server tick p50 {:.2} ms over two thirds of the 66-tick budget",
            r.tick_p50
        );
        assert!(
            r.tick_p99 < TICK_BUDGET_MS,
            "server tick p99 {:.2} ms over the 66-tick budget",
            r.tick_p99
        );
        // Instructions hardly move with load: ~25M a tick in the dev
        // build, ~20M in the playtest one.
        if let Some(m) = r.ins_mean {
            assert!(m < 50.0, "{m:.1}M instructions a tick (~25M expected)");
        }
        // At the default 20 updates a second: the soak's 18 KB/s for
        // twelve characters plus each other character's body.
        assert!(kb_max < 40.0, "{kb_max:.1} KB/s to a client");
    }
}
