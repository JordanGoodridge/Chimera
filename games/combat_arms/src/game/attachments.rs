//! Combat Arms' weapon attachments: what can be fitted to a weapon and
//! how each changes it (its numbers, its alternate mode, its looks).
//! Defined by specs/combat_arms/attachments.md (not written yet).
//!
//! Wave 0: none.
//!
//! The seam: Chimera has no attachment framework. An attachment is most
//! likely data (a document naming the weapon parts it changes, `data`)
//! applied by a hook registered here when a weapon is built (as the data's
//! hooks run after its parts, `weapon::data`), with the alternate modes
//! Chimera has (`weapon::AltModes`: zoom, silencer, burst) where they fit.
//! If a second game needs attachments, the part moves into Chimera's
//! weapon framework (the rule of three).

use bevy::prelude::*;

/// Combat Arms' attachments: nothing yet.
pub struct CaAttachmentsPlugin;

impl Plugin for CaAttachmentsPlugin {
    fn build(&self, _app: &mut App) {}
}
