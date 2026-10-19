//! Drawing Combat Arms' effects (`game::effects`; specs/combat_arms/
//! effects_sounds.md): the textures come from the install
//! (`FxImages`: `.dtx`, a sprite's first frame), loaded once each.
//!
//! - Decals (6): flat quads of the surface's bullet-hole sprite, 64·k u
//!   wide, spun about the normal, grey `MarkColorRatio`, lit by the scene,
//!   alpha α(t) (`rules::mark_alpha`).
//! - Tracers (4): a strip `Width` wide from tail to head facing the camera,
//!   the record's texture and colour, alpha from `InitialAlpha` to
//!   `FinalAlpha`, not additive, drawn without depth test (Quirk 1:
//!   `OnTop`).
//! - Casings (8): the FireFX's casing model and skin (`guns_sc`) at
//!   `ShellScale` × the growth, else a small brass box.
//! - The first-person muzzle flash (3): the muzzle part's or weapon's
//!   `PVMuzzleFXName` group's particle (texture, size keys over its life,
//!   colour keys, additive, a random spin per shot) at the view model's
//!   flash place, in the view-model pass; a missing group shows nothing
//!   (grade C and Red silencers). Its light and every effect group's
//!   `DynaLight` are point lights without shadows (`lights`).
//! - Effect groups' particles are Chimera's pool's (drawn by the map).
//!
//! Known shortcuts: lights light what Bevy lights (the lightmapped world
//! may not show them); the camera jitter (`CaFx::jitter_at`) isn't added
//! to the view; decals aren't clipped to their polygon (as the game's
//! `MarksClip` 0) and lie 0.2 cm off the surface.

use std::collections::HashMap;

use bevy::{
    camera::visibility::RenderLayers,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    render::render_resource::{AsBindGroup, CompareFunction, RenderPipelineDescriptor, SpecializedMeshPipelineError},
};
use lithtech_formats::{dtx::Dtx, fxf::sample_keys};

use crate::{
    client::FirstPersonCamera,
    core::LocalPlayer,
    game::{
        attachments::CaMods,
        effects::{
            CaShell, CaTracers,
            fx::CaFx,
            data::CaFxTables,
            library::{self, normalize},
            pv_muzzle, rules,
        },
        inventory::UNIT,
        view_model::{CaViewModel, CaViewParams, VIEW_SCALE},
    },
    map::view_model::{VIEW_MODEL_LAYER, ViewModelCamera},
    weapon::Inventory,
};

pub struct CaEffectsDrawPlugin;

impl Plugin for CaEffectsDrawPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<OnTopMaterial>::default())
            .init_resource::<FxImages>()
            .add_systems(
                Update,
                (decals, tracers, casings, muzzle_flash, lights)
                    .after(crate::map::DriveAnimation)
                    .after(crate::game::effects::CaEffectsSet),
            );
    }
}

/// A material drawn over everything (no depth test): tracers.
pub type OnTopMaterial = ExtendedMaterial<StandardMaterial, OnTop>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct OnTop {}

