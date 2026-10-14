//! Drawn parts the logic switches (`map::EntityPart`: sprites, glows,
//! beams, dust volumes, steam jets, emitters): shown or hidden
//! (`LogicHidden`) as their entity is turned on and off or killed
//! (`switch_parts`).

use bevy::prelude::*;

use super::{dust, steam};
use crate::map::{EntityPart, LogicHidden, VisClusters};

/// Sprites and dust volumes follow their entity (`EntityPart`): a sprite
/// is drawn while on; a dust volume while it exists (off, it only stops
/// spawning motes).
#[allow(clippy::type_complexity)]
pub(crate) fn switch_parts(
    parts: Query<
        (
            Entity,
            &EntityPart,
            Option<&VisClusters>,
            Has<LogicHidden>,
            Has<dust::DustEmitter>,
            Has<steam::SteamEmitter>,
        ),
        Changed<EntityPart>,
    >,
    mut commands: Commands,
) {
    for (e, part, clusters, hidden, dust, steam) in &parts {
        // Dust and steam stay drawn while off: what they made lives on.
        let dust = dust || steam;
        let shown = part.exists && (dust || part.on);
        if shown != hidden {
            continue;
        }
        if shown {
            let mut c = commands.entity(e);
            c.remove::<LogicHidden>();
            if clusters.is_none_or(|c| c.potentially_visible) {
                c.insert(Visibility::Inherited);
            }
        } else {
            commands.entity(e).insert((LogicHidden, Visibility::Hidden));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switched_parts_hide_and_show() {
        let mut app = App::new();
        app.add_systems(Update, switch_parts);
        let sprite = app
            .world_mut()
            .spawn((
                EntityPart {
                    entity: 3,
                    on: true,
                    exists: true,
                },
                Visibility::Inherited,
            ))
            .id();
        app.update();
        assert!(!app.world().entity(sprite).contains::<LogicHidden>());
        app.world_mut().get_mut::<EntityPart>(sprite).unwrap().on = false;
        app.update();
        assert!(app.world().entity(sprite).contains::<LogicHidden>());
        assert_eq!(app.world().get::<Visibility>(sprite), Some(&Visibility::Hidden));
        app.world_mut().get_mut::<EntityPart>(sprite).unwrap().on = true;
        app.update();
        assert!(!app.world().entity(sprite).contains::<LogicHidden>());
        assert_eq!(app.world().get::<Visibility>(sprite), Some(&Visibility::Inherited));
    }
}
