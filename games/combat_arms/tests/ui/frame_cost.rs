//! The front end's per-frame cost doesn't grow with the string table
//! while its screens are down: the options screen, the lobby and the chat
//! panel each cloned `CaUi` (the install's string table with it) every
//! frame, about 16 ms a frame in Lucker Party (docs/performance.md,
//! "Lucker Party"). Headless on `CaSim` with the screens' state plugins;
//! frames with a table of 200 000 extra strings against frames with ours,
//! compared in the same run (so the box's load cancels out).

use std::time::{Duration, Instant};

use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{
    harness::CaSim,
    ui::{CaUi, lobby::CaLobbyStatePlugin, menu::CaMenuStatePlugin, options::CaOptionsStatePlugin, talk::CaTalkStatePlugin},
};

const FRAMES: u32 = 20;

fn frames(extra_strings: usize) -> Duration {
    let mut s = Sim::with(CaSim, |app| {
        app.add_plugins((GreyboxMapPlugin, CaMenuStatePlugin, CaLobbyStatePlugin, CaOptionsStatePlugin, CaTalkStatePlugin));
    });
    {
        let mut ui = s.app.world_mut().resource_mut::<CaUi>();
        for i in 0..extra_strings {
            ui.strings.values.insert(format!("Neutral_String_{i}"), format!("Some text number {i}"));
        }
    }
    s.app.update();
    let t = Instant::now();
    for _ in 0..FRAMES {
        s.app.update();
    }
    t.elapsed()
}

#[test]
fn screens_that_are_down_cost_nothing_per_string() {
    let ours = frames(0);
    let big = frames(200_000);
    // Three clones of the big table a frame took ~50 ms (1 s for the
    // frames); a frame without them is the same either way.
    assert!(
        big < ours * 2 + Duration::from_millis(150),
        "{FRAMES} frames: {big:?} with a big string table, {ours:?} with ours: something copies the table every frame"
    );
}
