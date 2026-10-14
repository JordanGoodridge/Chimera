//! Builds `map::MapPlugin`: the map's own systems (`map`) and the audio
//! that plays its sounds (`audio`), registered in one fixed order (the
//! order decides how Bevy orders systems that aren't ordered explicitly).
//! The plugin's type stays in `map`, so `map::MapPlugin` is what tests and
//! apps add; its registration lives here, above both layers.

use bevy::prelude::*;

use crate::{audio, map::MapPlugin};

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        self.build_first(app);
        app.add_plugins(audio::SoundPlugin);
        self.build_rest(app);
    }
}
