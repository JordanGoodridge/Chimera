//! Lucker Party's dedicated server's binary (`lucker_party_server`): the
//! library's `run` (its `ServerPlugin` on a headless run loop, the console
//! on stdin: `status`, `minigames`, `minigame_status`, `minigame_restart`,
//! `changelevel`, `quit`, any cvar; its output on stdout). Options:
//! `--help` (lib.rs's `USAGE`).
//!
//! Released built alone: `cargo build --release -p lucker_party_server`
//! (no presentation links).

use lucker_party_server::{Parsed, USAGE};

fn main() {
    let options = match lucker_party_server::parse(std::env::args().skip(1)) {
        Ok(Parsed::Run(o)) => o,
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return;
        }
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    lucker_party_server::run(&options, |_| {});
}