impl MaterialExtension for OnTop {
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_compare = Some(CompareFunction::Always);
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// The install's effect textures as images (None: doesn't read).
#[derive(Resource, Default)]
pub struct FxImages(HashMap<String, Option<(Handle<Image>, UVec2)>>);

impl FxImages {
    /// A texture (`.dtx`) or a sprite's (`.spr`: its first frame's).
    pub fn get(&mut self, path: &str, images: &mut Assets<Image>) -> Option<(Handle<Image>, UVec2)> {
        let path = normalize(path);
        self.0
            .entry(path.clone())
            .or_insert_with(|| {
                let mount = library::install_mount()?;
                let tex = if path.ends_with(".spr") {
                    library::sprite_texture(&mount.read(&path).ok()?)?
                } else {
                    path.clone()
                };
                let dtx = Dtx::parse(mount.read(&tex).ok()?).ok()?;
                let size = UVec2::new(dtx.width, dtx.height);
                let image = Image::new(
                    bevy::render::render_resource::Extent3d {
                        width: dtx.width,
                        height: dtx.height,
                        depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    dtx.rgba(0),
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                );
                Some((images.add(image), size))
            })
            .clone()
    }
}

/// A unit quad in its XY plane facing +Z.
fn quad(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    meshes.add(Rectangle::new(1.0, 1.0))
}

// ---------------------------------------------------------------------------
// Decals

#[derive(Component)]
struct DecalQuad(usize);

#[allow(clippy::too_many_arguments)]
fn decals(
    marks: Res<rules::CaMarks>,
    mut drawn: Query<(Entity, &DecalQuad, &mut Transform, &MeshMaterial3d<StandardMaterial>, &mut Visibility)>,
    mut images: ResMut<Assets<Image>>,
    mut fx_images: ResMut<FxImages>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mesh: Local<Option<Handle<Mesh>>>,
    mut commands: Commands,
) {
    let mesh = mesh.get_or_insert_with(|| quad(&mut meshes)).clone();
    let mut slots: HashMap<usize, (Entity, Mut<Transform>, Handle<StandardMaterial>, Mut<Visibility>)> = drawn
        .iter_mut()
        .map(|(e, d, t, m, v)| (d.0, (e, t, m.0.clone(), v)))
        .collect();
    for (i, m) in marks.marks.iter().enumerate() {
        let Some(alpha) = m.alpha() else { continue };
        let n = m.normal.normalize_or(Vec3::Y);
        let rot = Quat::from_rotation_arc(Vec3::Z, n) * Quat::from_rotation_z(m.angle);
        let pose = Transform::from_translation(m.at * UNIT + n * 0.002)
            .with_rotation(rot)
            .with_scale(Vec3::splat(m.width * UNIT));
        let tex = fx_images.get(&m.sprite, &mut images).map(|t| t.0);
        let color = Color::srgba(m.grey * 2.0, m.grey * 2.0, m.grey * 2.0, alpha);
        match slots.remove(&i) {
            Some((_, mut t, mat, mut v)) => {
                *t = pose;
                *v = Visibility::Inherited;
                if let Some(mut mat) = materials.get_mut(&mat) {
                    mat.base_color = if mat.base_color_texture.is_some() { color } else { color.with_alpha(alpha * 0.8) };
                    if tex.is_some() && mat.base_color_texture != tex {
                        mat.base_color_texture = tex;
                    }
                }
            }
            None => {
                let has_tex = tex.is_some();
                commands.spawn((
                    Name::new("Decal"),
                    DecalQuad(i),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: if has_tex { color } else { Color::srgba(0.05, 0.05, 0.05, alpha * 0.8) },
                        base_color_texture: tex,
                        alpha_mode: AlphaMode::Blend,
                        perceptual_roughness: 1.0,
                        depth_bias: 100.0,
                        ..default()
                    })),
                    pose,
                    bevy::light::NotShadowCaster,
                ));
            }
        }
    }
    for (_, (_, _, _, mut v)) in slots {
        *v = Visibility::Hidden;
    }
}

// ---------------------------------------------------------------------------
// Tracers

