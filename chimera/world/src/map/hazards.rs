//! Zones a player must not be placed in, registered per game and read by
//! whatever places players on a map (the minigames' random spawns,
//! `chimera_sim::minigame::spawns`): a function returning the world-space
//! brushes (engine space, m) that hurt, kill or drown a player standing
//! in them (CS:S's `trigger_hurt`, Combat Arms' damage and liquid
//! volumes). Water volumes (`core::MapWater`) count without registering.
//! Called when the zones are wanted, so a source sees the map's current
//! state.

use bevy::prelude::*;

use crate::core::MapBrush;

/// The registered sources.
#[derive(Resource, Default)]
pub struct SpawnHazards(pub Vec<fn(&mut World) -> Vec<MapBrush>>);

/// Registering hazards.
pub trait SpawnHazardAppExt {
    fn add_spawn_hazards(&mut self, source: fn(&mut World) -> Vec<MapBrush>) -> &mut Self;
}

impl SpawnHazardAppExt for App {
    fn add_spawn_hazards(&mut self, source: fn(&mut World) -> Vec<MapBrush>) -> &mut Self {
        let mut hazards = self.world_mut().get_resource_or_init::<SpawnHazards>();
        if !hazards.0.iter().any(|s| *s as usize == source as usize) {
            hazards.0.push(source);
        }
        self
    }
}
