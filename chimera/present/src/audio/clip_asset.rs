//! Sound files as assets (architecture review L4): a `.wav` or `.mp3` of
//! a game's install loaded by the `AssetServer` (`<game>://sound/...`,
//! `crate::assets`) and decoded on its threads into a `SoundAsset` (the
//! clip the live sounds play, `map::MapSoundClip`); freed when the last
//! handle drops. Typed loads pick this loader over Bevy's own `.wav`
//! one (`AudioSource`).

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};

use crate::map::sound::{MapSoundClip, decode_clip};

/// A decoded sound file.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct SoundAsset(pub MapSoundClip);

/// Decodes `.wav` and `.mp3` files into `SoundAsset`s.
#[derive(TypePath, Default)]
pub struct SoundLoader;

impl AssetLoader for SoundLoader {
    type Asset = SoundAsset;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(&self, reader: &mut dyn Reader, _: &(), _: &mut LoadContext<'_>) -> Result<SoundAsset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        decode_clip(&bytes).map(SoundAsset).map_err(std::io::Error::other)
    }

    fn extensions(&self) -> &[&str] {
        &["wav", "mp3"]
    }
}

/// `SoundAsset` and its loader, where there is an asset server.
pub struct SoundAssetPlugin;

impl Plugin for SoundAssetPlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<AssetServer>() {
            app.init_asset::<SoundAsset>().register_asset_loader(SoundLoader);
        }
    }
}