#[derive(Component)]
struct TracerQuad(usize);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn tracers(
    list: Res<CaTracers>,
    camera: Option<Single<&GlobalTransform, With<FirstPersonCamera>>>,
    mut drawn: Query<(&TracerQuad, &mut Transform, &MeshMaterial3d<OnTopMaterial>, &mut Visibility)>,
    mut images: ResMut<Assets<Image>>,
    mut fx_images: ResMut<FxImages>,
    mut materials: ResMut<Assets<OnTopMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mesh: Local<Option<Handle<Mesh>>>,
    mut commands: Commands,
) {
    let mesh = mesh.get_or_insert_with(|| quad(&mut meshes)).clone();
    let eye = camera.map_or(Vec3::ZERO, |c| c.translation());
    let mut slots: HashMap<usize, (Mut<Transform>, Handle<OnTopMaterial>, Mut<Visibility>)> = drawn
        .iter_mut()
        .map(|(d, t, m, v)| (d.0, (t, m.0.clone(), v)))
        .collect();
    for (i, tr) in list.0.iter().enumerate() {
        let Some((tail, head)) = tr.path.ends(tr.age) else { continue };
        let (a, b) = (
            (tr.path.from + tr.path.dir * tail) * UNIT,
            (tr.path.from + tr.path.dir * head) * UNIT,
        );
        let mid = (a + b) / 2.0;
        let along = (b - a).normalize_or(Vec3::Z);
        let to_eye = (eye - mid).normalize_or(Vec3::Y);
        let side = along.cross(to_eye).normalize_or(Vec3::X);
        let normal = side.cross(along);
        let rot = Quat::from_mat3(&Mat3::from_cols(along, side, normal));
        let pose = Transform::from_translation(mid)
            .with_rotation(rot)
            .with_scale(Vec3::new((b - a).length().max(1e-4), tr.look.width * UNIT, 1.0));
        let life = tr.path.life().max(1e-4);
        let alpha = tr.look.initial_alpha + (tr.look.final_alpha - tr.look.initial_alpha) * (tr.age / life).min(1.0);
        let [r, g, bl] = tr.look.color.map(|c| c / 255.0);
        let color = Color::srgba(r, g, bl, alpha);
        let tex = fx_images.get(&tr.look.texture, &mut images).map(|t| t.0);
        match slots.remove(&i) {
            Some((mut t, mat, mut v)) => {
                *t = pose;
                *v = Visibility::Inherited;
                if let Some(mut m) = materials.get_mut(&mat) {
                    m.base.base_color = color;
                    if m.base.base_color_texture != tex {
                        m.base.base_color_texture = tex;
                    }
                }
            }
            None => {
                commands.spawn((
                    Name::new("Tracer"),
                    TracerQuad(i),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(materials.add(OnTopMaterial {
                        base: StandardMaterial {
                            base_color: color,
                            base_color_texture: tex,
                            alpha_mode: AlphaMode::Blend,
                            unlit: true,
                            double_sided: true,
                            cull_mode: None,
                            ..default()
                        },
                        extension: OnTop {},
                    })),
                    pose,
                    bevy::light::NotShadowCaster,
                ));
            }
        }
    }
    for (_, (_, _, mut v)) in slots {
        *v = Visibility::Hidden;
    }
}

// ---------------------------------------------------------------------------
// Casings

/// A casing's look: a mesh and material per FireFX record (the model's
/// first piece).
#[derive(Default)]
struct CasingLooks(HashMap<String, (Handle<Mesh>, Handle<StandardMaterial>)>);

#[allow(clippy::too_many_arguments)]
fn casings(
    new: Query<(Entity, &CaShell), Without<Mesh3d>>,
    mut shells: Query<(&CaShell, &mut Transform), With<Mesh3d>>,
    tables: Res<CaFxTables>,
    mut images: ResMut<Assets<Image>>,
    mut fx_images: ResMut<FxImages>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut looks: Local<CasingLooks>,
    mut commands: Commands,
) {
    for (e, s) in &new {
        let record = tables.fire(&s.fire).cloned().unwrap_or_default();
        let (mesh, material) = looks
            .0
            .entry(s.fire.clone())
            .or_insert_with(|| {
                let mesh = library::install_mount()
                    .and_then(|m| casing_mesh(&record.shell_model, &m))
                    .map(|m| meshes.add(m))
                    .unwrap_or_else(|| meshes.add(Cuboid::new(0.006, 0.006, 0.016)));
                let tex = (!record.shell_skin.is_empty())
                    .then(|| fx_images.get(&record.shell_skin, &mut images).map(|t| t.0))
                    .flatten();
                let material = materials.add(StandardMaterial {
                    base_color: if tex.is_some() { Color::WHITE } else { Color::srgb(0.8, 0.62, 0.25) },
                    base_color_texture: tex,
                    metallic: 0.6,
                    perceptual_roughness: 0.4,
                    ..default()
                });
                (mesh, material)
            })
            .clone();
        commands.entity(e).insert((Mesh3d(mesh), MeshMaterial3d(material), Visibility::default()));
    }
    for (s, mut t) in &mut shells {
        let base = tables.fire(&s.fire).map_or(1.0, |f| if f.shell_scale > 0.0 { f.shell_scale } else { 1.0 });
        t.scale = Vec3::splat(base * s.scale());
    }
}

