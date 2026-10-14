//! The video settings the options dialogs set (the game's dialog, its
//! rows: `games::cs_source::ui::options`): `mat_vsync`, `mashup_fullscreen`
//! and `mashup_resolution` (the window), `mat_antialias` (MSAA), texture
//! detail and filtering (`mat_picmip`, `mat_trilinear`, `mat_forceaniso`)
//! and the player's field of view (`fov_desired`).
//!
//! `fov_desired` (`PlayerFov`: the world camera's unzoomed field of view, 75
//! to 90, CS:S's 90); the video cvars (`mat_vsync`, `mashup_fullscreen`,
//! `mashup_resolution`, `mat_antialias`; archived) set the window's present
//! mode, mode and size when they change (not in `--window` or automated runs)
//! and every camera's MSAA; the texture settings (`TextureSettings`:
//! `mat_picmip`, `mat_trilinear`, `mat_forceaniso`) each repeating, mipmapped
//! texture takes as it is added (`apply_texture_settings`, before it goes to
//! the GPU; so from the next map load).

use bevy::{
    prelude::*,
    window::{MonitorSelection, PresentMode, PrimaryMonitor, PrimaryWindow, VideoModeSelection, WindowMode},
};

use crate::console::{Console, ConsoleAppExt, resource_cvar};

pub struct VideoPlugin;

impl Plugin for VideoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VideoSettings>()
            .init_resource::<PlayerFov>()
            .add_systems(Update, apply_video)
            .add_systems(PostUpdate, apply_msaa);
        resource_cvar::<VideoSettings, u8>(
            app,
            "mat_vsync",
            "1: wait for vertical sync (no tearing, frames capped at the refresh rate).",
            |v| &mut v.vsync,
        );
        resource_cvar::<VideoSettings, u8>(
            app,
            "mashup_fullscreen",
            "Display mode: 0 windowed, 1 full screen, 2 borderless full screen.",
            |v| &mut v.fullscreen,
        );
        resource_cvar::<VideoSettings, String>(
            app,
            "mashup_resolution",
            "Window size in pixels, e.g. 1920x1080 (empty: as started).",
            |v| &mut v.resolution,
        );
        resource_cvar::<VideoSettings, u8>(
            app,
            "mat_antialias",
            "Multisample antialiasing: 0 off, 2 or 4 samples per pixel (4 by default). Off saves GPU time \
             at high resolutions (each sample is drawn and resolved for every camera).",
            |v| &mut v.antialias,
        );
        app.console_cvar(
            "fov_desired",
            "The player's field of view, 75 to 90 degrees (horizontal at 4:3); zoomed scopes keep theirs.",
            "90",
            |w| w.get_resource::<PlayerFov>().map(|f| f.0.to_string()),
            |w, v| {
                let v: f32 = v.trim().parse().map_err(|_| format!("bad value \"{v}\""))?;
                w.get_resource_mut::<PlayerFov>().ok_or("not available")?.0 = v.clamp(FOV_RANGE.0, FOV_RANGE.1);
                Ok(())
            },
        );
        app.init_resource::<crate::render::rope_sway::RopeWind>();
        resource_cvar::<crate::render::rope_sway::RopeWind, f32>(
            app,
            "rope_wind_dist",
            "Ropes sway in the wind while the camera is within this many units of them (0: never).",
            |w| &mut w.dist,
        );
        app.init_resource::<crate::render::lod::ModelSettings>();
        resource_cvar::<crate::render::lod::ModelSettings, u8>(
            app,
            "r_rootlod",
            "Model detail: the finest level of detail models draw, 0 high, 1 medium, 2 low (models with fewer \
             levels draw their coarsest). Applies at once.",
            |s| &mut s.root_lod,
        );
        resource_cvar::<crate::render::lod::ModelSettings, i32>(
            app,
            "r_lod",
            "-1: models draw the level of detail their size on screen asks for; n: always level n.",
            |s| &mut s.lod,
        );
        app.init_resource::<crate::map::shadows::ShadowSettings>();
        resource_cvar::<crate::map::shadows::ShadowSettings, u8>(
            app,
            "r_shadows",
            "0: no dynamic shadows (props' and characters').",
            |s| &mut s.shadows,
        );
        resource_cvar::<crate::map::shadows::ShadowSettings, u8>(
            app,
            "r_shadowrendertotexture",
            "1: props cast their silhouette (render-to-texture shadows); 0: blob shadows, a round patch straight \
             down (shadow detail Low). Applies at once.",
            |s| &mut s.render_to_texture,
        );
        resource_cvar::<crate::map::shadows::ShadowSettings, u8>(
            app,
            "r_flashlightdepthtexture",
            "1: flashlights cast depth shadows (shadow detail High; CS:S has no flashlights: as Medium).",
            |s| &mut s.flashlight_depth,
        );
        app.init_resource::<TextureSettings>().add_systems(
            PostUpdate,
            apply_texture_settings
                .after(bevy::asset::AssetEventSystems)
                .run_if(resource_exists::<Assets<Image>>),
        );
        resource_cvar::<TextureSettings, i32>(
            app,
            "mat_picmip",
            "Texture detail: 2 low, 1 medium, 0 high, -1 very high (as high: no larger textures). Each level skips \
             a texture's largest mip. Applies from the next map load.",
            |t| &mut t.picmip,
        );
        resource_cvar::<TextureSettings, u8>(
            app,
            "mat_trilinear",
            "1: blend between mip levels (trilinear filtering). Applies from the next map load.",
            |t| &mut t.trilinear,
        );
        resource_cvar::<TextureSettings, u16>(
            app,
            "mat_forceaniso",
            "Anisotropic filtering: 1 off, 2, 4, 8 or 16 samples. Applies from the next map load.",
            |t| &mut t.aniso,
        );
        let mut console = app.world_mut().resource_mut::<Console>();
        for name in [
            "mat_vsync",
            "mashup_fullscreen",
            "mashup_resolution",
            "mat_antialias",
            "fov_desired",
            "mat_picmip",
            "mat_trilinear",
            "mat_forceaniso",
            "r_rootlod",
            "r_shadows",
            "r_shadowrendertotexture",
            "r_flashlightdepthtexture",
        ] {
            console.archive(name);
        }
    }
}

