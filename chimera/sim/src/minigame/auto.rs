//! An automatic gun game from whatever games an app composed
//! (docs/plans/active/ultimate-gun-game.md): each game's claim of its parts
//! (`chimera_core::game_parts`) checked against the app (`report`), a
//! ladder spread across the games' guns (`ladder`), a map pool of every
//! game's maps (`map_pool`) and the definition text that plays them
//! (`definition`) on `chimera:ladder` with `identity = "weapon"`.
//!
//! - `report`: per registered game, what it really lends here: its guns
//!   that are in the weapon catalogue, its knife if there, its movement if
//!   registered, and its claimed characters, HUD and maps. A game is in
//!   a ladder with guns and movement (`Capabilities::playable`); the rest
//!   falls back (`fallbacks`): no HUD, `PartsRegistry::fallback_hud`;
//!   no characters, the shell's capsule (`map::CharacterModels::
//!   for_identity` finds none of its game); no maps, the other games'.
//! - `ladder`: `rungs` guns, seeded and deterministic (SplitMix64; no
//!   hash maps), the games taken in turn from a seeded start so each
//!   gives as many rungs as its guns allow (spread), each rung's class the
//!   one nearest its place in the climb (pistols first, snipers last), a
//!   gun at most once, then ordered by class (a stable sort: the turns
//!   stay within a class).
//! - `map_pool`: every map of every game whose install the players have,
//!   the games in turn (a rotation alternates them); none: the greybox.
//! - `definition`: the minigame's TOML (`MinigameDef`), filtered by
//!   `installed` (the games every player has: L3's mount reports, which
//!   the app reads; `chimera_net::minigame_net::installed_by_all`). Its
//!   kit is by game (`def::Kit::ByGame`): each gun's game's own knife,
//!   the base game's for a game that lends none.

use chimera_core::{
    game_parts::{GameParts, GunClass, GunPart, PartsRegistry},
    slots::MovementRegistry,
};

use bevy::prelude::*;

use crate::weapon::data::WeaponCatalog;

/// What a game really lends in this app: its claim (`GameParts`) checked
/// against the weapon catalogue and the movement registry.
#[derive(Clone, Debug, PartialEq)]
pub struct Capabilities {
    pub game: &'static str,
    pub guns: Vec<GunPart>,
    pub knife: Option<&'static str>,
    pub movement: Option<&'static str>,
    pub characters: Option<&'static str>,
    pub hud: Option<&'static str>,
    pub maps: Vec<&'static str>,
    /// Claimed parts the app doesn't have (a gun not in the catalogue, an
    /// unregistered movement), in words.
    pub missing: Vec<String>,
}

impl Capabilities {
    /// Guns and movement: in a ladder.
    pub fn playable(&self) -> bool {
        !self.guns.is_empty() && self.movement.is_some()
    }

    /// `parts` checked against `catalog` and `movements`.
    pub fn of(parts: &GameParts, catalog: &WeaponCatalog, movements: Option<&MovementRegistry>) -> Self {
        let mut missing = Vec::new();
        let guns = parts
            .guns
            .iter()
            .filter(|g| {
                let ok = catalog.get(g.id).is_some();
                if !ok {
                    missing.push(format!("gun {} isn't in the weapon catalogue", g.id));
                }
                ok
            })
            .copied()
            .collect();
        let knife = parts.knife.filter(|k| catalog.get(k).is_some());
        let movement = parts.movement.filter(|m| {
            let ok = movements.is_some_and(|r| r.get(m).is_some());
            if !ok {
                missing.push(format!("movement {m} isn't registered"));
            }
            ok
        });
        Self {
            game: parts.game,
            guns,
            knife,
            movement,
            characters: parts.characters,
            hud: parts.hud,
            maps: parts.maps.to_vec(),
            missing,
        }
    }

