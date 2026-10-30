//! Render styles (specs/combat_arms/models_ltb.md "Textures and render
//! styles", characters.md 2, view_models.md 2): a model piece names a
//! render-style index, which picks a render-style file (`RS\*.LTB`) from
//! the list its object was given (a body record's `RenderStyle#`, a
//! weapon's `PVRenderStyle#` / `HHRenderStyle#`, a head piece's or
//! face's); a missing entry is the engine's default style. How each file
//! draws is our data (data/render_styles.toml, by file name: opaque,
//! blended by the texture's alpha, added, glowing, or alpha-tested cut
//! out as models_ltb.md "Render styles and sprite skins" reads the
//! files), applied to the piece's mesh (`apply`).
//!
//! Known shortcuts: the files aren't read (their format is in
//! models_ltb.md now); the table is our summary of them, so a style
//! not in it draws opaque, as the default style, and the environment and
//! rim maps of slot 1 (`envmap`, `itemedge`, `pv_char`...) aren't drawn.

use std::{collections::HashMap, sync::OnceLock};

use serde::Deserialize;

use crate::map::{MapAlpha, MapMesh};

/// How a render style draws a piece.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// The engine's default: opaque (alpha-tested where the texture says).
    #[default]
    Opaque,
    /// Blended by the texture's alpha (glass, the transparent mark).
    Blend,
    /// Added to what's behind (light effects).
    Add,
    /// At its texture's own brightness, ignoring the lighting.
    Glow,
    /// Alpha-tested, not blended (models_ltb.md "Render styles and sprite
    /// skins"): drawn opaque where the texture's alpha passes the test
    /// against `reference`/255 (≥, or > when `strict`), discarded
    /// elsewhere; `unlit`: the texture's colour alone; `two_sided`: no
    /// culling.
    Cutout {
        reference: u8,
        #[serde(default)]
        strict: bool,
        #[serde(default)]
        unlit: bool,
        #[serde(default)]
        two_sided: bool,
    },
}

impl Style {
    /// Whether a texel of alpha `alpha` (0..255) is drawn.
    pub fn keeps(self, alpha: u8) -> bool {
        match self {
            Style::Cutout { reference, strict: false, .. } => alpha >= reference,
            Style::Cutout { reference, strict: true, .. } => alpha > reference,
            _ => alpha > 0,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    styles: HashMap<String, Style>,
}

fn key(name: &str) -> String {
    name.trim().replace('\\', "/").to_lowercase()
}

fn table() -> &'static HashMap<String, Style> {
    static T: OnceLock<HashMap<String, Style>> = OnceLock::new();
    T.get_or_init(|| {
        let t: Table = toml::from_str(include_str!("../../data/render_styles.toml")).expect("data/render_styles.toml");
        t.styles.into_iter().map(|(k, v)| (key(&k), v)).collect()
    })
}

/// How render-style file `name` draws (an unknown or empty name: the
/// default, opaque).
pub fn style(name: &str) -> Style {
    table().get(&key(name)).copied().unwrap_or_default()
}

/// The style of index `index` in an object's render-style list (past
/// the list or negative: the default).
pub fn of_list(list: &[String], index: i32) -> Style {
    usize::try_from(index)
        .ok()
        .and_then(|i| list.get(i))
        .map_or(Style::Opaque, |n| style(n))
}

/// Draw `mesh` in `style`.
pub fn apply(mesh: &mut MapMesh, style: Style) {
    match style {
        Style::Opaque => {}
        Style::Blend => mesh.alpha = MapAlpha::Blend,
        Style::Add => {
            mesh.alpha = MapAlpha::Add;
            mesh.unlit = true;
        }
        Style::Glow => mesh.unlit = true,
        Style::Cutout { reference, strict, unlit, two_sided } => {
            // The cut between the passing and failing texel values.
            let cut = if strict { f32::from(reference) + 0.5 } else { f32::from(reference) - 0.5 };
            mesh.alpha = MapAlpha::Mask(cut / 255.0);
            mesh.unlit |= unlit;
            mesh.double_sided |= two_sided;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_body_styles() {
        assert_eq!(style("RS\\default.ltb"), Style::Opaque);
        assert_eq!(style("RS\\MARKTRANSPARENT.LTB"), Style::Blend);
        assert_eq!(style("rs/marktransparent.ltb"), Style::Blend);
        assert_eq!(style("RS\\nothing_like_it.ltb"), Style::Opaque);
        let list = ["RS\\default.ltb".to_string(), "RS\\GLOW.LTB".into(), "RS\\MARKTRANSPARENT.LTB".into()];
        assert_eq!(of_list(&list, 2), Style::Blend);
        assert_eq!(of_list(&list, 3), Style::Opaque);
        assert_eq!(of_list(&list, -1), Style::Opaque);
        let mut m = MapMesh::default();
        apply(&mut m, Style::Blend);
        assert_eq!(m.alpha, MapAlpha::Blend);
    }
}
