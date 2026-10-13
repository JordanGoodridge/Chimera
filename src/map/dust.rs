//! Dust mote simulation and drawing for `MapDust` volumes, per
//! specs/cs_source/sprites_dust.md 8-9: motes spawn at random points inside
//! the volume's brushes (10 tries in its bounds), drift (horizontal speed
//! dies out toward the wind, vertical stays), fade in and out over their
//! life and with view depth, and keep a constant size on screen (dust
//! clouds: a size in the world). Frozen volumes make their motes once, and
//! those never move, age or die. One mesh per volume, rebuilt each frame
//! facing the camera while it is shown and has motes in view.

use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};

use super::{MapDust, SkyboxCamera};

/// All dust shares this many motes at most (the game's particle limit).
const MAX_MOTES: usize = 2048;
/// Horizontal speed moves toward the wind (none) at this rate, m/s^2.
const WIND_PULL: f32 = 50.0 * 0.0254;

struct Mote {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    life: f32,
    size: f32,
}

#[derive(Component)]
pub(super) struct DustEmitter {
    pub dust: MapDust,
    pub mesh: Handle<Mesh>,
    motes: Vec<Mote>,
    pending: f32,
    rng: u64,
    /// A frozen volume made its motes.
    frozen_spawned: bool,
    /// The mesh was last written with no motes in it: not written again
    /// until some show (a modified mesh is uploaded again and makes every
    /// mesh entity re-check its pipeline that frame).
    drawn_empty: bool,
}

impl DustEmitter {
    pub fn new(dust: MapDust, mesh: Handle<Mesh>, seed: u64) -> Self {
        Self {
            dust,
            mesh,
            motes: Vec::new(),
            pending: 0.0,
            rng: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            frozen_spawned: false,
            drawn_empty: false,
        }
    }

    /// Uniform in 0..1 (xorshift).
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn between(&mut self, (a, b): (f32, f32)) -> f32 {
        a + (b - a) * self.random()
    }

    /// A spawn point (8): up to 10 tries at random in the bounds, the
    /// first inside one of the brushes (any point without brushes).
    fn spawn_point(&mut self) -> Option<Vec3> {
        let (min, max) = (self.dust.min, self.dust.max);
        for _ in 0..10 {
            let p = Vec3::new(
                min.x + (max.x - min.x) * self.random(),
                min.y + (max.y - min.y) * self.random(),
                min.z + (max.z - min.z) * self.random(),
            );
            if self.dust.brushes.is_empty() || self.dust.brushes.iter().any(|b| b.iter().all(|(n, d)| n.dot(p) <= *d)) {
                return Some(p);
            }
        }
        None
    }
}

/// A mote's half-width (9): screen-constant (size / 10000 x depth), or a
/// dust cloud's size in the world.
fn half_width(dust: &MapDust, size: f32, depth: f32) -> f32 {
    if dust.world_size { size } else { size / 10000.0 * depth }
}

/// A mote's alpha before the volume's own (9): its life's fade in and out
/// (frozen motes: 1) times the fade with view depth; None past DistMax.
fn mote_alpha(dust: &MapDust, age: f32, life: f32, depth: f32) -> Option<f32> {
    if depth <= 0.0 || depth >= dust.fade_distance {
        return None;
    }
    let life_alpha = if dust.frozen {
        1.0
    } else {
        0.5 - 0.5 * (std::f32::consts::TAU * age / life).cos()
    };
    Some(life_alpha * (1.0 - depth / dust.fade_distance))
}

/// A mesh for an emitter to fill each frame: one invisible triangle, since
/// Bevy's mesh allocator logs a use-after-free for meshes with no vertices
/// (it skips allocating them but still copies their data).
pub(super) fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}

