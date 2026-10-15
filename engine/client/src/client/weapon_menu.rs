//! The state of a game HUD's weapon selection (CS:S's: its front end's
//! `weapon_select` draws and drives it): the shell's input reads it for
//! the intent's pick and to hold fire after a pick.

use bevy::prelude::*;

/// The selection's state.
#[derive(Resource, Default)]
pub struct WeaponMenu {
    /// The highlighted weapon while the row shows.
    pub highlighted: Option<Entity>,
    /// Seconds the row has shown; None when hidden.
    pub shown_for: Option<f32>,
    /// Open for a pick (fast switch off), not just showing a switch.
    pub choosing: bool,
    /// The slot or pick (`core::Intent::select`) to select and how long
    /// it is still held in the intent.
    pub select: Option<(u8, f32)>,
    /// Attack has picked a weapon and is still held: no firing until it
    /// is let go.
    pub swallow_fire: bool,
    /// Slot keys work through the selection (fast switch off and the game's
    /// HUD has one); else they switch directly.
    pub takes_slots: bool,
    /// Slot commands run since last frame (console, binds of `slotN`).
    pub pressed: Vec<u8>,
    /// `invnext` (+1) and `invprev` (-1) run since last frame.
    pub steps: Vec<i8>,
}

impl WeaponMenu {
    /// The slot the input should select this frame.
    pub fn selected_slot(&self) -> Option<u8> {
        self.select.map(|s| s.0)
    }

    /// Close the selection row, picking nothing (`cancelselect`).
    pub fn cancel(&mut self) {
        self.shown_for = None;
        self.choosing = false;
        self.highlighted = None;
    }
}