    /// One line for the console (`parts`).
    pub fn line(&self) -> String {
        let or = |o: Option<&str>, fallback: &str| o.map_or(format!("none ({fallback})"), str::to_string);
        format!(
            "{}: {} guns, knife {}, movement {}, characters {}, HUD {}, maps {}{}{}",
            self.game,
            self.guns.len(),
            or(self.knife, "the base game's"),
            or(self.movement, "not in a ladder"),
            or(self.characters, "a capsule"),
            or(self.hud, "the fallback HUD"),
            if self.maps.is_empty() { "none (other games')".to_string() } else { self.maps.join(", ") },
            if self.playable() { "" } else { "; not playable" },
            if self.missing.is_empty() { String::new() } else { format!("; missing: {}", self.missing.join("; ")) },
        )
    }
}

/// Every registered game's capabilities here, in registration order.
pub fn report(world: &World) -> Vec<Capabilities> {
    let Some(registry) = world.get_resource::<PartsRegistry>() else {
        return Vec::new();
    };
    let empty = WeaponCatalog::default();
    let catalog = world.get_resource::<WeaponCatalog>().unwrap_or(&empty);
    let movements = world.get_resource::<MovementRegistry>();
    registry.all().map(|p| Capabilities::of(p, catalog, movements)).collect()
}

/// A small deterministic generator (SplitMix64).
#[derive(Clone, Copy, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// In `0..n` (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn class_index(c: GunClass) -> usize {
    GunClass::ALL.iter().position(|x| *x == c).unwrap_or(0)
}

/// `rungs` guns of `games` (each playable), spread across them, climbing
/// by class; shorter when the games have fewer guns.
pub fn ladder(games: &[&Capabilities], rungs: usize, seed: u64) -> Vec<&'static str> {
    let games: Vec<&Capabilities> = games.iter().copied().filter(|c| c.playable()).collect();
    if games.is_empty() || rungs == 0 {
        return Vec::new();
    }
    let mut rng = Rng::new(seed);
    let start = rng.below(games.len());
    let mut used: Vec<&'static str> = Vec::new();
    let mut out: Vec<GunPart> = Vec::new();
    let classes = GunClass::ALL.len();
    let mut turn = 0;
    while out.len() < rungs {
        // The next game in turn with a gun left (none left: done).
        let Some(game) = (0..games.len())
            .map(|k| games[(start + turn + k) % games.len()])
            .find(|g| g.guns.iter().any(|gun| !used.contains(&gun.id)))
        else {
            break;
        };
        let place = if rungs > 1 { out.len() * (classes - 1) / (rungs - 1) } else { 0 };
        let left: Vec<&GunPart> = game.guns.iter().filter(|g| !used.contains(&g.id)).collect();
        let nearest = left.iter().map(|g| class_index(g.class).abs_diff(place)).min().unwrap_or(0);
        let pick: Vec<&GunPart> = left.into_iter().filter(|g| class_index(g.class).abs_diff(place) == nearest).collect();
        let gun = *pick[rng.below(pick.len())];
        used.push(gun.id);
        out.push(gun);
        turn += 1;
    }
    out.sort_by_key(|g| class_index(g.class));
    out.into_iter().map(|g| g.id).collect()
}

/// Every map of the games in `caps` that `installed` says the players
/// have, the games in turn; none: the greybox.
pub fn map_pool(caps: &[Capabilities], installed: &dyn Fn(&str) -> bool) -> Vec<String> {
    let lists: Vec<&Vec<&'static str>> = caps.iter().filter(|c| installed(c.game)).map(|c| &c.maps).collect();
    let longest = lists.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..longest {
        for l in &lists {
            if let Some(m) = l.get(i) {
                out.push(m.to_string());
            }
        }
    }
    if out.is_empty() {
        out.push("greybox".into());
    }
    out
}

/// What a player of game `game` gets where its parts lack one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fallbacks {
    /// The HUD drawn: the game's, else the registry's fallback.
    pub hud: &'static str,
    /// Its own bodies (its character kit), else the shell's capsule.
    pub own_body: bool,
    /// Its own maps in the pool, else other games' only.
    pub own_maps: bool,
}

