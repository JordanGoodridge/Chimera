//! Kill-feed weapon icons across games: the party apps put one game's guns
//! in another game's HUD, and a HUD draws only its own art. Each game
//! registers, in `WeaponIcons`, how its weapon ids (`game:name`) resolve to
//! an icon it can hand over as plain data (`WeaponIcon`: a glyph of an icon
//! font, or an image), and any HUD asks the registry for an id of another
//! game and draws what comes back with `spawn_weapon_icon`. A HUD that
//! gets none shows the gun's display name instead (`gun_name`). No game
//! crate knows another: the registry is keyed by the id's game part, and
//! the data crosses as handles. A game re-registers when its art loads
//! or changes (later calls replace); with nothing registered every lookup
//! is none, so plain single-game apps are unchanged.
//!
//! Known shortcuts: an icon is drawn at the feed's own size (an image
//! scaled to the line's height, a glyph at its font's size), not at
//! the registering game's layout.

use std::{collections::HashMap, sync::Arc};

use bevy::prelude::*;

/// What a game hands over for one of its guns.
#[derive(Clone, Debug, PartialEq)]
pub enum WeaponIcon {
    /// A glyph of an icon font; `em` is its font size in reference pixels
    /// (the game's 480-line screen).
    Glyph { font: Handle<Font>, ch: char, em: f32 },
    /// An image and its size in texels.
    Image { image: Handle<Image>, size: Vec2 },
}

type Resolver = Arc<dyn Fn(&str) -> Option<WeaponIcon> + Send + Sync>;

/// The games' icon resolvers by game name (the id's part before `:`, lower
/// case).
#[derive(Resource, Default, Clone)]
pub struct WeaponIcons(HashMap<String, Resolver>);

impl WeaponIcons {
    /// Registers (or replaces) `game`'s resolver for its weapon ids.
    pub fn register(&mut self, game: &str, resolver: impl Fn(&str) -> Option<WeaponIcon> + Send + Sync + 'static) {
        self.0.insert(game.to_ascii_lowercase(), Arc::new(resolver));
    }

    /// The icon of weapon `id` from its own game's resolver, if that game
    /// registered one and has art for the gun.
    pub fn icon(&self, id: &str) -> Option<WeaponIcon> {
        let (game, _) = id.split_once(':')?;
        self.0.get(&game.to_ascii_lowercase())?(id)
    }
}

/// Spawns `icon` as a child of `p`: a line `height` tall in pixels
/// (`scale` is pixels per reference pixel, for a glyph's size), absolutely
/// at `at` or, with none, in the parent's flex flow.
pub fn spawn_weapon_icon(p: &mut ChildSpawnerCommands, icon: &WeaponIcon, scale: f32, height: f32, at: Option<Vec2>) {
    let place = |mut n: Node| {
        if let Some(at) = at {
            n.position_type = PositionType::Absolute;
            n.left = px(at.x);
            n.top = px(at.y);
        }
        n
    };
    match icon {
        WeaponIcon::Glyph { font, ch, em } => {
            // Icon fonts draw their glyphs high in the line: lowered to
            // sit with the names.
            p.spawn((
                Text::new(ch.to_string()),
                TextColor(Color::WHITE),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(em * scale),
                    ..default()
                },
                place(Node {
                    margin: UiRect::top(px(em * scale * 0.5)),
                    ..default()
                }),
            ));
        }
        WeaponIcon::Image { image, size } => {
            let wide = if size.y > 0.0 { height * size.x / size.y } else { height };
            p.spawn((
                place(Node {
                    width: px(wide),
                    height: px(height),
                    ..default()
                }),
                ImageNode::new(image.clone()),
            ));
        }
    }
}
