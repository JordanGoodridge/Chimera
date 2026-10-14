//! The sky: the 2D sky cubemap's image, the 3D skybox's camera following
//! the main one, and the render layers that separate them.

#[allow(unused_imports)]
use super::*;

/// Render layer for 3D skybox content.
pub const SKYBOX_LAYER: usize = 1;

/// Draws the 3D skybox; follows the main camera.
#[derive(Component)]
pub struct SkyboxCamera;

#[derive(Resource, Clone)]
pub(crate) struct SkyCameraInfo(pub(crate) MapSkyCamera);

/// The map's sky visibility, when it has one (see `MapSkyVis`).
#[derive(Resource)]
pub(crate) struct SkyVis(pub(crate) MapSkyVis);

/// The 3D skybox also exists where it was built, outside the playable area
/// (as in Source, where you can noclip to it). Drawn there only while the
/// camera is outside the playable area too: without the game's visibility
/// data, that keeps it from floating in view from inside the map.
#[allow(clippy::type_complexity)]
pub(crate) fn show_skybox_in_place(
    area: Option<Res<PlayableArea>>,
    cameras: Query<
        (&GlobalTransform, &Camera),
        (
            With<Camera3d>,
            Without<monitor::ScreenCamera>,
            Without<SkyboxCamera>,
            Without<ViewModelCamera>,
            Without<water::WaterReflectionCamera>,
        ),
    >,
    mut parts: Query<&mut bevy::camera::visibility::RenderLayers, Without<Camera>>,
    mut outside_before: Local<Option<bool>>,
) {
    let Some(area) = area else { return };
    let Some((eye, _)) = cameras.iter().find(|(_, c)| c.is_active) else {
        return;
    };
    let p = eye.translation();
    let (lo, hi) = area.0;
    let outside = p.cmplt(lo).any() || p.cmpgt(hi).any();
    if *outside_before == Some(outside) {
        return;
    }
    *outside_before = Some(outside);
    let sky = bevy::camera::visibility::RenderLayers::layer(SKYBOX_LAYER);
    let both = bevy::camera::visibility::RenderLayers::from_layers(&[0, SKYBOX_LAYER]);
    for mut layers in &mut parts {
        if layers.intersects(&sky) {
            *layers = if outside { both.clone() } else { sky.clone() };
        }
    }
}

/// Set when the map has a real 3D skybox (not just the 2D-sky camera).
#[derive(Resource)]
pub(crate) struct ActiveMapHas3dSky;

/// Holds nothing: the sky camera's layer when only the 2D sky shows.
pub(crate) const EMPTY_LAYER: usize = 31;

/// The map's sky cubemap, for cameras to show.
#[derive(Resource, Clone)]
pub struct MapSkybox(pub Handle<Image>);

/// Give every 3D camera the map's sky.
#[allow(clippy::type_complexity)]
pub(crate) fn attach_sky(
    mut commands: Commands,
    sky: Option<Res<MapSkybox>>,
    sky_camera: Option<Res<SkyCameraInfo>>,
    cameras: Query<
        (Entity, Has<SkyboxCamera>),
        (
            With<Camera3d>,
            Without<monitor::ScreenCamera>,
            Without<bevy::light::Skybox>,
            Without<ViewModelCamera>,
        ),
    >,
) {
    let Some(sky) = sky else { return };
    for (cam, is_sky_camera) in &cameras {
        // With a 3D skybox, the 2D sky is drawn behind it by the sky camera.
        if sky_camera.is_some() && !is_sky_camera {
            continue;
        }
        commands.entity(cam).insert(bevy::light::Skybox {
            image: Some(sky.0.clone()),
            // Show the texture at its own brightness (cancel camera exposure).
            brightness: LIGHTMAP_EXPOSURE,
            ..default()
        });
    }
}

