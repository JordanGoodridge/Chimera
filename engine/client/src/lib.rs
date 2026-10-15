//! The client shell's base (docs/ARCHITECTURE.md, "Crates"): the pieces
//! of `client` a game's front end builds on that need nothing of the
//! network, the rules or a game: the front end's interface
//! (`front_end`: the shell's hooks, `MenuStatus`, `Panels`), the scheme's
//! fonts (`fonts`), HUD text (`hud_text`), the VGUI-like widgets
//! (`widgets`) and the drawing of a game's VGUI layouts (`vgui`), and the
//! key binds (`binds`). It uses only the engine's base (`engine_core`) and
//! map data (`engine_world`: HUD layouts, schemes), so it sits below the
//! games: a game crate's front end can build on it.
//!
//! The game crate re-exports each at its old path (`mashup::client::fonts`,
//! ...); the rest of the shell (window, input, console UI, chat, the HUD,
//! net glue, debug tools) is still `mashup::client`, which names the
//! network and CS:S's rules (docs/plans/active/engine-split.md).

pub mod client;

// `crate::console`, `crate::map`, ... inside `client`, as in the game crate.
use engine_core::{console, game_registry};
use engine_world::map;
