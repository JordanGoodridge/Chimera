//! A turned or sheared quad of an image, clipped to a rectangle, as a UI
//! material (`MaterialNode<ClipQuadMaterial>`). Bevy's UI clip
//! (`Overflow::clip`) does not work on turned nodes (a turned node is
//! drawn whole, spilling out of its window), so panels that turn images
//! inside a window (Combat Arms' radar) draw them with this: the node is
//! the shape's bounding box cut to the clip, unturned, and the shader
//! cuts the shape out of it.
//!
//! `quad_node` makes the node and its material from the quad's corners
//! (top-left, top-right, bottom-right, bottom-left of the texture; any
//! parallelogram) and the clip rectangle, both in the parent's px.

use bevy::{asset::embedded_asset, prelude::*, reflect::TypePath, render::render_resource::{AsBindGroup, ShaderType}, shader::ShaderRef};

pub struct ClipQuadPlugin;

impl Plugin for ClipQuadPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "clip_quad.wesl");
        app.add_plugins(UiMaterialPlugin::<ClipQuadMaterial>::default());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct QuadParams {
    pub color: LinearRgba,
    /// First corner (xy) and edge u (zw), px from the node's top-left.
    pub origin_u: Vec4,
    /// Edge v (xy), padding (zw).
    pub v_pad: Vec4,
    /// The part of the image drawn: uv offset (xy) and size (zw).
    pub uv: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ClipQuadMaterial {
    #[uniform(0)]
    pub params: QuadParams,
    #[texture(1)]
    #[sampler(2)]
    pub image: Handle<Image>,
}

impl UiMaterial for ClipQuadMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://chimera_client/client/clip_quad.wesl".into()
    }
}

/// A quad's node (the bounding box within the clip) and its material.
pub struct QuadNode {
    pub node: Node,
    pub material: ClipQuadMaterial,
}

/// The node and material drawing `image` (white when `None`) in `color`
/// over the parallelogram `corners` (parent px), cut to `clip` (min, max;
/// parent px), the image's `uv` part (offset, size) mapped over it. `None`
/// when nothing of it can show.
pub fn quad_node(corners: [Vec2; 4], clip: (Vec2, Vec2), color: Color, image: Option<Handle<Image>>, uv: Vec4) -> Option<QuadNode> {
    let [p0, p1, _, p3] = corners;
    let (u, v) = (p1 - p0, p3 - p0);
    if (u.x * v.y - u.y * v.x).abs() < 1e-6 {
        return None;
    }
    let lo = corners.iter().fold(Vec2::splat(f32::MAX), |a, c| a.min(*c)).max(clip.0);
    let hi = corners.iter().fold(Vec2::splat(f32::MIN), |a, c| a.max(*c)).min(clip.1);
    if hi.x <= lo.x || hi.y <= lo.y {
        return None;
    }
    let size = hi - lo;
    Some(QuadNode {
        node: Node {
            position_type: PositionType::Absolute,
            left: px(lo.x),
            top: px(lo.y),
            width: px(size.x),
            height: px(size.y),
            ..default()
        },
        material: ClipQuadMaterial {
            params: QuadParams {
                color: color.to_linear(),
                origin_u: Vec4::new(p0.x - lo.x, p0.y - lo.y, u.x, u.y),
                v_pad: Vec4::new(v.x, v.y, 0.0, 0.0),
                uv,
            },
            image: image.unwrap_or_default(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quad turned about the middle of the clip, much bigger than it:
    /// its node never leaves the clip.
    #[test]
    fn a_turned_quad_stays_inside_its_clip() {
        let clip = (Vec2::ZERO, Vec2::new(349.0, 261.0));
        let c = Vec2::new(170.0, 130.0);
        for deg in (0..360).step_by(15) {
            let r = Rot2::degrees(deg as f32);
            let h = Vec2::splat(260.0);
            let corners = [Vec2::new(-h.x, -h.y), Vec2::new(h.x, -h.y), h, Vec2::new(-h.x, h.y)].map(|p| c + r * p);
            let q = quad_node(corners, clip, Color::WHITE, None, Vec4::new(0.0, 0.0, 1.0, 1.0)).expect("shows");
            let (Val::Px(l), Val::Px(t), Val::Px(w), Val::Px(hh)) = (q.node.left, q.node.top, q.node.width, q.node.height) else {
                panic!("px")
            };
            assert!(l >= 0.0 && t >= 0.0 && l + w <= 349.0 + 1e-3 && t + hh <= 261.0 + 1e-3, "{deg}: {l} {t} {w} {hh}");
        }
        // Wholly outside: nothing.
        let off = [Vec2::new(500.0, 0.0), Vec2::new(600.0, 0.0), Vec2::new(600.0, 50.0), Vec2::new(500.0, 50.0)];
        assert!(quad_node(off, clip, Color::WHITE, None, Vec4::ONE).is_none());
    }
}
