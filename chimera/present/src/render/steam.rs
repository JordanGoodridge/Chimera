//! Steam jets (`map::steam::MapSteam`, Source's env_steam:
//! specs/source/visual_entities.md 7): while on and in view, puffs leave
//! the jet's origin along its direction with a random sideways spread,
//! grow by the second, fade in and out over their life (sin), roll, and
//! take the light sampled along the jet. One mesh per jet, rebuilt each
//! frame facing the camera, like the dust volumes (`dust.rs`). Engine
//! space: meters, seconds.
//!
//! Steam jets for any game: puffs emitted while on and in view, rolled; one
//! camera-facing mesh per jet rebuilt per frame; the logic switches them
//! (`EntityPart`); headless they still puff, undrawn.
//!
//! Heat-wave jets (Type 1, `MapSteam::heat_wave`) puff the same way but draw
//! `HeatWaveMaterial` quads in the view plane (not rolled, size not truncated),
//! each showing the scene behind it shifted by a random direction picked per
//! puff each frame, fading to nothing at the quad's edge and with the puff's
//! opacity (heatwave.wesl).
//!
//! Known shortcuts (docs/tech-debt.md): Steam jets
//! (specs/source/visual_entities.md 7, `render/steam.rs`): heat-wave jets shift
//! the scene by a fixed 2% of the screen at most with a radial falloff, where
//! the game's `sprites/heatwave` material refracts through its own normal map
//! (not read); the JetLength/SpreadSpeed/Speed inputs
//! (`SteamInputs`) change new puffs only and not the light ramp, which stays
//! sampled along the map's jet; roll and RollSpeed are read as
//! degrees (Q7); "drawn last frame" is approximated by the jet's place being
//! potentially visible (vis), not its on-screen test; the light samples use the
//! light field's probe (the mean of its ambient cube plus its direct lights, ×
//! the look's light scale) once at load (jets don't move), which comes out near
//! full white for de_nuke's jets even in dim corners (brightness unchecked
//! against CS:S); puffs are drawn with the material's texture alpha-blended
//! (StandardMaterial, unlit), not the material's own shader. Why it's OK for
//! now: de_nuke's jets puff where the steam ambient plays; a screenshot
//! comparison settles the look (Q4).

use bevy::{
    asset::embedded_asset,
    mesh::Indices,
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

use crate::map::{EntityPart, MapPart, MapSpawn, SkyboxCamera, SteamInputs, ViewModelCamera, monitor, steam::MapSteam, water};

/// The scene shift of a heat-wave puff at full opacity, as a fraction of
/// the screen.
const HEAT_SHIFT: f32 = 0.02;

/// A heat-wave jet's material: refracts the opaque scene behind its quads
/// (heatwave.wesl); the transmissive phase.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct HeatWaveMaterial {
    /// x: the largest shift (fraction of the screen).
    #[uniform(0)]
    pub params: Vec4,
}

impl Default for HeatWaveMaterial {
    fn default() -> Self {
        Self {
            params: Vec4::new(HEAT_SHIFT, 0.0, 0.0, 0.0),
        }
    }
}

impl Material for HeatWaveMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://chimera_present/render/heatwave.wesl".into()
    }

    /// Opaque, in the transmissive phase: it shows the scene behind it.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn reads_view_transmission_texture(&self) -> bool {
        true
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Registers the heat-wave material. Needs rendering.
pub struct HeatWaveMaterialPlugin;

impl Plugin for HeatWaveMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "heatwave.wesl");
        super::water::views_keep_transmission(app);
        app.add_plugins(MaterialPlugin::<HeatWaveMaterial>::default());
    }
}

struct Puff {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    roll: f32,
    roll_speed: f32,
}

/// A jet's puffs and its mesh.
#[derive(Component)]
pub struct SteamEmitter {
    pub steam: MapSteam,
    pub mesh: Handle<Mesh>,
    puffs: Vec<Puff>,
    /// Seconds until the next puff.
    next: f32,
    /// Emitting since its last turn on (the first puff comes at once).
    started: bool,
    rng: u64,
    drawn_empty: bool,
    /// The jet as the map placed it, kept when an input first changes it.
    base: Option<MapSteam>,
}

