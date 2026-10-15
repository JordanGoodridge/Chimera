//! Network play (engine/net, crate `engine_net`) at its old path, with
//! the replicated types the game and Source's logic register as their
//! parts of it (`net::NetAppExt::net_part`): CS:S's round, bomb, hostages
//! and requests (`cs_source::game::net`) and Source's movers and HUD
//! messages (`logic::net`).

pub use engine_net::net::*;

pub use cs_source::game::net::{
    ClassRequest, NetBomb, NetHostage, NetRound, ObjectiveNews, RoundOver, TeamRequest, bomb_outcome, round_phase,
};
pub use source_logic::logic::net::{MapHud, NetMover, mover_flags};

/// `version`'s text for this build (`core::VERSION`).
pub fn version_text() -> String {
    engine_net::net::version_text(crate::core::VERSION)
}