/// Video > Advanced's texture detail and filtering (`mat_picmip`,
/// `mat_trilinear`, `mat_forceaniso`; CS:S's defaults: high, bilinear).
/// Each map texture takes them as it loads (`apply_texture_settings`):
/// the textures live only on the GPU once drawn, so a change shows from
/// the next map load (CS:S's applies at once).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct TextureSettings {
    pub picmip: i32,
    pub trilinear: u8,
    pub aniso: u16,
}

impl Default for TextureSettings {
    fn default() -> Self {
        Self {
            picmip: 0,
            trilinear: 0,
            aniso: 1,
        }
    }
}

impl TextureSettings {
    /// Set a map texture's sampler: repeating, mipmapped and filtered
    /// (the world's, models', water's; not the HUD's or UI's pictures,
    /// which clamp, or pixel-art ones, which don't filter). Returns
    /// whether it is one.
    pub fn apply(&self, image: &mut Image) -> bool {
        use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler};
        let levels = image.texture_descriptor.mip_level_count;
        let ImageSampler::Descriptor(d) = &mut image.sampler else {
            return false;
        };
        if levels < 2
            || d.address_mode_u != ImageAddressMode::Repeat
            || d.mag_filter != ImageFilterMode::Linear
            || d.min_filter != ImageFilterMode::Linear
        {
            return false;
        }
        let aniso = self.aniso.clamp(1, 16);
        // The GPU filters anisotropically only with every filter linear.
        d.mipmap_filter = if self.trilinear != 0 || aniso > 1 {
            ImageFilterMode::Linear
        } else {
            ImageFilterMode::Nearest
        };
        d.anisotropy_clamp = aniso;
        // Low and medium skip the largest one or two levels (never the
        // last).
        d.lod_min_clamp = self.picmip.clamp(0, levels as i32 - 1) as f32;
        true
    }
}