impl SteamEmitter {
    pub fn new(steam: MapSteam, mesh: Handle<Mesh>, seed: u64) -> Self {
        Self {
            steam,
            mesh,
            puffs: Vec::new(),
            next: 0.0,
            started: false,
            rng: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            drawn_empty: false,
            base: None,
        }
    }

    /// The value inputs over the map's own values (`SteamInputs`); the
    /// puffs already out keep the velocity they were born with.
    fn apply_inputs(&mut self, inputs: Option<&SteamInputs>) {
        if inputs.is_none() && self.base.is_none() {
            return;
        }
        let i = inputs.copied().unwrap_or_default();
        let base = self.base.get_or_insert_with(|| self.steam.clone()).clone();
        self.steam.length = i.length.unwrap_or(base.length);
        self.steam.spread = i.spread.unwrap_or(base.spread);
        self.steam.speed = i.speed.unwrap_or(base.speed);
    }

    /// Uniform in 0..1 (xorshift).
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn between(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.random()
    }

    /// Puffs alive now.
    pub fn count(&self) -> usize {
        self.puffs.len()
    }

    /// One frame of `dt` seconds: emit while `on` (one puff at once, then
    /// one every 1/rate s), move, roll and age the puffs.
    pub fn step(&mut self, dt: f32, on: bool) {
        let Some(life) = self.steam.life() else {
            self.puffs.clear();
            return;
        };
        if on {
            if !self.started {
                self.started = true;
                self.next = 0.0;
            }
            self.next -= dt;
            let period = 1.0 / self.steam.rate;
            while self.next <= 0.0 {
                // Born `-next` seconds ago, within this frame.
                let s = self.steam.clone();
                let (a, b) = (self.between(-s.spread, s.spread), self.between(-s.spread, s.spread));
                let velocity = s.forward * s.speed + s.right * a + s.up * b;
                let roll = self.between(0.0, std::f32::consts::TAU);
                let roll_speed = self.between(-s.roll_speed, s.roll_speed);
                let late = -self.next;
                self.puffs.push(Puff {
                    position: s.origin + velocity * late,
                    velocity,
                    age: late,
                    roll: roll + roll_speed * late,
                    roll_speed,
                });
                self.next += period;
            }
        } else {
            self.started = false;
        }
        for p in &mut self.puffs {
            p.age += dt;
            p.position += p.velocity * dt;
            p.roll += p.roll_speed * dt;
        }
        self.puffs.retain(|p| p.age <= life);
    }
}

/// Headless stand-ins (`MapDrawer::spawn_undrawn`): the jets still puff,
/// undrawn, switched by the logic.
pub fn spawn_undrawn(commands: &mut Commands, s: &MapSpawn) {
    for (i, jet) in s.data.source_fx().steam.iter().enumerate() {
        let mut e = commands.spawn((
            Name::new(format!("Steam {i}")),
            MapPart,
            SteamEmitter::new(jet.clone(), Handle::default(), i as u64 + 1),
        ));
        if let Some(entity) = jet.entity {
            e.insert(EntityPart {
                entity,
                on: jet.start_on,
                exists: true,
            });
        }
    }
}

