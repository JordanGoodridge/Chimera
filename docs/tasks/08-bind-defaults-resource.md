# Default binds per game as a resource

## Context

`chimera_client::client::binds` keeps the default game's binds in two
statics, `GAME_DEFAULTS` and `OLD_DEFAULTS`, set once from the install.
An app with several games can't hold each game's defaults, and switching
the gameplay can't switch them. Step 10 of
[docs/plans/active/ecs-cleanup.md](../plans/active/ecs-cleanup.md).

## Files

- `chimera/client/src/client/binds.rs` (`GAME_DEFAULTS`, `OLD_DEFAULTS`,
  `read_install_defaults`, `use_game_defaults`)
- `chimera_core::gameplay::Gameplay` (its `binds` slot)
- Guards: binds' unit tests, `apps/party_kit/tests/it/controls.rs`

## Definition of done

- [ ] A resource holding each registered game's defaults (and old
      defaults); the shell reads the entry of the gameplay's `binds` slot.
- [ ] The statics are gone; the determinism ratchet reflects it.
- [ ] A test: two games' defaults registered, the gameplay's are used.
- [ ] Re-reading on a gameplay switch: implement only if trivial, else
      leave it as a "Known shortcuts" line in the file's `//!` doc.

## Constraints

Parity: CS:S's own defaults and `config.cfg` behaviour don't change.

## Out of bounds

The Controls page's UI.

## Size and labels

- Size: S
- Area: area:ui, area:engine
- Needs install: no
- good-first-task
