//! Monitors (Source's func_monitor showing a point_camera's view, public
//! entity docs), for any game: what the simulation and the drawing share.
//! The logic writes the active camera (`MonitorCamera`) and its fog
//! (`MonitorFog`, here as a view's `DistanceFog` marked `SOURCE_VIEW_FOG`,
//! which the map's shaders read as the view's fog: `fog.wesl`,
//! `view_fog`); `render::monitor` draws the screen with cameras of
//! its own (`ScreenCamera`, `ScreenSkyCamera`), which every main-view
//! system leaves out.
//!
//! Monitors for any game (Source's func_monitor and point_camera): the screen
//! cameras' markers (`ScreenCamera`, `ScreenSkyCamera`, kept out of every
//! main-view query) and the point_camera's fog as a `DistanceFog` marked
//! `SOURCE_VIEW_FOG`, which `fog.wesl` reads as the view's fog.

use bevy::prelude::*;

use super::MonitorFog;

/// `DistanceFog::directional_light_exponent` marking a view's fog as
/// Source fog for the map's shaders (`fog.wesl`): its colour alpha 1 when
/// on, its linear start and end in meters, its max density in the
/// directional light colour's red (alpha 0, so Bevy's own scattering stays
/// off).
pub const SOURCE_VIEW_FOG: f32 = -7.0;

impl MonitorFog {
    /// The fog as the screen camera's `DistanceFog` (`SOURCE_VIEW_FOG`),
    /// distances scaled to meters by `scale`.
    #[cfg(feature = "render")]
    pub fn distance_fog(&self, scale: f32) -> bevy::pbr::DistanceFog {
        let [r, g, b] = self.color;
        bevy::pbr::DistanceFog {
            color: Color::srgba(r, g, b, if self.enabled { 1.0 } else { 0.0 }),
            directional_light_color: Color::linear_rgba(self.max_density, 0.0, 0.0, 0.0),
            directional_light_exponent: SOURCE_VIEW_FOG,
            falloff: bevy::pbr::FogFalloff::Linear {
                start: self.start * scale,
                end: self.end * scale,
            },
        }
    }
}

/// Our camera drawing into the monitor screen (`render::monitor`).
#[derive(Component)]
pub struct ScreenCamera;

/// The sky behind the monitor camera's view (as `SkyboxCamera` is for the
/// main view). It is a `ScreenCamera` too, so what leaves monitor cameras
/// out (the main view's systems) leaves it out.
#[derive(Component)]
pub struct ScreenSkyCamera;