/// `game`'s fallbacks (None: no parts registered for it).
pub fn fallbacks(registry: &PartsRegistry, game: &str) -> Option<Fallbacks> {
    let p = registry.get(game)?;
    Some(Fallbacks {
        hud: registry.hud_of(game)?,
        own_body: p.characters.is_some(),
        own_maps: !p.maps.is_empty(),
    })
}

/// How an automatic gun game is made.
#[derive(Clone, Debug)]
pub struct AutoOptions {
    pub id: String,
    pub name: String,
    pub rungs: usize,
    pub seed: u64,
    /// The game whose binds, tick and bots the match uses, whose knife a
    /// game without one gets, whose HUD is drawn before a gun's game
    /// takes it.
    pub base: &'static str,
    /// The match's length, seconds.
    pub time_limit: f32,
}

/// TOML-quoted.
fn q(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

fn list(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| q(s)).collect::<Vec<_>>().join(", "))
}

/// The automatic gun game's definition text: the ladder of the playable
/// games every player has, the pool of their maps; an error when no game
/// is left to play.
pub fn definition(caps: &[Capabilities], opts: &AutoOptions, installed: &dyn Fn(&str) -> bool) -> Result<String, String> {
    let games: Vec<&Capabilities> = caps.iter().filter(|c| c.playable() && installed(c.game)).collect();
    let ladder = ladder(&games, opts.rungs, opts.seed);
    if ladder.is_empty() {
        return Err(format!(
            "no game lends guns and movement that every player has ({})",
            caps.iter().map(|c| c.game).collect::<Vec<_>>().join(", ")
        ));
    }
    let base = caps.iter().find(|c| c.game == opts.base);
    let mut sets: Vec<String> = Vec::new();
    for id in &ladder {
        let game = id.split_once(':').map_or(*id, |(g, _)| g).to_string();
        if !sets.contains(&game) {
            sets.push(game);
        }
    }
    // The kit by game: each ladder game's own knife, else the base game's.
    let fallback = base.and_then(|b| b.knife);
    let mut kit: Vec<String> = Vec::new();
    for game in sets.clone() {
        let own = caps.iter().find(|c| c.game == game).and_then(|c| c.knife);
        if let Some(knife) = own.or(fallback) {
            kit.push(format!("{} = {}", q(&game), list(&[knife.to_string()])));
            let of = knife.split_once(':').map_or(knife, |(g, _)| g);
            if !sets.iter().any(|s| s == of) {
                sets.push(of.to_string());
            }
        }
    }
    let kit = format!("{{ {} }}", kit.join(", "));
    let ladder: Vec<String> = ladder.iter().map(|s| s.to_string()).collect();
    let maps = map_pool(caps, installed);
    let movement = base.and_then(|b| b.movement).map(|m| format!("movement = {}\n", q(m))).unwrap_or_default();
    let hud = base.and_then(|b| b.hud).unwrap_or(chimera_core::game_parts::MINIMAL_HUD);
    let hud = if hud == chimera_core::game_parts::MINIMAL_HUD { String::new() } else { format!("hud = {}\n", q(hud)) };
    Ok(format!(
        "# An automatic gun game (chimera_sim::minigame::auto), seed {seed}.\n\
         id = {id}\nname = {name}\n\
         description = \"Climb a ladder of every game's guns; each gun makes you its game.\"\n\
         players = {{ min = 2, max = 16 }}\nmaps = {maps}\nteams = \"free_for_all\"\n\
         time_limit = {time}\nrotate = true\n\n\
         [slots]\n{hud}binds = {base}\ntick = {base}\nbots = {base}\n{movement}\
         identity = \"weapon\"\nweapons = {sets}\n\n\
         [spawn]\nrespawn = {{ delay = \"2s\", protection = \"1.5s\" }}\npoints = \"random_hidden\"\n\
         min_distance = 20.32\nkit = {kit}\n\n\
         [rules]\nmodule = \"chimera:ladder\"\nladder = {ladder}\nadvance_on = \"kill\"\n\
         demote_on = \"melee_death\"\nrefill_ammo = true\nwin = {{ kind = \"ladder_end\" }}\n",
        seed = opts.seed,
        id = q(&opts.id),
        name = q(&opts.name),
        maps = list(&maps),
        time = opts.time_limit,
        base = q(opts.base),
        sets = list(&sets),
        ladder = list(&ladder),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guns(game: &'static str, n: usize) -> Capabilities {
        // Leaked: a test's few ids.
        let guns = (0..n)
            .map(|i| GunPart {
                id: Box::leak(format!("{game}:gun{i}").into_boxed_str()),
                class: GunClass::ALL[i % GunClass::ALL.len()],
            })
            .collect();
        Capabilities {
            game,
            guns,
            knife: None,
            movement: Some("m"),
            characters: None,
            hud: None,
            maps: vec![],
            missing: vec![],
        }
    }

    #[test]
    fn a_ladder_spreads_across_the_games_and_climbs() {
        let (a, b, c) = (guns("a", 12), guns("b", 12), guns("c", 3));
        let l = ladder(&[&a, &b, &c], 9, 1);
        assert_eq!(l.len(), 9);
        let count = |g: &str| l.iter().filter(|id| id.starts_with(&format!("{g}:"))).count();
        assert_eq!((count("a"), count("b"), count("c")), (3, 3, 3), "{l:?}");
        let mut seen = l.clone();
        seen.dedup();
        assert_eq!(seen.len(), l.len(), "a gun once");
        let classes: Vec<usize> = l
            .iter()
            .map(|id| [&a, &b, &c].iter().flat_map(|g| &g.guns).find(|g| g.id == *id).unwrap().class)
            .map(class_index)
            .collect();
        assert!(classes.windows(2).all(|w| w[0] <= w[1]), "climbing: {classes:?}");
        // More rungs than guns: every gun once.
        assert_eq!(ladder(&[&c], 10, 1).len(), 3);
        // Two games, an odd count: they differ by one at most.
        let l = ladder(&[&a, &b], 7, 5);
        let (na, nb) = (
            l.iter().filter(|id| id.starts_with("a:")).count(),
            l.iter().filter(|id| id.starts_with("b:")).count(),
        );
        assert!(na.abs_diff(nb) <= 1, "{l:?}");
    }

    #[test]
    fn a_ladder_is_the_seeds() {
        let (a, b) = (guns("a", 12), guns("b", 12));
        assert_eq!(ladder(&[&a, &b], 8, 42), ladder(&[&a, &b], 8, 42));
        assert!((0..8).any(|s| ladder(&[&a, &b], 8, s) != ladder(&[&a, &b], 8, 42)), "seeds differ");
    }

    #[test]
    fn games_without_movement_or_install_stay_out() {
        let a = guns("a", 6);
        let mut b = guns("b", 6);
        b.movement = None;
        assert!(ladder(&[&a, &b], 6, 3).iter().all(|id| id.starts_with("a:")));
        let mut c = guns("c", 6);
        c.maps = vec!["c:map1", "c:map2"];
        let mut d = guns("d", 6);
        d.maps = vec!["d:map1"];
        let opts = AutoOptions {
            id: "test:auto".into(),
            name: "Auto".into(),
            rungs: 6,
            seed: 9,
            base: "a",
            time_limit: 600.0,
        };
        let caps = [a, c, d];
        let text = definition(&caps, &opts, &|g| g != "c").unwrap();
        let def = crate::minigame::MinigameDef::parse("auto", &text).expect(&text);
        assert_eq!(def.maps, ["d:map1"], "c isn't installed: its maps go too");
        let ladder = def.rules.params["ladder"].as_array().unwrap();
        assert!(ladder.iter().all(|id| !id.as_str().unwrap().starts_with("c:")), "{ladder:?}");
        assert_eq!(map_pool(&caps, &|_| true), ["c:map1", "d:map1", "c:map2"], "in turn");
        assert_eq!(map_pool(&caps[..1], &|_| true), ["greybox"]);
        assert!(definition(&caps, &opts, &|_| false).is_err());
    }
}