/// A casing model's first piece as a mesh (metres).
fn casing_mesh(path: &str, mount: &crate::mount::Mount) -> Option<Mesh> {
    if path.is_empty() {
        return None;
    }
    let file = |p: &str| mount.read(p).ok();
    let loaded = crate::game::model::load(path, &file).ok()?;
    let model = crate::game::model::convert(&loaded, UNIT, &|_| String::new());
    let m = model.meshes.into_iter().find(|m| !m.positions.is_empty())?;
    let mut mesh = Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    let n = m.positions.len();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, m.positions);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        if m.normals.len() == n { m.normals } else { vec![[0.0, 1.0, 0.0]; n] },
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, if m.uvs.len() == n { m.uvs } else { vec![[0.0; 2]; n] });
    mesh.insert_indices(bevy::mesh::Indices::U32(m.indices));
    Some(mesh)
}

// ---------------------------------------------------------------------------
// The first-person muzzle flash

/// The flash quad, a child of the view-model camera, and its light.
#[derive(Component)]
struct FlashQuad;

#[derive(Component)]
struct FlashLight;

#[derive(Default)]
struct FlashState {
    shot: Option<f64>,
    roll: f32,
    seed: u32,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn muzzle_flash(
    time: Res<Time>,
    player: Option<Single<(&CaViewModel, &Inventory), With<LocalPlayer>>>,
    weapons: Query<(Option<&CaViewParams>, Option<&CaMods>)>,
    camera: Option<Single<Entity, With<ViewModelCamera>>>,
    tables: Res<CaFxTables>,
    mut quads: Query<(&mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), (With<FlashQuad>, Without<FlashLight>)>,
    mut light: Query<(&mut Transform, &mut PointLight, &mut Visibility), (With<FlashLight>, Without<FlashQuad>)>,
    mut images: ResMut<Assets<Image>>,
    mut fx_images: ResMut<FxImages>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut state: Local<FlashState>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let Ok((mut qt, mut qv, qm)) = quads.single_mut() else {
        let Some(camera) = camera else { return };
        let q = commands
            .spawn((
                FlashQuad,
                Mesh3d(quad(&mut meshes)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(1.0, 0.75, 0.4),
                    alpha_mode: AlphaMode::Add,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })),
                Transform::default(),
                Visibility::Hidden,
                RenderLayers::layer(VIEW_MODEL_LAYER),
                bevy::light::NotShadowCaster,
            ))
            .id();
        commands.entity(*camera).add_child(q);
        commands.spawn((
            FlashLight,
            PointLight {
                shadow_maps_enabled: false,
                intensity: 0.0,
                radius: 0.0,
                ..default()
            },
            Transform::default(),
            Visibility::Hidden,
            RenderLayers::from_layers(&[0, VIEW_MODEL_LAYER]),
        ));
        return;
    };
    let shown = player.as_ref().and_then(|p| {
        let (vm, inv) = (p.0, p.1);
        let at = vm.flash_at?;
        let (view, mods) = inv.active.and_then(|w| weapons.get(w).ok())?;
        let group = pv_muzzle(&tables, &view?.muzzle_fx, mods).to_string();
        Some((vm.clone(), at, group))
    });
    let Some((vm, at, group)) = shown else {
        *qv = Visibility::Hidden;
        if let Ok((_, _, mut v)) = light.single_mut() {
            *v = Visibility::Hidden;
        }
        state.shot = None;
        return;
    };
    if vm.flash.shot_at != state.shot {
        state.shot = vm.flash.shot_at;
        state.seed = state.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state.roll = (state.seed >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU;
    }
    let age = state.shot.map_or(0.0, |s| (now - s) as f32);
    let lib = library::library();
    // Without the effect file: a plain glow (no install); with it, the
    // group's particle, or nothing when the group is missing.
    let (size, color, tex, light_keys) = match lib.as_ref() {
        None => (0.016, Vec3::new(1.0, 0.75, 0.4), None, None),
        Some(lib) => {
            let Some(g) = lib.group(&group) else {
                *qv = Visibility::Hidden;
                return;
            };
            let p = g.keys.iter().find(|k| k.kind == "ParticleSystem");
            let l = g.keys.iter().find(|k| k.kind == "DynaLight");
            let Some(p) = p else {
                *qv = Visibility::Hidden;
                return;
            };
            let life = p.float("MaxParticleLifeSpan").unwrap_or(0.05).max(1e-3);
            let t = (age / life).min(1.0);
            let s = sample_keys(&p.scale_keys(), t, 0.5);
            let ck = p.color_keys();
            let ch = |i: usize| sample_keys(&ck.iter().map(|(k, c)| (*k, c[i])).collect::<Vec<_>>(), t, 1.0);
            let tex = lib.texture_of(p).and_then(|path| fx_images.get(&path, &mut images)).map(|t| t.0);
            // A particle of size s is a square of side 4 s view units.
            (4.0 * s * VIEW_SCALE, Vec3::new(ch(0), ch(1), ch(2)), tex, l.map(|l| (l.scale_keys(), l.color_keys(), l.end - l.start)))
        }
    };
    *qt = Transform::from_translation(at)
        .with_rotation(Quat::from_rotation_z(state.roll))
        .with_scale(Vec3::splat(size));
    *qv = Visibility::Inherited;
    if let Some(mut m) = materials.get_mut(&qm.0) {
        m.base_color = Color::srgb(color.x, color.y, color.z);
        if tex.is_some() && m.base_color_texture != tex {
            m.base_color_texture = tex;
        }
    }
    if let Ok((mut lt, mut pl, mut lv)) = light.single_mut() {
        match light_keys {
            Some((radius, color, life)) if age <= life => {
                let t = (age / life.max(1e-3)).min(1.0);
                let r = sample_keys(&radius, t, 0.0) * UNIT;
                let ch = |i: usize| sample_keys(&color.iter().map(|(k, c)| (*k, c[i])).collect::<Vec<_>>(), t, 1.0);
                lt.translation = vm.flash_point;
                pl.range = r.max(0.01);
                pl.color = Color::srgb(ch(0), ch(1), ch(2));
                pl.intensity = LIGHT_LUMENS;
                *lv = Visibility::Inherited;
            }
            _ => *lv = Visibility::Hidden,
        }
    }
}

