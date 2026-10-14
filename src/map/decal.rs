//! Runtime decals (bullet holes, knife slashes) for any game, as data:
//! the map carries decal definitions grouped by name (`MapDecals`), which
//! `crate::render::decal` projects onto what a `PlaceDecal` message hits;
//! how high above surfaces impacts lie (`LIFT`, `DEPTH_DROP`: dynamic
//! shadows lie between).
//!
//! Runtime decals for any game as data: `MapDecals` (decal images in atlas
//! rectangles, weighted groups, surface material → group), the impacts' lift.

use std::collections::HashMap;

use bevy::prelude::*;

/// One decal image: a rectangle of a texture (often a decal atlas) and its
/// size on surfaces.
#[derive(Clone, Debug)]
pub struct MapDecal {
    /// Index into `MapData::textures`.
    pub texture: usize,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    /// Width and height, meters.
    pub size: Vec2,
    pub blend: DecalBlend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecalBlend {
    /// The surface times 2 x texel (0.5 grey leaves it unchanged).
    Modulate2x,
    /// Alpha-blended decals (lit by the surface in the game); drawn as a
    /// stain for now.
    Alpha,
}

/// The map's runtime decals: definitions, named groups of weighted
/// variants, and the group a surface material letter takes when shot.
#[derive(Clone, Debug, Default)]
pub struct MapDecals {
    pub decals: Vec<MapDecal>,
    /// Group name -> (weight, index into `decals`).
    pub groups: HashMap<String, Vec<(f32, usize)>>,
    /// Surface game material letter -> group name.
    pub by_material: HashMap<char, String>,
}

/// Lift off the surface (meters): above the map's own decals (0.15 units)
/// and overlays (up to 0.25 units), so impacts draw on top of them.
pub(crate) const LIFT: f32 = 0.35 * 0.0254;
/// How far below the decal its depth-only copy lies (meters): that copy
/// (0.30 units up) keeps overlays (up to 0.25) off impacts while every
/// impact's colour (0.35) still passes over every other impact's depth.
/// Dynamic shadows lie above it (`shadows::LIFT`), so they darken
/// impacts as the ground around them.
pub(crate) const DEPTH_DROP: f32 = 0.05 * 0.0254;
