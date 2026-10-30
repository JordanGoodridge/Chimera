---
name: add-weapon-from-data
description: Add or change a weapon as a TOML data document (no Rust), register it, and test it.
---

# Add a weapon from data

Reference: docs/weapon-data.md (every table and key). Weapons are TOML
documents parsed by `chimera_sim::weapon::data`; behaviour data can't
express is a hook a game registers under a name the data asks for.

1. **Values come from a spec** (`specs/<game>/...`) or a measurement,
   never from memory of the original game. An original weapon of ours
   is fine: say so in the PR.
2. Write the document: `games/<game>/data/weapons/<name>.toml` for a
   game's weapon (CS:S: also add it to `weapons::DATA`; a unit test fails
   for a file missing there), or an app's own data loaded with
   `App::add_weapon_data`. Start from `base = "<game>:gun"` and set
   only what differs: `id` (namespaced, `game:weapon_name`), `slot`,
   `class`, `[trigger]`, `[magazine]` with an `ammo` type, `[hitscan]`,
   `[damage]`, `[buy]`.
3. Test it:
   - parse and values: CS:S's table check is
     `games/cs_source/tests/it/cs_guns.rs`; add the new weapon's row;
   - behaviour: a scenario test that gives it (`give <name>` or the test
     kit's helpers) and fires: timing, magazine, damage at range
     (`chimera/sim/tests/it/weapon_frame.rs` shows how).
   `cargo test -p <game crate> --features dev --test it cs_guns::`
4. In a game: `give <name>` in the console, then screenshot it
   (playtest skill). Models and sounds load from the install; their
   paths are checked by heavy tests that need it.
