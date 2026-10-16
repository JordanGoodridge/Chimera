//! How bullets pass things, as data (docs/weapon-data.md, "Penetration
//! by material"): a game's document gives `PassMaterials`, the scale and
//! damage factor per surface material class (the letter a map's surface
//! names, `MapSurface::game_material`, which comes from the game's
//! install), for classes not listed, and for characters. Each ammo type's
//! power and reach are its weapon data's (`[ammo."<id>"]`).
//!
//! ```toml
//! units = { length = 0.0254 }     # metres per length unit (default 1)
//! [[material]]
//! class = "W"                     # one letter
//! scale = 2.0                     # material passed per unit of power
//! damage = 0.6                    # damage carried on, as a factor
//! [default]                       # other classes (default 1.0, 0.5)
//! scale = 1.0
//! damage = 0.5
//! [character]                     # (default: never passed)
//! cost = 21.5                     # power one character uses, length units
//! damage = 0.5
//! range_scale = 0.5               # range left beyond its exit, as a factor
//! ```
//!
//! Unknown keys, a class that isn't one letter and a class given twice
//! are load errors.

use serde::Deserialize;

use super::{CharacterPass, PassMaterial, PassMaterials};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PassDoc {
    #[serde(default)]
    units: Option<LengthUnit>,
    #[serde(default)]
    material: Vec<MaterialData>,
    #[serde(default)]
    default: Option<FactorData>,
    #[serde(default)]
    character: Option<CharacterData>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LengthUnit {
    length: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialData {
    class: String,
    scale: f32,
    damage: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactorData {
    scale: f32,
    damage: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterData {
    cost: f32,
    damage: f32,
    range_scale: f32,
}

impl PassMaterials {
    /// Parse a penetration document (`name` for errors).
    pub fn parse(name: &str, text: &str) -> Result<Self, String> {
        let doc: PassDoc = toml::from_str(text).map_err(|e| format!("{name}: {e}"))?;
        let len = doc.units.map_or(1.0, |u| u.length);
        let mut out = Self::default();
        for m in &doc.material {
            let mut chars = m.class.chars();
            let (Some(class), None) = (chars.next(), chars.next()) else {
                return Err(format!("{name}: material class {:?} isn't one letter", m.class));
            };
            if out.by_class.iter().any(|(c, _)| *c == class) {
                return Err(format!("{name}: material class {class} given twice"));
            }
            out.by_class.push((
                class,
                PassMaterial {
                    scale: m.scale,
                    damage: m.damage,
                },
            ));
        }
        if let Some(d) = doc.default {
            out.default = PassMaterial {
                scale: d.scale,
                damage: d.damage,
            };
        }
        if let Some(c) = doc.character {
            out.character = CharacterPass {
                cost: c.cost * len,
                damage: c.damage,
                range_scale: c.range_scale,
            };
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_gives_the_table_in_its_units() {
        let m = PassMaterials::parse(
            "p.toml",
            "units = { length = 0.5 }\n[[material]]\nclass = \"W\"\nscale = 2.0\ndamage = 0.6\n\
             [character]\ncost = 10.0\ndamage = 0.5\nrange_scale = 0.5",
        )
        .unwrap();
        assert_eq!(
            m.get(Some('W')),
            PassMaterial {
                scale: 2.0,
                damage: 0.6
            }
        );
        assert_eq!(m.get(Some('X')), PassMaterials::default().default);
        assert_eq!(m.character.cost, 5.0);
        for (text, why) in [
            ("[[material]]\nclass = \"WD\"\nscale = 1.0\ndamage = 1.0", "one letter"),
            (
                "[[material]]\nclass = \"W\"\nscale = 1.0\ndamage = 1.0\n[[material]]\nclass = \"W\"\nscale = 1.0\ndamage = 1.0",
                "twice",
            ),
            ("[default]\nscale = 1.0\ndamage = 1.0\nhardness = 2", "unknown field"),
        ] {
            let e = PassMaterials::parse("p.toml", text).unwrap_err();
            assert!(e.starts_with("p.toml: ") && e.contains(why), "{e}");
        }
    }
}
