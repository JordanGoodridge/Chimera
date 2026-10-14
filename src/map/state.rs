//! State the logic layer writes on spawned map parts and the map applies
//! (entity parts on/off, prop looks and sequences, light styles,
//! soundscape switches, tonemap inputs, view kicks).
//!
//! What the logic writes on spawned parts and the map applies: `EntityPart`,
//! `PropLook`, `PropSequence`, `LightStyles`, soundscape switches,
//! `TonemapInputs`, `ViewKick`.

use bevy::prelude::*;

use super::{DEFAULT_AUTO_EXPOSURE, MapHdr};

/// A kick to a player's view from the map (env_viewpunch): pitch up and
/// yaw left, radians, added to its view punch (the weapon layer's
/// `ViewPunch`, which decays it as recoil).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct ViewKick {
    pub player: Entity,
    pub angles: Vec2,
}

/// A drawn part of a map entity the logic switches (sprites, dust
/// volumes): `on` shows a sprite or lets dust spawn motes; `exists` false
/// (killed) removes it from view at once. The logic layer writes it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntityPart {
    /// Index into `MapData::entities`.
    pub entity: usize,
    pub on: bool,
    pub exists: bool,
}

/// How a prop entity looks, as the logic says (skin family, and the body
/// number SetBodyGroup set; `MapModel::skins`, `MapModel::body_choice`).
/// The logic layer writes it; the map applies it.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PropLook {
    pub skin: i32,
    pub body_group: Option<i32>,
}

/// What an animated prop plays, as the logic says: a sequence by name
/// with a serial (a new serial restarts it), the one played at spawn and
/// after a sequence ends, and the round (a new round starts over).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct PropSequence {
    pub play: Option<(String, u32)>,
    pub default: String,
    pub round: u32,
}

/// Switchable light styles that are off (Source styles 32+). The logic
/// layer writes it; the map relights the lightmaps when it changes.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct LightStyles {
    /// Style numbers and whether each is lit; unlisted styles keep their
    /// state at map start.
    pub styles: Vec<(u8, bool)>,
}

/// Which soundscape volumes (by entity index, `SoundscapeZone::entity`)
/// the listener touches, as the logic layer's touch code says; None: the
/// map tests the zones' boxes itself.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundscapeTouches(pub Option<Vec<usize>>);

/// Which env_soundscape entities (by entity index,
/// `SoundscapeEmitter::entity`) are enabled, as the logic layer's
/// Enable/Disable inputs left them; None: the map goes by their
/// StartDisabled keys.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundscapeSwitches(pub Option<Vec<usize>>);

/// What the map's env_tonemap_controller inputs set while it runs (the
/// logic layer writes it): each set field replaces the map's look
/// (`MapHdr`), which holds what the map sets at its start.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct TonemapInputs {
    pub exposure_min: Option<f32>,
    pub exposure_max: Option<f32>,
    pub bloom_scale: Option<f32>,
    pub rate: Option<f32>,
    /// UseDefaultAutoExposure: bounds back at `DEFAULT_AUTO_EXPOSURE`
    /// (until set again).
    pub default_exposure: bool,
}

impl TonemapInputs {
    /// `hdr` with these inputs applied.
    pub fn apply(&self, hdr: &MapHdr) -> MapHdr {
        let mut out = hdr.clone();
        if let Some((lo, hi)) = out.exposure {
            let (lo, hi) = if self.default_exposure {
                DEFAULT_AUTO_EXPOSURE
            } else {
                (lo, hi)
            };
            out.exposure = Some((self.exposure_min.unwrap_or(lo), self.exposure_max.unwrap_or(hi)));
        }
        if let Some(b) = self.bloom_scale {
            out.bloom_scale = b;
        }
        if let Some(r) = self.rate {
            out.rate = r;
        }
        out
    }
}
