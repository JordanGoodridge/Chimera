//! Glow sprites' visibility (`map::GlowSprite`: spotlight halos, light
//! glows, glow-mode sprites): each frame, faded by how much of its
//! occlusion proxy the eye sees (`glow_visibility`: line tests standing in
//! for the game's occlusion queries).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::map::{
    GlowSprite, LogicHidden, SkyboxCamera, ViewModelCamera, VisClusters, monitor, sprite_material::SpriteMaterial,
    water,
};

/// Glow visibility (specs/cs_source/sprites_dust.md, 5a): the game measures
/// the visible fraction of a view-facing square (half-diagonal = proxy
/// size) pulled the proxy size toward the eye, with occlusion queries. We
/// approximate it with lines to the square's centre and corners, and fade
/// the glow by the fraction that get through. Static props don't occlude.
#[allow(clippy::type_complexity)]
pub(crate) fn glow_visibility(
    query: SpatialQuery,
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
    ignored: Query<Entity, With<crate::core::Intent>>,
    mut glows: Query<
        (
            &GlobalTransform,
            &GlowSprite,
            &MeshMaterial3d<SpriteMaterial>,
            &mut Visibility,
            Option<&VisClusters>,
        ),
        Without<LogicHidden>,
    >,
    materials: Option<ResMut<Assets<SpriteMaterial>>>,
) {
    let (Some((eye, camera)), Some(mut materials)) = (cameras.iter().next(), materials) else {
        return;
    };
    let filter = SpatialQueryFilter::from_excluded_entities(ignored.iter()).with_mask(crate::core::NOT_SHADOW);
    let from = eye.translation();
    let (right, up) = (eye.right().as_vec3(), eye.up().as_vec3());
    for (glow, sprite, material, mut visibility, clusters) in &mut glows {
        // Not potentially visible: hidden, no occlusion tests.
        if clusters.is_some_and(|c| !c.potentially_visible) {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let p = glow.translation();
        let centre = p + (from - p).normalize_or_zero() * sprite.proxy;
        let r = sprite.proxy / std::f32::consts::SQRT_2;
        let points = [
            centre,
            centre + (right + up) * r,
            centre + (right - up) * r,
            centre - (right + up) * r,
            centre - (right - up) * r,
        ];
        let seen = points
            .iter()
            .filter(|to| {
                Dir3::new(**to - from).map_or(true, |dir| {
                    query.cast_ray(from, dir, from.distance(**to), true, &filter).is_none()
                })
            })
            .count() as f32
            / points.len() as f32;
        // Times the fraction of the proxy's screen rectangle inside the
        // viewport (0 if any corner is behind the eye), as the game's
        // occlusion queries are clipped to the screen.
        let seen = seen * clip_fraction(camera, eye, &points[1..]);
        visibility.set_if_neq(if seen > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        // Only touch the material when the colour changes: a mutable
        // borrow alone re-prepares it for the GPU.
        let color = (sprite.color.truncate() * seen).extend(sprite.color.w);
        if materials.get(&material.0).is_some_and(|m| m.params.color != color)
            && let Some(mut m) = materials.get_mut(&material.0)
        {
            m.params.color = color;
        }
    }
}

/// How much of the screen rectangle around `corners` lies in the viewport.
fn clip_fraction(camera: &Camera, eye: &GlobalTransform, corners: &[Vec3]) -> f32 {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for c in corners {
        let Some(ndc) = camera.world_to_ndc(eye, *c) else {
            return 0.0;
        };
        if ndc.z < 0.0 || ndc.z > 1.0 {
            return 0.0;
        }
        lo = lo.min(ndc.truncate());
        hi = hi.max(ndc.truncate());
    }
    let area = (hi - lo).max(Vec2::splat(1e-6));
    let inside = (hi.min(Vec2::ONE) - lo.max(Vec2::NEG_ONE)).max(Vec2::ZERO);
    (inside.x * inside.y / (area.x * area.y)).clamp(0.0, 1.0)
}
