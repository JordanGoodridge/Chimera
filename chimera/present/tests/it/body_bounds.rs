//! A character body's parts are culled by where the pose puts them
//! (`render::bodies`): each skinned part's mesh carries per-joint bounds
//! and its entity `DynamicSkinnedMeshBounds`, so a body whose bind pose
//! is far from its drawn pose (a Combat Arms female body is bound standing
//! on its origin, half a body above where its animation draws it) is not
//! culled for the bind pose's box: its parts used to vanish from close
//! up.

use bevy::{
    camera::visibility::DynamicSkinnedMeshBounds,
    ecs::system::SystemState,
    mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes, entity_aabb_from_skinned_mesh_bounds},
    prelude::*,
};
use chimera_present::{
    MapPlugin,
    render::{
        bodies::BodyJoint, prop_material::PropMaterial, rope_material::RopeMaterial, shadows::ShadowMaterial,
        sprite_material::SpriteMaterial, world_material::WorldMaterial,
    },
};
use chimera_world::{
    harness::Sim,
    map::{BodyName, MapBone, MapCharacterModel, MapData, MapMesh, MapModel},
};

/// A 0.4 m quad 2 m up in the bind pose, on bone 1 (bone 0 the root).
fn body() -> MapData {
    let mut data = MapData { name: "test:body_bounds".into(), entity_scale: 0.0254, ..default() };
    let mesh = MapMesh {
        material: "test/body".into(),
        positions: vec![[-0.2, 2.0, 0.0], [0.2, 2.0, 0.0], [0.2, 2.4, 0.0], [-0.2, 2.4, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        uvs: vec![[0.0, 0.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        joints: vec![[1, 0, 0, 0]; 4],
        joint_weights: vec![[1.0, 0.0, 0.0, 0.0]; 4],
        color: [128; 3],
        ..default()
    };
    let bone = |name: &str, parent: Option<usize>, y: f32| MapBone {
        name: name.into(),
        parent,
        position: Vec3::Y * y,
        rotation: Quat::IDENTITY,
    };
    data.kit.characters.push(MapCharacterModel {
        game: None,
        team: None,
        name: Some("tester".into()),
        class: None,
        model: MapModel { meshes: vec![mesh], ..default() },
        hitboxes: Vec::new(),
        bones: vec![bone("root", None, 0.0), bone("head", Some(0), 2.0)],
        root: Transform::IDENTITY,
        animations: None,
        boxes: Vec::new(),
        ragdoll: None,
    });
    data
}

#[test]
fn body_parts_are_culled_by_their_posed_bounds() {
    let mut sim = Sim::with((chimera_core::core::CorePlugin, chimera_core::console::ConsolePlugin), |app| {
        app.init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_asset::<WorldMaterial>()
            .init_asset::<PropMaterial>()
            .init_asset::<SpriteMaterial>()
            .init_asset::<RopeMaterial>()
            .init_asset::<ShadowMaterial>()
            .init_asset::<SkinnedMeshInverseBindposes>()
            .add_plugins(MapPlugin::new(body()));
    });
    sim.app.world_mut().spawn((
        chimera_core::core::Intent::default(),
        BodyName("tester".into()),
        avian3d::prelude::ColliderAabb::new(Vec3::new(0.0, 0.9, 0.0), Vec3::new(0.3, 0.9, 0.3)),
        Transform::default(),
    ));
    sim.ticks(3);
    // Pose the head bone 1.5 m lower than its bind place.
    let world = sim.app.world_mut();
    let head = world
        .query::<(Entity, &BodyJoint)>()
        .iter(world)
        .find(|(_, j)| j.0 == 1)
        .map(|(e, _)| e)
        .expect("the head joint");
    world.entity_mut(head).insert(Transform::from_xyz(0.0, 0.5, 0.0));
    sim.ticks(2);
    let world = sim.app.world_mut();
    let parts: Vec<(Entity, Handle<Mesh>, SkinnedMesh, bool)> = world
        .query::<(Entity, &Mesh3d, &SkinnedMesh, Has<DynamicSkinnedMeshBounds>)>()
        .iter(world)
        .map(|(e, m, s, d)| (e, m.0.clone(), s.clone(), d))
        .collect();
    assert_eq!(parts.len(), 1, "the body's one part");
    let (part, mesh, skinned, dynamic) = parts.into_iter().next().unwrap();
    assert!(dynamic, "the part's bounds follow its joints");
    let mut state: SystemState<(
        Query<&GlobalTransform>,
        Res<Assets<Mesh>>,
        Res<Assets<SkinnedMeshInverseBindposes>>,
    )> = SystemState::new(world);
    let (transforms, meshes, binds) = state.get(world).unwrap();
    let mesh = meshes.get(&mesh).expect("the part's mesh");
    assert!(mesh.skinned_mesh_bounds().is_some(), "per-joint bounds on the mesh");
    let at = transforms.get(part).ok();
    let aabb = entity_aabb_from_skinned_mesh_bounds(&transforms, mesh, &skinned, binds.get(&skinned.inverse_bindposes).unwrap(), at)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let to_world = at.copied().unwrap_or_default();
    let (lo, hi) = (to_world.transform_point(aabb.min.into()), to_world.transform_point(aabb.max.into()));
    // Drawn 1.5 m lower: 0.5 to 0.9 m above the body's feet, which sit
    // at the collider's bottom (0).
    assert!(lo.y < 0.55 && hi.y > 0.85 && hi.y < 1.0, "posed bounds {lo}..{hi}");
}