/// Six faces into one cube texture. Faces narrower than tall or shorter
/// than wide (Source sides are often half height) are stretched to square.
pub(crate) fn sky_image(sky: &MapSky, textures: &[MapTexture]) -> Image {
    if let Some(faces) = sky.hdr.as_deref()
        && std::env::var_os("MASHUP_SKY_DEBUG").is_none()
    {
        return hdr_sky_image(sky, faces);
    }
    let size = sky
        .faces
        .iter()
        .map(|(t, _)| textures[*t].width.max(textures[*t].height))
        .max()
        .unwrap_or(1);
    let mut data = Vec::with_capacity((size * size * 4 * 6) as usize);
    // Calibration hook for `refcmp skyconv`: each pixel encodes its own
    // face (blue) and position (red = u, green = v).
    let debug = std::env::var_os("MASHUP_SKY_DEBUG").is_some();
    for (face, (tex, turns)) in sky.faces.into_iter().enumerate() {
        if debug {
            for y in 0..size {
                for x in 0..size {
                    data.extend_from_slice(&[
                        (x * 255 / size) as u8,
                        (y * 255 / size) as u8,
                        face as u8 * 40 + 20,
                        255,
                    ]);
                }
            }
            continue;
        }
        let t = &textures[tex];
        for y in 0..size {
            for x in 0..size {
                let (sx, sy) = sky_texel(x, y, size, turns, &sky.transforms[face], t.width, t.height);
                let i = ((sy * t.width + sx) * 4) as usize;
                data.extend_from_slice(&t.rgba8[i..i + 4]);
            }
        }
    }
    sky_cube_image(size, data, TextureFormat::Rgba8UnormSrgb)
}

/// How badly one cube layer of the (LDR) sky joins its four neighbours:
/// the mean colour difference (0-255) across its edges, each side sampled
/// just inside its own layer and just inside the neighbour, as a GPU
/// samples the cube. Each layer's orientation (`MapSky::faces`) decides
/// how the textures meet, so a misturned face shows as a large error.
pub fn sky_seam_error(sky: &MapSky, textures: &[MapTexture], layer: usize) -> f32 {
    let mut sky = sky.clone();
    sky.hdr = None;
    let image = sky_image(&sky, textures);
    let size = image.width() as usize;
    let Some(data) = image.data.as_ref() else { return 0.0 };
    // A direction's layer and texel, by the GPU cube convention.
    let sample = |d: Vec3| -> [f32; 3] {
        let a = d.abs();
        let (l, u, v, m) = if a.x >= a.y && a.x >= a.z {
            if d.x > 0.0 {
                (0, -d.z, -d.y, a.x)
            } else {
                (1, d.z, -d.y, a.x)
            }
        } else if a.y >= a.z {
            if d.y > 0.0 {
                (2, d.x, d.z, a.y)
            } else {
                (3, d.x, -d.z, a.y)
            }
        } else if d.z > 0.0 {
            (4, d.x, -d.y, a.z)
        } else {
            (5, -d.x, -d.y, a.z)
        };
        let px = |c: f32| (((c / m + 1.0) / 2.0 * size as f32) as usize).min(size - 1);
        let i = ((l * size + px(v)) * size + px(u)) * 4;
        [data[i] as f32, data[i + 1] as f32, data[i + 2] as f32]
    };
    let axis = layer / 2;
    let sign = if layer % 2 == 0 { 1.0 } else { -1.0 };
    let eps = 1.5 / size as f32;
    let (mut total, mut n) = (0.0, 0);
    for other in (0..3).filter(|a| *a != axis) {
        let third = 3 - axis - other;
        for edge in [-1.0f32, 1.0] {
            for k in 0..64 {
                let t = (k as f32 + 0.5) / 64.0 * 2.0 - 1.0;
                let point = |main: f32, side: f32| {
                    let mut p = Vec3::ZERO;
                    p[axis] = sign * main;
                    p[other] = edge * side;
                    p[third] = t * (1.0 - eps);
                    p
                };
                let inside = sample(point(1.0, 1.0 - eps));
                let outside = sample(point(1.0 - eps, 1.0));
                total += (0..3).map(|c| (inside[c] - outside[c]).abs()).sum::<f32>() / 3.0;
                n += 1;
            }
        }
    }
    total / n as f32
}

