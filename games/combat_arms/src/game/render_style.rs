//! Render styles (specs/combat_arms/models_ltb.md "Textures and render
//! styles", characters.md 2, view_models.md 2): a model piece names a
//! render-style index, which picks a render-style file (`RS\*.LTB`) from
//! the list its object was given (a body record's `RenderStyle#`, a
//! weapon's `PVRenderStyle#` / `HHRenderStyle#`, a head piece's or
//! face's); a missing entry is the engine's default style. How each file
//! draws is our data (data/render_styles.toml, by file name: opaque,
//! blended by the texture's alpha, added, or glowing), applied to the
//! piece's mesh (`apply`).
//!
//! Known shortcuts: the render-style files' format isn't in the specs
//! (models_ltb.md "Other files": "not covered"), so what each one does
//! is our reading of its name (the transparent team mark, characters.md
//! 2: "drawn with the transparent mark render style, so enemies show
//! nothing", is the texture's alpha blended); a file not in our table
//! draws opaque, as the default style.

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