/// Write an emitter's quads into its mesh (dust motes, particles).
///
/// Bevy's mesh allocator frees a modified mesh and allocates it again in
/// the GPU buffer (slab) it shares with other meshes of its layout; a mesh
/// whose size changed every frame made that cost 3.5-4.5 ms of the render
/// world per frame on de_dust2 for one dust volume
/// (`allocate_and_free_meshes`; presumably the new size not fitting the
/// hole the old one left, so the slab, which holds the whole map's
/// indices, is reallocated and copied; docs/performance.md). So the
/// buffers are padded to a power of two (at least 64 vertices) with
/// degenerate triangles: the size changes rarely. An empty mesh gets the
/// padding too (Bevy's allocator logs a use-after-free for meshes without
/// vertices).
pub(super) fn write_dynamic_mesh(
    mesh: &mut Mesh,
    normal: Vec3,
    mut positions: Vec<[f32; 3]>,
    mut uvs: Vec<[f32; 2]>,
    mut colors: Vec<[f32; 4]>,
    mut indices: Vec<u32>,
) {
    let vertices = positions.len().max(1).next_power_of_two().max(MIN_DYNAMIC_VERTICES);
    // Six indices per quad of four vertices.
    let index_count = (vertices / 2 * 3).max(indices.len().next_power_of_two());
    // Padding sits on a real vertex (bounds stay put), fully transparent.
    let at = positions.first().copied().unwrap_or([0.0; 3]);
    positions.resize(vertices, at);
    uvs.resize(vertices, [0.0; 2]);
    colors.resize(vertices, [0.0; 4]);
    indices.resize(index_count, 0);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![normal.to_array(); vertices]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

/// Smallest vertex count `write_dynamic_mesh` writes.
pub(super) const MIN_DYNAMIC_VERTICES: usize = 64;

#[allow(clippy::type_complexity)]
pub(super) fn update_dust(
    time: Res<Time>,
    cameras: Query<
        &GlobalTransform,
        (
            With<Camera3d>,
            Without<super::monitor::ScreenCamera>,
            Without<SkyboxCamera>,
            Without<super::ViewModelCamera>,
            Without<super::water::WaterReflectionCamera>,
        ),
    >,
    mut emitters: Query<(&mut DustEmitter, Option<&super::EntityPart>, Option<&Visibility>)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs();
    let Some(eye) = cameras.iter().next() else { return };
    let (from, forward, right, up) = (
        eye.translation(),
        eye.forward().as_vec3(),
        eye.right().as_vec3(),
        eye.up().as_vec3(),
    );
    let mut total: usize = emitters.iter().map(|(e, ..)| e.motes.len()).sum();
    for (mut e, part, visibility) in &mut emitters {
        // Turned off (TurnOff): no new motes, the others live out their
        // life; removed (Kill): gone at once.
        if part.is_some_and(|p| !p.exists) {
            e.motes.clear();
        }
        let spawning = part.is_none_or(|p| p.on && p.exists);
        // Spawn (frame time for spawning is clamped to 0.1 s). Frozen:
        // `rate` tries once, when first switched on.
        if spawning && e.dust.frozen {
            if !e.frozen_spawned {
                e.frozen_spawned = true;
                e.pending = e.dust.rate.max(0.0).floor();
            }
        } else if spawning {
            e.pending += e.dust.rate * dt.min(0.1);
        } else {
            e.pending = 0.0;
        }
        while e.pending >= 1.0 {
            e.pending -= 1.0;
            if total >= MAX_MOTES {
                continue;
            }
            let Some(position) = e.spawn_point() else { continue };
            let speed = e.dust.speed;
            let velocity = Vec3::new(
                (e.random() * 2.0 - 1.0) * speed,
                (e.random() * 2.0 - 1.0) * speed,
                (e.random() * 2.0 - 1.0) * speed,
            );
            let (life_range, size_range) = (e.dust.life, e.dust.size);
            let life = e.between(life_range).max(0.01);
            let size = e.between(size_range);
            e.motes.push(Mote {
                position,
                velocity,
                age: 0.0,
                life,
                size,
            });
            total += 1;
        }
        // Move and age (frozen motes do neither).
        let pull = WIND_PULL * dt;
        let frozen = e.dust.frozen;
        e.motes.retain_mut(|m| {
            if frozen {
                return true;
            }
            let horizontal = Vec2::new(m.velocity.x, m.velocity.z);
            let slowed = horizontal.clamp_length_max((horizontal.length() - pull).max(0.0));
            m.velocity.x = slowed.x;
            m.velocity.z = slowed.y;
            m.position += m.velocity * dt;
            m.age += dt;
            m.age < m.life
        });
        // Hidden (culled by `vis`, or switched off): the mesh waits.
        if visibility == Some(&Visibility::Hidden) {
            continue;
        }
        // Draw: camera-facing quads, far to near.
        let mut visible: Vec<(f32, f32, &Mote)> = e
            .motes
            .iter()
            .filter_map(|m| {
                let depth = (m.position - from).dot(forward);
                Some((depth, mote_alpha(&e.dust, m.age, m.life, depth)?, m))
            })
            .collect();
        visible.sort_by(|a, b| b.0.total_cmp(&a.0));
        let color = Color::srgb(e.dust.color[0], e.dust.color[1], e.dust.color[2]).to_linear();
        let (mut positions, mut uvs, mut colors, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (depth, alpha, m) in visible {
            let alpha = alpha * e.dust.color[3];
            let half = half_width(&e.dust, m.size, depth);
            let base = positions.len() as u32;
            for (cx, cy, u, v) in [
                (-1.0, -1.0, 0.0, 1.0),
                (-1.0, 1.0, 0.0, 0.0),
                (1.0, 1.0, 1.0, 0.0),
                (1.0, -1.0, 1.0, 1.0),
            ] {
                positions.push((m.position + (right * cx + up * cy) * half).to_array());
                uvs.push([u, v]);
                colors.push([color.red, color.green, color.blue, alpha]);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        if positions.is_empty() && e.drawn_empty {
            continue;
        }
        e.drawn_empty = positions.is_empty();
        if let Some(mut mesh) = meshes.get_mut(&e.mesh) {
            write_dynamic_mesh(&mut mesh, -forward, positions, uvs, colors, indices);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dust() -> MapDust {
        MapDust {
            min: Vec3::ZERO,
            max: Vec3::splat(2.0),
            brushes: Vec::new(),
            rate: 10.0,
            size: (64.0 * 0.0254, 128.0 * 0.0254),
            world_size: true,
            frozen: false,
            speed: 0.1,
            life: (3.0, 5.0),
            fade_distance: 10.0,
            color: [1.0; 4],
            texture: None,
            entity: None,
            start_on: true,
        }
    }

    #[test]
    fn clouds_are_sized_in_the_world_and_motes_on_screen() {
        let mut d = dust();
        // A cloud's mote: its size at any depth.
        assert_eq!(half_width(&d, 1.0, 5.0), 1.0);
        // A mote's: size / 10000 x depth (constant on screen).
        d.world_size = false;
        assert!((half_width(&d, 20.0, 5.0) - 0.01).abs() < 1e-6);
        assert!((half_width(&d, 20.0, 10.0) - 0.02).abs() < 1e-6);
    }

    #[test]
    fn alpha_fades_with_life_and_depth_frozen_ones_by_depth_only() {
        let mut d = dust();
        // Mid-life at half DistMax: 1 x 0.5; at birth 0; past DistMax none.
        assert!((mote_alpha(&d, 2.0, 4.0, 5.0).unwrap() - 0.5).abs() < 1e-6);
        assert!(mote_alpha(&d, 0.0, 4.0, 5.0).unwrap().abs() < 1e-6);
        assert_eq!(mote_alpha(&d, 2.0, 4.0, 10.5), None);
        assert_eq!(mote_alpha(&d, 2.0, 4.0, -1.0), None);
        d.frozen = true;
        assert!((mote_alpha(&d, 0.0, 4.0, 5.0).unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn motes_start_inside_the_brushes() {
        let mut d = dust();
        // One brush: the bounds' lower x quarter (x <= 0.5 of 0..2).
        d.brushes = vec![vec![(Vec3::X, 0.5)]];
        let mut e = DustEmitter::new(d, Handle::default(), 7);
        let mut found = 0;
        for _ in 0..200 {
            if let Some(p) = e.spawn_point() {
                assert!(p.x <= 0.5, "{p}");
                found += 1;
            }
        }
        // Ten tries at a quarter of the box nearly always find it.
        assert!(found > 180, "{found}");
        // No brushes: anywhere in the bounds.
        e.dust.brushes.clear();
        assert!(e.spawn_point().is_some());
    }

    #[test]
    fn frozen_volumes_spawn_once_and_hold() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.add_systems(Update, update_dust);
        app.world_mut().spawn((Camera3d::default(), GlobalTransform::default()));
        let mut d = dust();
        d.frozen = true;
        d.min = Vec3::new(-1.0, -1.0, -5.0);
        d.max = Vec3::new(1.0, 1.0, -3.0);
        let emitter = app.world_mut().spawn(DustEmitter::new(d, Handle::default(), 3)).id();
        let motes = |app: &App| {
            app.world()
                .get::<DustEmitter>(emitter)
                .unwrap()
                .motes
                .iter()
                .map(|m| (m.position, m.age))
                .collect::<Vec<_>>()
        };
        app.update();
        let first = motes(&app);
        assert_eq!(first.len(), 10, "SpawnRate motes at once");
        for _ in 0..5 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.update();
        }
        assert_eq!(motes(&app), first, "no new motes; none moved or aged");
    }
}
