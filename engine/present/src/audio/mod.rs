//! Audio for any game: what the listener hears of the simulation's sounds.
//! `sound` plays `map::PlaySound`s (distance model, panning, channels);
//! `live_sound` plays long-lived sounds started and stopped by
//! `map::SoundControl`; `room` the room DSP; `soundscape` picks and plays
//! the listener's soundscape; `hearing` muffles and rings after blasts.
//!
//! The simulation's side of sounds (the map's sound data, `PlaySound`,
//! `SoundControl`, the distance model) is `map::sound`; nothing in the
//! simulation uses this module. `SoundPlugin` is added with
//! `map::MapPlugin` (`crate::map_plugin`).

pub mod hearing;
pub mod live_sound;
pub mod room;
pub mod sound;
pub mod soundscape;

pub use sound::SoundPlugin;