/// Effect lights' intensity: Chimera's world shader reads colour ×
/// intensity / 4π as lightmap units (render::view_model's flash lights),
/// so a light adds its colour at its centre, falling to 0 at its radius.
const LIGHT_LUMENS: f32 = 4.0 * std::f32::consts::PI;

// ---------------------------------------------------------------------------
// Effect groups' lights

#[derive(Component)]
struct FxLightSlot(usize);

/// At most this many effect lights at once (the strongest).
const MAX_LIGHTS: usize = 8;

fn lights(fx: Res<CaFx>, mut slots: Query<(&FxLightSlot, &mut Transform, &mut PointLight, &mut Visibility)>, mut commands: Commands) {
    if slots.is_empty() {
        for i in 0..MAX_LIGHTS {
            commands.spawn((
                FxLightSlot(i),
                PointLight {
                    shadow_maps_enabled: false,
                    intensity: 0.0,
                    radius: 0.0,
                    ..default()
                },
                Transform::default(),
                Visibility::Hidden,
                RenderLayers::from_layers(&[0, VIEW_MODEL_LAYER]),
            ));
        }
        return;
    }
    let mut list: Vec<(f32, Vec3, Vec3)> = fx
        .lights
        .iter()
        .map(|l| (l.radius_now(), l.at, l.color_now()))
        .filter(|l| l.0 > 0.0)
        .collect();
    list.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (slot, mut t, mut pl, mut v) in &mut slots {
        match list.get(slot.0) {
            Some((r, at, c)) => {
                t.translation = *at;
                pl.range = r * UNIT;
                pl.color = Color::srgb(c.x, c.y, c.z);
                pl.intensity = LIGHT_LUMENS;
                *v = Visibility::Inherited;
            }
            None => *v = Visibility::Hidden,
        }
    }
}