/// Each texture added this frame takes `TextureSettings` before it goes
/// to the GPU (the render world takes it at the end of the frame).
fn apply_texture_settings(
    settings: Res<TextureSettings>,
    mut events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
) {
    for event in events.read() {
        if let AssetEvent::Added { id } = event
            && let Some(image) = images.get_mut_untracked(*id)
        {
            settings.apply(image);
        }
    }
}

/// `fov_desired`: the player's unzoomed field of view, degrees
/// horizontal at 4:3 (CS:S's 90; the video Advanced dialog's slider takes
/// 75 to 90). The world camera uses it unzoomed (`client::zoom_camera`);
/// the view model zooms with it (`map::view_model::view_model_fov`); a
/// scope's zoom is its own.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PlayerFov(pub f32);

impl Default for PlayerFov {
    fn default() -> Self {
        Self(90.0)
    }
}

/// `fov_desired`'s bounds (CS:S's slider's, `OptionsSubVideoAdvancedDlg.res`).
pub const FOV_RANGE: (f32, f32) = (75.0, 90.0);

/// The window's mode, size and vsync, as the options set them.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct VideoSettings {
    pub vsync: u8,
    pub fullscreen: u8,
    pub resolution: String,
    /// MSAA samples (`mat_antialias`).
    pub antialias: u8,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            vsync: 1,
            fullscreen: 0,
            resolution: String::new(),
            antialias: 4,
        }
    }
}

impl VideoSettings {
    /// `mat_antialias` as Bevy's MSAA setting: 0 or 1 off, 2, anything
    /// higher 4 (8 isn't supported everywhere).
    pub fn msaa(&self) -> Msaa {
        match self.antialias {
            0 | 1 => Msaa::Off,
            2 | 3 => Msaa::Sample2,
            _ => Msaa::Sample4,
        }
    }
}

/// Every camera (the main view, sky, view model, water reflection and the
/// HUD's: the ones drawing into one target should agree, or each switch
/// costs a full-screen copy) takes `mat_antialias`.
fn apply_msaa(
    settings: Res<VideoSettings>,
    cameras: Query<(Entity, Option<&Msaa>), With<Camera>>,
    mut commands: Commands,
) {
    let want = settings.msaa();
    for (e, msaa) in &cameras {
        if msaa != Some(&want) {
            commands.entity(e).insert(want);
        }
    }
}

/// `1920x1080` -> (1920, 1080).
pub fn parse_resolution(s: &str) -> Option<UVec2> {
    let (w, h) = s.trim().split_once(['x', 'X'])?;
    let size = UVec2::new(w.trim().parse().ok()?, h.trim().parse().ok()?);
    (size.x >= 320 && size.y >= 240).then_some(size)
}

/// The window sizes to offer: the primary monitor's modes, largest
/// first, else common ones.
pub fn resolutions(monitors: &[(UVec2, Vec<UVec2>)]) -> Vec<String> {
    let mut sizes: Vec<UVec2> = monitors.iter().flat_map(|(_, modes)| modes.iter().copied()).collect();
    if sizes.is_empty() {
        sizes = [
            (800, 600),
            (1024, 768),
            (1280, 720),
            (1280, 1024),
            (1366, 768),
            (1600, 900),
            (1920, 1080),
            (2560, 1440),
        ]
        .map(|(w, h)| UVec2::new(w, h))
        .to_vec();
        if let Some((size, _)) = monitors.first() {
            sizes.retain(|s| s.x <= size.x && s.y <= size.y);
            sizes.push(*size);
        }
    }
    sizes.retain(|s| s.x >= 640 && s.y >= 480);
    sizes.sort_by_key(|s| (std::cmp::Reverse(s.x), std::cmp::Reverse(s.y)));
    sizes.dedup();
    sizes.iter().map(|s| format!("{}x{}", s.x, s.y)).collect()
}

