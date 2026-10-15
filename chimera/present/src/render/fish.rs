//! func_fish_pool's fish swimming (`map::fish::Fish`): each fish swims
//! toward its target, turning toward where it goes, never out of the
//! water, and picks another target when it gets there.

use bevy::prelude::*;

use crate::map::fish::{FISH_SPEED, Fish, heading};

/// How fast a fish turns toward its heading, radians per second.
const TURN_RATE: f32 = 2.0;

/// Fish swim to their targets and pick new ones.
pub fn swim(
    time: Res<Time>,
    water: Option<Res<crate::core::MapWater>>,
    mut fish: Query<(&mut Fish, &mut Transform)>,
) {
    let dt = time.delta_secs().min(0.1);
    let wet = |p: Vec3| {
        water
            .as_ref()
            .is_none_or(|w| w.0.iter().any(|v| v.brush.planes.iter().all(|(n, d)| n.dot(p) <= *d)))
    };
    for (mut f, mut t) in &mut fish {
        let target = match f.target {
            Some(target) if target.distance(t.translation) > FISH_SPEED * 0.5 => target,
            _ => {
                let p = f.pick(&wet);
                f.target = Some(p);
                p
            }
        };
        let dir = (target - t.translation).normalize_or_zero();
        if dir == Vec3::ZERO {
            continue;
        }
        let want = heading(dir);
        t.rotation = t.rotation.slerp(want, (TURN_RATE * dt).min(1.0));
        let ahead = t.rotation * Vec3::X;
        let next = t.translation + ahead * FISH_SPEED * dt;
        // Never out of the water.
        if wet(next) {
            t.translation = next;
        } else {
            f.target = None;
        }
    }
}
