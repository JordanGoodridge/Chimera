//! The scope's own picture: while the local player is zoomed through a
//! sight with a `ZoomSpriteName` (`game::weapons::zoom::Scope::sprite`;
//! specs/combat_arms/weapon_classes.md 7.10), that texture is loaded from
//! the install (`effects::FxImages`, the asset server's `.dtx` loader) and
//! handed to the client shell's scope overlay (`client::hud::
//! ScopeOverlayImage`), which draws it centred at its own aspect between
//! black bars. Without a picture (a sight with none, not in the install,
//! still loading) the shell draws its placeholder.
//!
//! Known shortcuts: `ZoomSpriteScale` isn't applied (the picture fills the
//! view's height); a spectated player's scope shows the shell's placeholder.

use bevy::prelude::*;

use crate::{
    client::hud::ScopeOverlayImage,
    core::LocalPlayer,
    game::weapons::CaZoom,
    weapon::{Inventory, Zoomed},
};

/// Puts the picture on the shell's overlay.
pub struct CaScopePlugin;

impl Plugin for CaScopePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScopeSprite>()
            .add_systems(Update, (pick_sprite, load_sprite).chain());
    }
}

/// The install path of the picture the local player's zoomed weapon wants.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct ScopeSprite(pub Option<String>);

/// The active weapon's sight picture while the owner is zoomed (a scope).
fn pick_sprite(
    player: Option<Single<(&Inventory, Option<&Zoomed>), With<LocalPlayer>>>,
    zooms: Query<&CaZoom>,
    mut sprite: ResMut<ScopeSprite>,
) {
    let want = player
        .filter(|p| p.1.is_some_and(|z| z.scope))
        .and_then(|p| p.0.active)
        .and_then(|w| zooms.get(w).ok())
        .and_then(|z| z.scope.as_ref())
        .map(|s| s.sprite.clone())
        .filter(|s| !s.is_empty());
    sprite.set_if_neq(ScopeSprite(want));
}

/// Load the picture asked for and show it once decoded.
fn load_sprite(
    sprite: Res<ScopeSprite>,
    fx: Option<ResMut<super::effects::FxImages>>,
    server: Option<Res<AssetServer>>,
    mut overlay: ResMut<ScopeOverlayImage>,
) {
    let handle = match (&sprite.0, fx, server) {
        (Some(path), Some(mut fx), Some(server)) => fx.get(path, &server).filter(|h| server.load_state(h).is_loaded()),
        _ => None,
    };
    if overlay.0 != handle {
        overlay.0 = handle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::weapons::zoom::Scope;

    fn picked(sprite: &str, zoomed: bool) -> Option<String> {
        let mut app = App::new();
        app.init_resource::<ScopeSprite>().add_systems(Update, pick_sprite);
        let mut z = CaZoom::new(default());
        z.scope = Some(Scope {
            name: "Sniper".into(),
            fov: vec![20.0],
            sprite: sprite.into(),
        });
        let gun = app.world_mut().spawn(z).id();
        let mut inv = Inventory::default();
        inv.weapons.push(gun);
        inv.active = Some(gun);
        let mut me = app.world_mut().spawn((LocalPlayer, inv));
        if zoomed {
            me.insert(Zoomed {
                fov: 20.0,
                scope: true,
                sensitivity: None,
            });
        }
        app.update();
        app.world().resource::<ScopeSprite>().0.clone()
    }

    #[test]
    fn a_zoomed_sniper_selects_its_picture() {
        assert_eq!(picked("hud/scope_x.dtx", true).as_deref(), Some("hud/scope_x.dtx"));
        assert_eq!(picked("hud/scope_x.dtx", false), None, "not zoomed");
        assert_eq!(picked("", true), None, "a sight with no picture");
    }

    /// With no install or asset server nothing is shown: the shell's
    /// placeholder stays.
    #[test]
    fn no_install_leaves_the_placeholder() {
        let mut app = App::new();
        app.init_resource::<ScopeOverlayImage>();
        app.insert_resource(ScopeSprite(Some("hud/scope_x.dtx".into())));
        app.add_systems(Update, load_sprite);
        app.update();
        assert!(app.world().resource::<ScopeOverlayImage>().0.is_none());
    }
}