/// The primary monitor's size and modes (for `resolutions`).
pub fn monitor_modes(monitors: &Query<&bevy::window::Monitor, With<PrimaryMonitor>>) -> Vec<(UVec2, Vec<UVec2>)> {
    monitors
        .iter()
        .map(|m| {
            (
                UVec2::new(m.physical_width, m.physical_height),
                m.video_modes.iter().map(|v| v.physical_size).collect(),
            )
        })
        .collect()
}

/// Set the window as the video cvars say, when they change. Runs given a
/// window size (`--window`) or automated (screenshots, captures) leave the
/// window alone.
fn apply_video(
    settings: Res<VideoSettings>,
    args: Option<Res<crate::client::ClientArgs>>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    monitors: Query<&bevy::window::Monitor, With<PrimaryMonitor>>,
) {
    if !settings.is_changed() {
        return;
    }
    if args.is_some_and(|a| a.0.window.is_some() || a.0.screenshot.is_some() || a.0.views.is_some()) {
        return;
    }
    let Ok(mut window) = window.single_mut() else { return };
    let present = if settings.vsync != 0 {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
    if window.present_mode != present {
        window.present_mode = present;
    }
    let size = parse_resolution(&settings.resolution);
    let mode = match settings.fullscreen {
        1 => {
            // The monitor's mode of that size, else its current one.
            let modes = monitors
                .iter()
                .flat_map(|m| m.video_modes.iter())
                .filter(|v| Some(v.physical_size) == size);
            let best = modes.max_by_key(|v| (v.refresh_rate_millihertz, v.bit_depth)).copied();
            WindowMode::Fullscreen(
                MonitorSelection::Current,
                best.map_or(VideoModeSelection::Current, VideoModeSelection::Specific),
            )
        }
        2 => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        _ => WindowMode::Windowed,
    };
    if window.mode != mode {
        window.mode = mode;
    }
    if settings.fullscreen != 2
        && let Some(size) = size
        && window.resolution.physical_size() != size
    {
        window.resolution.set_physical_resolution(size.x, size.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Texture detail and filtering set a map texture's sampler; pictures
    /// that clamp (HUD, UI) or have no mips are left alone.
    #[test]
    fn texture_settings_set_map_samplers() {
        use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
        let texture = |address, levels| {
            let mut image = Image::default();
            image.texture_descriptor.mip_level_count = levels;
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: address,
                address_mode_v: address,
                mipmap_filter: ImageFilterMode::Linear,
                ..ImageSamplerDescriptor::linear()
            });
            image
        };
        let sampler = |image: &Image| match &image.sampler {
            ImageSampler::Descriptor(d) => (d.mipmap_filter, d.anisotropy_clamp, d.lod_min_clamp),
            _ => panic!(),
        };
        // CS:S's defaults: bilinear (nearest mip), no anisotropy, full size.
        let mut map = texture(ImageAddressMode::Repeat, 8);
        assert!(TextureSettings::default().apply(&mut map));
        assert_eq!(sampler(&map), (ImageFilterMode::Nearest, 1, 0.0));
        // Low detail, anisotropic 8x (which needs the mip filter linear).
        let low = TextureSettings {
            picmip: 2,
            trilinear: 0,
            aniso: 8,
        };
        assert!(low.apply(&mut map));
        assert_eq!(sampler(&map), (ImageFilterMode::Linear, 8, 2.0));
        // Never past the last mip.
        let mut small = texture(ImageAddressMode::Repeat, 2);
        TextureSettings { picmip: 2, ..default() }.apply(&mut small);
        assert_eq!(sampler(&small).2, 1.0);
        // A HUD picture (clamped) and one without mips: untouched.
        let mut hud = texture(ImageAddressMode::ClampToEdge, 8);
        assert!(!low.apply(&mut hud));
        assert_eq!(sampler(&hud), (ImageFilterMode::Linear, 1, 0.0));
        assert!(!low.apply(&mut texture(ImageAddressMode::Repeat, 1)));
    }
}