/// Step every jet and rebuild its mesh: rolled camera-facing quads, far to
/// near. A jet out of view (hidden by `vis`) neither emits nor moves.
#[allow(clippy::type_complexity)]
pub fn update_steam(
    time: Res<Time>,
    cameras: Query<
        &GlobalTransform,
        (
            With<Camera3d>,
            Without<monitor::ScreenCamera>,
            Without<SkyboxCamera>,
            Without<ViewModelCamera>,
            Without<water::WaterReflectionCamera>,
        ),
    >,
    mut emitters: Query<(
        &mut SteamEmitter,
        Option<&EntityPart>,
        Option<&Visibility>,
        Option<&SteamInputs>,
    )>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
) {
    let dt = time.delta_secs().min(0.1);
    let eye = cameras.iter().next();
    for (mut e, part, visibility, inputs) in &mut emitters {
        e.apply_inputs(inputs);
        if part.is_some_and(|p| !p.exists) {
            e.puffs.clear();
        }
        let hidden = visibility == Some(&Visibility::Hidden);
        let on = part.map_or(e.steam.start_on, |p| p.on && p.exists);
        if !hidden {
            e.step(dt, on);
        }
        let (Some(eye), Some(meshes)) = (eye, meshes.as_mut()) else {
            continue;
        };
        if hidden {
            continue;
        }
        let Some(life) = e.steam.life() else { continue };
        let (from, forward) = (eye.translation(), eye.forward().as_vec3());
        let (right, up) = (eye.right().as_vec3(), eye.up().as_vec3());
        let mut order: Vec<(f32, usize)> = e
            .puffs
            .iter()
            .enumerate()
            .map(|(i, p)| ((p.position - from).dot(forward), i))
            .filter(|(d, _)| *d > 0.0)
            .collect();
        order.sort_by(|a, b| b.0.total_cmp(&a.0));
        let heat = e.steam.heat_wave;
        // A heat wave's random shift, new each frame, one per puff.
        let shifts: Vec<[f32; 2]> = if heat {
            order.iter().map(|_| [e.random(), e.random()]).collect()
        } else {
            Vec::new()
        };
        let (mut positions, mut uvs, mut colors, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (n, (_, i)) in order.into_iter().enumerate() {
            let p = &e.puffs[i];
            let half = e.steam.size(p.age);
            let alpha = e.steam.opacity(p.age, life);
            if half <= 0.0 || alpha <= 0.0 {
                continue;
            }
            let c = if heat {
                let [x, y] = shifts[n];
                Vec3::new(x, y, 0.0)
            } else {
                e.steam.color(p.age, life)
            };
            // A heat wave stays square to the view, unrolled.
            let (s, cs) = if heat { (0.0, 1.0) } else { p.roll.sin_cos() };
            let (r, u) = (right * cs + up * s, up * cs - right * s);
            let base = positions.len() as u32;
            for (cx, cy, tu, tv) in [
                (-1.0, -1.0, 0.0, 1.0),
                (-1.0, 1.0, 0.0, 0.0),
                (1.0, 1.0, 1.0, 0.0),
                (1.0, -1.0, 1.0, 1.0),
            ] {
                positions.push((p.position + (r * cx + u * cy) * half).to_array());
                uvs.push([tu, tv]);
                colors.push([c.x, c.y, c.z, alpha]);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        if positions.is_empty() && e.drawn_empty {
            continue;
        }
        e.drawn_empty = positions.is_empty();
        if positions.is_empty() {
            // Keep one invisible triangle (see `dust::empty_mesh`).
            positions.extend([[0.0; 3]; 3]);
            uvs.extend([[0.0; 2]; 3]);
            colors.extend([[0.0; 4]; 3]);
            indices.extend([0, 1, 2]);
        }
        if let Some(mut mesh) = meshes.get_mut(&e.mesh) {
            let n = positions.len();
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![(-forward).to_array(); n]);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            mesh.insert_indices(Indices::U32(indices));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::map::steam::STEAM_RAMPS;

    const UNIT: f32 = 0.0254;

    fn jet(speed: f32, length: f32, rate: f32) -> MapSteam {
        MapSteam {
            origin: Vec3::ZERO,
            forward: Vec3::Y,
            right: Vec3::X,
            up: Vec3::Z,
            spread: 15.0 * UNIT,
            speed: speed * UNIT,
            start_size: 10.0 * UNIT,
            end_size: 25.0 * UNIT,
            rate,
            length: length * UNIT,
            roll_speed: 8f32.to_radians(),
            alpha: 1.0,
            ramp: [Vec3::ONE; STEAM_RAMPS],
            texture: None,
            entity: None,
            start_on: true,
            heat_wave: false,
        }
    }

    #[test]
    fn value_inputs_replace_the_maps_values_and_clearing_restores_them() {
        let mut e = SteamEmitter::new(jet(120.0, 80.0, 26.0), Handle::default(), 1);
        let map = e.steam.clone();
        e.apply_inputs(Some(&SteamInputs {
            length: Some(240.0 * UNIT),
            speed: Some(60.0 * UNIT),
            ..default()
        }));
        assert!((e.steam.life().unwrap() - 4.0).abs() < 1e-5);
        assert_eq!(e.steam.spread, map.spread);
        e.apply_inputs(None);
        assert_eq!(e.steam.length, map.length);
        assert_eq!(e.steam.speed, map.speed);
    }

    /// A heat-wave jet built into a mesh by `update_steam`, seen from the
    /// origin down -Z.
    fn heat_wave_mesh(heat_wave: bool) -> Mesh {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<Assets<Mesh>>()
            .add_systems(Update, update_steam);
        app.world_mut().spawn((Camera3d::default(), GlobalTransform::default()));
        let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(Mesh::from(Rectangle::default()));
        let steam = MapSteam {
            origin: Vec3::new(0.0, 0.0, -2.0),
            forward: -Vec3::Z,
            right: Vec3::X,
            up: Vec3::Y,
            heat_wave,
            ..jet(120.0, 80.0, 26.0)
        };
        app.world_mut().spawn(SteamEmitter::new(steam, mesh.clone(), 3));
        for _ in 0..30 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
            app.update();
        }
        app.world().resource::<Assets<Mesh>>().get(&mesh).unwrap().clone()
    }

    #[test]
    fn heat_wave_puffs_are_unrolled_view_plane_quads_with_a_random_shift() {
        let mesh = heat_wave_mesh(true);
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap().to_vec();
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else {
            panic!("no colours")
        };
        assert!(positions.len() >= 8, "{} vertices", positions.len());
        for (quad, c) in positions.chunks(4).zip(colors.chunks(4)) {
            // Facing the eye (looking down -Z): one depth, axis-aligned edges.
            assert!(quad.iter().all(|p| p[2] == quad[0][2]));
            assert_eq!(quad[0][0], quad[1][0]);
            assert_eq!(quad[0][1], quad[3][1]);
            // The shift is two numbers in 0..1; alpha is the opacity.
            assert!(c.iter().all(|v| (0.0..=1.0).contains(&v[0]) && (0.0..=1.0).contains(&v[1]) && v[2] == 0.0));
            let alpha = c[0][3];
            assert!(alpha > 0.0 && c.iter().all(|v| v[3] == alpha));
        }
        // Not the same shift for every puff.
        assert!(colors.iter().any(|c| c[0] != colors[0][0]));
        // Smoke is rolled (its corners are not axis-aligned) and tinted.
        let smoke = heat_wave_mesh(false);
        let p = smoke.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap();
        assert!(p.chunks(4).any(|q| q[0][0] != q[1][0]));
    }

    #[test]
    fn rate_26_makes_26_or_27_a_second() {
        // Long-lived puffs, so none die within the second.
        let mut e = SteamEmitter::new(jet(10.0, 100.0, 26.0), Handle::default(), 1);
        for _ in 0..60 {
            e.step(1.0 / 60.0, true);
        }
        assert!((26..=27).contains(&e.count()), "{}", e.count());
        // Off: no more puffs; the others live out their life.
        let n = e.count();
        e.step(1.0 / 60.0, false);
        assert_eq!(e.count(), n);
    }

    #[test]
    fn puffs_leave_along_the_jet_and_die_at_its_end() {
        let mut e = SteamEmitter::new(jet(120.0, 80.0, 26.0), Handle::default(), 7);
        for _ in 0..120 {
            e.step(1.0 / 60.0, true);
        }
        let s = e.steam.clone();
        for p in &e.puffs {
            // Along +Y at 120 u/s, sideways within the spread.
            assert!((p.velocity.y - s.speed).abs() < 1e-5);
            assert!(p.velocity.x.abs() <= s.spread + 1e-6 && p.velocity.z.abs() <= s.spread + 1e-6);
            assert!(p.age <= s.life().unwrap());
        }
        // 26 a second, each living 2/3 s: about 17-18 alive.
        assert!((16..=19).contains(&e.count()), "{}", e.count());
    }
}
