//! Performance metrics (`chimera_core::metrics`, `client::frame_metrics`):
//! ticks and frames are measured with thread CPU time (and hardware
//! counters where the kernel allows them), schedules are named in order,
//! and a frame far slower than the median is flagged as a hitch (the
//! rule on made-up frame times, so load can't change the answer).

use std::time::Duration;

use bevy::prelude::*;
use chimera_client::client::frame_metrics::{FrameMetrics, FrameMetricsPlugin, HitchRatio};
use chimera_core::{
    console::ConsolePlugin,
    metrics::{TickMetrics, TickMetricsPlugin, hw_status},
};
use chimera_sim::SimBasePlugins;
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};

#[test]
fn every_tick_is_measured() {
    let mut sim = Sim::new((SimBasePlugins, TickMetricsPlugin), GreyboxMapPlugin);
    let before = sim.app.world().resource::<TickMetrics>().ticks;
    sim.ticks(50);
    let m = sim.app.world().resource::<TickMetrics>();
    assert_eq!(m.ticks - before, 50);
    let cpu = m.work.cpu_ms.ring.percentiles().expect("cpu times");
    assert!(cpu.total > 0.0, "{cpu:?}");
    match m.work.instructions.ring.percentiles() {
        // A greybox tick runs the engine's systems: well over 10k
        // instructions.
        Some(i) => assert!(i.p50 > 1e4, "{i:?}"),
        None => eprintln!("hardware counters: {}", hw_status()),
    }
}

#[derive(Resource, Default)]
struct UpdateCount(u32);

/// Frame 100 sleeps 40 ms (the others take microseconds).
fn slow_frame(mut n: ResMut<UpdateCount>) {
    n.0 += 1;
    if n.0 == 100 {
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// The marks on a real app with a real clock: every schedule is measured
/// in order, every frame counted, and the slow frame's time is in the
/// distribution. Only what load can't change is asserted (a sleep lasts at
/// least its time; a busy machine only adds to it). Whether frame 100 is
/// flagged as a hitch is not: under load other frames are slow too, and
/// more than 16 later hitches push it out of `hitch_frames`. The rule is
/// checked on made-up frame times below.
#[test]
fn frames_are_measured_in_schedule_order() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ConsolePlugin, FrameMetricsPlugin))
        .init_resource::<UpdateCount>()
        .add_systems(Update, slow_frame);
    app.finish();
    app.cleanup();
    for _ in 0..120 {
        app.update();
    }
    let world = app.world();
    assert_eq!(world.resource::<HitchRatio>().0, 2.0);
    let m = world.resource::<FrameMetrics>();
    let names = m.schedule_names();
    let first = names.iter().position(|n| n == "First").expect("First measured");
    let update = names.iter().position(|n| n == "Update").expect("Update measured");
    assert!(
        first < update && names.last().map(String::as_str) == Some("Last"),
        "{names:?}"
    );
    // The first frame has no start before it.
    assert_eq!(m.frames, 119);
    assert_eq!(m.main.wall_ms.ring.len(), 120);
    assert_eq!(m.frame_ms.ring.len(), 119);
    assert!(m.frame_ms.ring.percentiles().unwrap().max >= 40.0);
}

/// The hitch rule on made-up frame times (`FrameMetrics::record_frame`,
/// which the marks call with each frame's wall time): over
/// `chimera_hitch_ratio` times the median is a hitch, nothing is one before
/// the median is known (frame 61), and a ratio of 0 turns it off.
#[test]
fn a_frame_far_slower_than_the_median_is_a_hitch() {
    let times = |slow: u64| (1..=120u64).map(move |n| if n == slow { 40.0 } else { 1.0 + (n % 3) as f64 * 0.1 });
    let mut m = FrameMetrics::default();
    let flagged: Vec<u64> = times(100)
        .enumerate()
        .filter(|&(_, t)| m.record_frame(t, 2.0))
        .map(|(i, _)| i as u64 + 1)
        .collect();
    assert_eq!(flagged, [100]);
    assert_eq!(m.hitch_frames, [100]);
    assert_eq!(m.frames, 120);
    assert_eq!(m.last.frame_ms, times(100).last().unwrap());
    // Just under the ratio: not a hitch.
    let mut m = FrameMetrics::default();
    for t in times(0) {
        m.record_frame(t, 2.0);
    }
    assert!(!m.record_frame(2.0, 2.0) && m.record_frame(2.3, 2.0));
    // Before the median is known, and with the log off: none.
    let mut m = FrameMetrics::default();
    assert!(
        times(30).all(|t| !m.record_frame(t, 2.0)),
        "frame 30 is before the median"
    );
    let mut m = FrameMetrics::default();
    assert!(times(100).all(|t| !m.record_frame(t, 0.0)), "chimera_hitch_ratio 0");
    assert!(m.hitch_frames.is_empty());
    // The latest 16 are kept.
    let mut m = FrameMetrics::default();
    for t in times(0) {
        m.record_frame(t, 2.0);
    }
    for _ in 0..20 {
        m.record_frame(40.0, 2.0);
    }
    assert_eq!(m.hitch_frames, (125..=140).collect::<Vec<u64>>());
}