/// Which texel of a `w` x `h` sky face lands at (x, y) of a `size` cube
/// face, the face's coordinates through its material's `transform`
/// (clamped to the texture, as the sky samples it).
pub(crate) fn sky_texel(
    x: u32,
    y: u32,
    size: u32,
    turns: u8,
    transform: &MapUvTransform,
    w: u32,
    h: u32,
) -> (u32, u32) {
    // Rotate clockwise by `turns` quarter turns: sample the source
    // pixel that lands at (x, y).
    let (mut u, mut v) = (x, y);
    for _ in 0..turns % 4 {
        (u, v) = (v, size - 1 - u);
    }
    if turns >= 4 {
        u = size - 1 - u;
    }
    // Skip each face's outermost texel row and column: sky
    // textures leave them as borders (dust2's side faces end in
    // a black row) that the game never shows, while a cube map
    // blends them in at every seam.
    let inner = |p: u32, n: u32| 1 + (p as u64 * n.saturating_sub(2) as u64 / size as u64) as u32;
    if transform.is_identity() {
        return (inner(u, w).min(w - 1), inner(v, h).min(h - 1));
    }
    let face_uv = Vec2::new((u as f32 + 0.5) / size as f32, (v as f32 + 0.5) / size as f32);
    let t = transform.apply(face_uv, 0.0).clamp(Vec2::ZERO, Vec2::ONE);
    let texel = |c: f32, n: u32| ((c * n.saturating_sub(2) as f32) as u32 + 1).min(n.saturating_sub(2).max(1));
    (texel(t.x, w).min(w - 1), texel(t.y, h).min(h - 1))
}

/// The HDR sky (`MapSky::hdr`) as a linear half-float cube map.
pub(crate) fn hdr_sky_image(sky: &MapSky, faces: &[MapHdrImage; 6]) -> Image {
    let size = faces.iter().map(|f| f.width.max(f.height)).max().unwrap_or(1).max(1);
    let mut data = Vec::with_capacity((size * size * 8 * 6) as usize);
    for ((face, (_, turns)), transform) in faces.iter().zip(sky.faces).zip(&sky.transforms) {
        for y in 0..size {
            for x in 0..size {
                let [r, g, b] = if face.width == 0 || face.height == 0 {
                    [0.0; 3]
                } else {
                    let (sx, sy) = sky_texel(x, y, size, turns, transform, face.width, face.height);
                    face.rgb[(sy * face.width + sx) as usize]
                };
                for c in [r, g, b, 1.0] {
                    data.extend_from_slice(&half::f16::from_f32(c).to_le_bytes());
                }
            }
        }
    }
    sky_cube_image(size, data, TextureFormat::Rgba16Float)
}

