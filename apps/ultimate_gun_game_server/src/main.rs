//! Ultimate Gun Game's dedicated server (apps/ultimate_gun_game_server,
//! binary `ultimate_gun_game_server`): the party's server
//! (`lucker_party_server::run`) with the Ultimate Gun Game's parts
//! (`ultimate_gun_game::UltimateGunGamePlugin`), playing the automatic
//! gun game (`+minigame` names another) on its pool's first map unless
//! `+map` names one. `ugg_new [seed] [rungs]` on the console makes a new
//! ladder for the players connected; `parts` lists what each game lends.
//! The options are `lucker_party_server`'s (`--help`).

use lucker_party_server::{Parsed, USAGE};

fn main() {
    let first = |id: &str| ultimate_gun_game::first_map(id);
    let options = match lucker_party_server::parse_for(std::env::args().skip(1), ultimate_gun_game::MINIGAME, &first) {
        Ok(Parsed::Run(o)) => o,
        Ok(Parsed::Help) => {
            println!("{}\n(this server's default minigame: {})", USAGE, ultimate_gun_game::MINIGAME);
            return;
        }
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    lucker_party_server::run(&options, |app| {
        app.add_plugins(ultimate_gun_game::UltimateGunGamePlugin::default());
    });
}
