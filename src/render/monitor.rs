//! Monitors (Source's func_monitor showing a point_camera's view, public
//! entity docs), for any game: the map's monitor screen texture
//! (`MapData::camera_texture`) is a render target that a camera of our
//! own draws into from the active camera's pose (`map::MonitorCamera`,
//! which the logic writes), only while one is active. What the camera sees
//! is culled for the player's own view (`vis`), as the rest of the map.
//! The sky shows behind it as in the main view: a sky camera of its own
//! (`map::monitor::ScreenSkyCamera`) draws the 2D sky and the 3D skybox
//! from the monitor camera's place scaled into it, first. Its fog is the
//! point_camera's own (`map::MonitorFog`), not the map's.
//!
//! Monitors for any game (Source's func_monitor and point_camera): the map's
//! monitor screen texture (`MapData::camera_texture`, Source `_rt_Camera` /
//! `engine/camerarendertarget`) is a 512² render target (repeating, as monitor
//! faces' coordinates sit whole repeats from 0) that a camera of ours
//! (`ScreenCamera`) draws into from the active camera's pose, only while one is
//! active, over a sky camera of its own (`ScreenSkyCamera`, also a
//! `ScreenCamera`: the 2D sky and the 3D skybox from the camera's place scaled
//! into it); with the point_camera's own fog.

use bevy::{
    camera::RenderTarget,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::TextureFormat,
};

use super::sky::{MapSkybox, SkyCameraInfo};
use crate::map::{
    EMPTY_LAYER, LIGHTMAP_EXPOSURE, MapPart, MonitorCamera, SKYBOX_LAYER,
    monitor::{ScreenCamera, ScreenSkyCamera},
};

/// The monitor screen's size, pixels.
pub const SCREEN_SIZE: u32 = 512;

/// The render target monitors show.
#[derive(Resource, Clone, Debug)]
pub struct MonitorScreen(pub Handle<Image>);

/// A render target image for the screen.
pub fn screen_image(images: &mut Assets<Image>) -> Handle<Image> {
    let mut image = Image::new_target_texture(
        SCREEN_SIZE,
        SCREEN_SIZE,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    );
    // Monitor faces' texture coordinates are whole repeats away from 0
    // (59..60): the picture repeats, as the game's textures do.
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    images.add(image)
}

/// Keep the screen camera where the active point_camera is, on only while
/// one is.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_screen_camera(
    screen: Option<Res<MonitorScreen>>,
    want: Res<MonitorCamera>,
    mut cameras: Query<
        (Entity, &mut Camera, &mut Transform, &mut Projection),
        (With<ScreenCamera>, Without<ScreenSkyCamera>),
    >,
    mut skies: Query<(Entity, &mut Camera, &mut Transform, &mut Projection), With<ScreenSkyCamera>>,
    sky_info: Option<Res<SkyCameraInfo>>,
    skybox: Option<Res<MapSkybox>>,
    mut commands: Commands,
) {
    let Some(screen) = screen else {
        for (e, ..) in cameras.iter().chain(skies.iter()) {
            commands.entity(e).despawn();
        }
        return;
    };
    // The camera's fog, else none (not the map's).
    let fog = want.1.unwrap_or_default().distance_fog(1.0);
    let Some((at, rotation, fov)) = want.0 else {
        for (_, mut c, ..) in cameras.iter_mut().chain(skies.iter_mut()) {
            if c.is_active {
                c.is_active = false;
            }
        }
        return;
    };
    // The sky camera: the 2D sky, and the 3D skybox from the monitor
    // camera's place scaled into it.
    let has_sky = skybox.is_some() || sky_info.is_some();
    if has_sky {
        let (sky_at, layer) = match &sky_info {
            Some(i) => (i.0.origin + at / i.0.scale, SKYBOX_LAYER),
            None => (at, EMPTY_LAYER),
        };
        let sky_projection = Projection::Perspective(PerspectiveProjection {
            fov: fov.clamp(1.0, 170.0).to_radians(),
            aspect_ratio: 1.0,
            near: 0.01,
            ..default()
        });
        match skies.single_mut() {
            Ok((_, mut c, mut t, mut p)) => {
                if !c.is_active {
                    c.is_active = true;
                }
                t.set_if_neq(Transform::from_translation(sky_at).with_rotation(rotation));
                if let (Projection::Perspective(now), Projection::Perspective(want)) = (&*p, &sky_projection)
                    && (now.fov - want.fov).abs() > 1e-4
                {
                    *p = sky_projection;
                }
            }
            Err(_) => {
                let mut e = commands.spawn((
                    Name::new("Monitor sky camera"),
                    ScreenSkyCamera,
                    ScreenCamera,
                    MapPart,
                    Camera3d::default(),
                    Camera {
                        order: -2,
                        clear_color: Color::BLACK.into(),
                        ..default()
                    },
                    RenderTarget::Image(screen.0.clone().into()),
                    sky_projection,
                    Transform::from_translation(sky_at).with_rotation(rotation),
                    bevy::camera::visibility::RenderLayers::layer(layer),
                ));
                if let Some(sky) = &skybox {
                    e.insert(bevy::light::Skybox {
                        image: Some(sky.0.clone()),
                        brightness: LIGHTMAP_EXPOSURE,
                        ..default()
                    });
                }
                if let Some(fog) = sky_info
                    .as_ref()
                    .and_then(|i| i.0.fog.as_ref().map(|f| (f.clone(), i.0.scale)))
                {
                    let ([r, g, b], scale) = (fog.0.color, fog.1);
                    e.insert(bevy::pbr::DistanceFog {
                        color: Color::srgb(r, g, b),
                        falloff: bevy::pbr::FogFalloff::Linear {
                            start: fog.0.start / scale,
                            end: fog.0.end / scale,
                        },
                        ..default()
                    });
                }
            }
        }
    }
    let projection = Projection::Perspective(PerspectiveProjection {
        fov: fov.clamp(1.0, 170.0).to_radians(),
        aspect_ratio: 1.0,
        ..default()
    });
    match cameras.single_mut() {
        Ok((e, mut c, mut t, mut p)) => {
            if !c.is_active {
                c.is_active = true;
            }
            if want.is_changed() {
                commands.entity(e).insert(fog);
            }
            t.set_if_neq(Transform::from_translation(at).with_rotation(rotation));
            let fov_now = match &*p {
                Projection::Perspective(pp) => pp.fov,
                _ => -1.0,
            };
            if (fov_now - fov.clamp(1.0, 170.0).to_radians()).abs() > 1e-4 {
                *p = projection;
            }
        }
        Err(_) => {
            commands.spawn((
                Name::new("Monitor camera"),
                ScreenCamera,
                MapPart,
                Camera3d::default(),
                Camera {
                    // Before the main view, which shows its picture; after
                    // its sky camera, whose picture it draws over.
                    order: -1,
                    clear_color: if has_sky {
                        ClearColorConfig::None
                    } else {
                        Color::BLACK.into()
                    },
                    ..default()
                },
                RenderTarget::Image(screen.0.clone().into()),
                fog,
                projection,
                Transform::from_translation(at).with_rotation(rotation),
            ));
        }
    }
}

/// Drives the screen cameras (`map_plugin` adds it right after map's own
/// registration, where the map's plugins were added).
pub struct MonitorPlugin;

impl Plugin for MonitorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, drive_screen_camera.run_if(resource_exists::<Assets<Image>>));
    }
}
