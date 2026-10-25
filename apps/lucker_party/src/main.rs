//! The Lucker Party game binary (`lucker_party`): a minigame (the gun game
//! on de_dust2 without `+minigame`) played as Combat Arms' soldiers with
//! both games' guns on its pool's first map (`--map` picks another). The
//! options are the shell's (`--help`); `+minigame <id>` the minigame,
//! `minigames` (console) lists them.

use chimera_client::client::Args;

fn main() {
    if std::env::args().skip(1).any(|a| a == "--help" || a == "-h") {
        println!(
            "lucker_party: minigames with Counter-Strike: Source's and Combat Arms' parts. Starts {} on\n\
             its first map; +minigame <id> picks another ({}), --map cs_source:<name> (or\n\
             combat_arms:<name>) the map. The options are the shell's (below, named after the CS:S app).\n",
            party_kit::DEFAULT_MINIGAME,
            party_kit::ids().join(", "),
        );
    }
    let args = party_kit::with_defaults(Args::parse());
    // What this machine lacks, said exactly (a joiner needs both games
    // too: each player loads the maps, models and sounds from their own).
    let problems = party_kit::requirements();
    if !problems.is_empty() {
        let (text, fatal) = party_kit::report(&problems);
        eprintln!("{text}");
        if fatal {
            std::process::exit(1);
        }
    }
    let app = party_kit::client_app(args, "Lucker Party");
    // The executors, `CHIMERA_SYSTEM_DUMP`, then the game, as every app.
    chimera_client::client::app::run(app);
}
