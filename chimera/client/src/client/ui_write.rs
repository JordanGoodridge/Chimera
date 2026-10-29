//! Writing UI components only when the value changes. `bevy_ui` lays a
//! node out again, and re-measures its text, whenever its `Node`,
//! `Text` or content size is marked changed, and borrowing one mutably
//! marks it changed even when nothing differs: a HUD text set to the same
//! string every frame (or to "" while hidden) costs a re-layout of its
//! tree each frame (docs/performance.md, "Party frame-rate pass", third
//! pass). `set_text` and `edit` compare first and write only a real
//! change.

use bevy::{ecs::change_detection::DetectChangesMut, prelude::*};

/// Set a text's string if it differs (no change mark, no allocation
/// otherwise).
pub fn set_text(text: &mut Mut<Text>, s: &str) {
    if text.0 != s {
        let t = &mut text.0;
        t.clear();
        t.push_str(s);
    }
}

/// Change a copy of a component with `f` and write it back only if it
/// differs (a `Node`'s place and size, a colour).
pub fn edit<T: Clone + PartialEq>(c: &mut Mut<T>, f: impl FnOnce(&mut T)) {
    let mut v = (**c).clone();
    f(&mut v);
    c.set_if_neq(v);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed_after(world: &mut World, e: Entity, f: impl FnOnce(&mut Mut<Text>, &mut Mut<Node>)) -> (bool, bool) {
        world.clear_trackers();
        let tick = world.change_tick();
        world.increment_change_tick();
        {
            let mut q = world.query::<(&mut Text, &mut Node)>();
            let (mut t, mut n) = q.get_mut(world, e).unwrap();
            f(&mut t, &mut n);
        }
        let this_run = world.change_tick();
        let t = world.entity(e).get_ref::<Text>().unwrap().last_changed();
        let n = world.entity(e).get_ref::<Node>().unwrap().last_changed();
        (t.is_newer_than(tick, this_run), n.is_newer_than(tick, this_run))
    }

    #[test]
    fn writes_only_changes() {
        let mut world = World::new();
        let e = world.spawn((Text::new("12"), Node::default())).id();
        // The same values: nothing marked.
        assert_eq!(
            changed_after(&mut world, e, |t, n| {
                set_text(t, "12");
                edit(n, |n| n.left = Val::Auto);
            }),
            (false, false)
        );
        // New values: both marked and written.
        assert_eq!(
            changed_after(&mut world, e, |t, n| {
                set_text(t, "13");
                edit(n, |n| n.left = px(4.0));
            }),
            (true, true)
        );
        assert_eq!(world.get::<Text>(e).unwrap().0, "13");
        assert_eq!(world.get::<Node>(e).unwrap().left, px(4.0));
    }
}