pub(crate) fn sky_cube_image(size: u32, data: Vec<u8>, format: TextureFormat) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(bevy::render::render_resource::TextureViewDescriptor {
        dimension: Some(bevy::render::render_resource::TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::linear();
    image
}

/// World brushes are also drawn by water reflections (props and characters
/// only when the reflection shows entities).
pub(crate) fn world_layer_of(skybox: bool) -> bevy::camera::visibility::RenderLayers {
    if skybox { layer_of(true) } else { water::world_layers() }
}

pub(crate) fn layer_of(skybox: bool) -> bevy::camera::visibility::RenderLayers {
    bevy::camera::visibility::RenderLayers::layer(if skybox { SKYBOX_LAYER } else { 0 })
}

/// Keep a sky camera behind each frame's main camera: at
/// `origin + eye / scale`, same rotation, projection and render target. The
/// main camera then draws the world over it without clearing.
#[allow(clippy::type_complexity)]
pub(crate) fn follow_sky_camera(
    mut commands: Commands,
    info: Option<Res<SkyCameraInfo>>,
    vis: Option<Res<SkyVis>>,
    has_3d_sky: Option<Res<ActiveMapHas3dSky>>,
    water_view: Option<Res<water::WaterView>>,
    main: Query<
        (
            &GlobalTransform,
            &Projection,
            Option<&bevy::camera::RenderTarget>,
            Option<&bevy::core_pipeline::tonemapping::Tonemapping>,
            Entity,
            &Camera,
        ),
        (
            With<Camera3d>,
            Without<monitor::ScreenCamera>,
            Without<SkyboxCamera>,
            Without<ViewModelCamera>,
            Without<water::WaterReflectionCamera>,
        ),
    >,
    mut sky: Query<
        (
            Entity,
            &mut Transform,
            &mut GlobalTransform,
            &mut Projection,
            &mut Camera,
            &mut bevy::camera::visibility::RenderLayers,
            Option<&bevy::camera::RenderTarget>,
            Option<&bevy::core_pipeline::tonemapping::Tonemapping>,
        ),
        With<SkyboxCamera>,
    >,
) {
    let Some(info) = info else { return };
    let Some((main_tf, projection, target, tonemapping, main_entity, main_camera)) =
        main.iter().find(|(.., c)| c.is_active)
    else {
        return;
    };
    let eye = main_tf.translation();
    let translation = info.0.origin + eye / info.0.scale;
    let rotation = main_tf.rotation();
    // Source: the sky is drawn only from leaves that see it; inside solid
    // the view clears to black (specs/cs_source/shadows_sky.md). Leaves
    // that see no sky show the previous frame in the game; black here.
    let leaf = vis.as_ref().map_or(LeafSky::Sky3d, |v| v.0.at(eye));
    // Under water the view clears to the water's fog colour and draws no
    // sky (specs/cs_source/water.md section 9).
    let under = water_view.as_ref().and_then(|w| w.clear);
    let sky_on = matches!(leaf, LeafSky::Sky2d | LeafSky::Sky3d) && under.is_none();
    let layer = if leaf == LeafSky::Sky3d && has_3d_sky.is_some() {
        SKYBOX_LAYER
    } else {
        EMPTY_LAYER
    };
    let clear = match (sky_on, under) {
        (true, _) => ClearColorConfig::None,
        (false, Some([r, g, b])) => ClearColorConfig::Custom(Color::srgb(r, g, b)),
        (false, None) => ClearColorConfig::Custom(Color::BLACK),
    };
    let same = match (main_camera.clear_color, clear) {
        (ClearColorConfig::None, ClearColorConfig::None) => true,
        (ClearColorConfig::Custom(a), ClearColorConfig::Custom(b)) => a == b,
        _ => false,
    };
    if !same {
        commands.entity(main_entity).insert(Camera {
            clear_color: clear,
            ..main_camera.clone()
        });
    }
    match sky.single_mut() {
        Ok((entity, mut tf, mut global, mut proj, mut camera, mut layers, sky_target, sky_tonemapping)) => {
            if camera.is_active != sky_on {
                camera.is_active = sky_on;
            }
            if !layers.intersects(&bevy::camera::visibility::RenderLayers::layer(layer)) {
                *layers = bevy::camera::visibility::RenderLayers::layer(layer);
            }
            tf.translation = translation;
            tf.rotation = rotation;
            // Propagation has run: set the global transform too (no parent).
            *global = GlobalTransform::from(*tf);
            // Only real changes: a changed projection, target or tonemapping
            // makes Bevy redo the camera's setup.
            if let (Projection::Perspective(p), Projection::Perspective(main_p)) = (&*proj, projection)
                && (p.fov != main_p.fov || p.aspect_ratio != main_p.aspect_ratio)
                && let Projection::Perspective(p) = &mut *proj
            {
                p.fov = main_p.fov;
                p.aspect_ratio = main_p.aspect_ratio;
            }
            let mut e = commands.entity(entity);
            if let Some(t) = target
                && sky_target.and_then(|c| c.normalize(None)) != t.normalize(None)
            {
                e.insert(t.clone());
            }
            if let Some(t) = tonemapping
                && sky_tonemapping != Some(t)
            {
                e.insert(*t);
            }
        }
        Err(_) => {
            let mut e = commands.spawn((
                Name::new("Sky camera"),
                SkyboxCamera,
                MapPart,
                Camera3d::default(),
                Camera {
                    order: main_camera.order - 1,
                    ..default()
                },
                Projection::Perspective(PerspectiveProjection {
                    near: 0.01,
                    ..default()
                }),
                Transform::from_translation(translation).with_rotation(rotation),
                bevy::camera::visibility::RenderLayers::layer(layer),
            ));
            if let Some(fog) = &info.0.fog {
                let [r, g, b] = fog.color;
                e.insert(bevy::pbr::DistanceFog {
                    color: Color::srgb(r, g, b),
                    falloff: bevy::pbr::FogFalloff::Linear {
                        start: fog.start / info.0.scale,
                        end: fog.end / info.0.scale,
                    },
                    ..default()
                });
            }
        }
    }
}
