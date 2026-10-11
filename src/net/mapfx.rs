//! Map effects the server's logic sets off, for network clients to draw
//! (they run no map logic): strike-generator env_beams' strikes
//! (`map::emitters::BeamStrike`), point_tesla sparks
//! (`map::emitters::TeslaSpark`) and env_spark bursts
//! (`map::beams::SparkBurst`). Unreliable, like Source's temporary
//! entities: a lost one is a missed flash.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use super::MapFx;
use crate::{
    core::NetRole,
    map::{
        beams::SparkBurst,
        emitters::{BeamStrike, TeslaSpark},
    },
};

pub(super) fn plugin(app: &mut App) {
    app.add_message::<BeamStrike>()
        .add_message::<TeslaSpark>()
        .add_message::<SparkBurst>()
        .add_systems(
            PostUpdate,
            forward
                .before(ServerSystems::Send)
                .run_if(resource_equals(NetRole::Server)),
        )
        .add_systems(
            PreUpdate,
            receive
                .after(ClientSystems::Receive)
                .run_if(resource_equals(NetRole::Client)),
        );
}

/// The server: this frame's map effects to every client.
fn forward(
    mut strikes: MessageReader<BeamStrike>,
    mut teslas: MessageReader<TeslaSpark>,
    mut sparks: MessageReader<SparkBurst>,
    mut out: MessageWriter<ToClients<MapFx>>,
) {
    let fx = strikes
        .read()
        .map(|s| MapFx::Strike {
            entity: s.entity as u32,
            start: s.start.to_array(),
            end: s.end.to_array(),
            life: s.life,
        })
        .chain(teslas.read().map(|t| MapFx::Tesla {
            entity: t.entity as u32,
        }))
        .chain(sparks.read().map(|s| MapFx::Spark {
            at: s.at.to_array(),
            dir: s.dir.to_array(),
            magnitude: s.magnitude,
        }));
    for message in fx {
        out.write(ToClients {
            targets: SendTargets::CLIENTS_ONLY,
            message,
        });
    }
}

/// A client: the server's map effects as its own map's messages.
fn receive(mut fx: MessageReader<MapFx>, mut commands: Commands) {
    for f in fx.read().cloned() {
        commands.queue(move |w: &mut World| match f {
            MapFx::Strike {
                entity,
                start,
                end,
                life,
            } => {
                w.write_message(BeamStrike {
                    entity: entity as usize,
                    start: Vec3::from_array(start),
                    end: Vec3::from_array(end),
                    life,
                });
            }
            MapFx::Tesla { entity } => {
                w.write_message(TeslaSpark {
                    entity: entity as usize,
                });
            }
            MapFx::Spark { at, dir, magnitude } => {
                w.write_message(SparkBurst {
                    at: Vec3::from_array(at),
                    dir: Vec3::from_array(dir),
                    magnitude,
                });
            }
        });
    }
}
