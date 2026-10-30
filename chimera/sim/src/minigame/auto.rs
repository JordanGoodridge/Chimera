//! An automatic gun game from whatever games an app composed
//! (docs/plans/active/ultimate-gun-game.md): each game's claim of its parts
//! (`chimera_core::game_parts`) checked against the app (`report`), a
//! ladder spread across the games' guns (`ladder`), a map pool of every
//! game's maps (`map_pool`) and the definition text that plays them
//! (`definition`) on `chimera:ladder` with `identity = "weapon"`.
//!
//! - `report`: per registered game, what it really lends here: its guns
//!   that are in the weapon catalogue (claimed, and every other weapon of
//!   its own whose data names a class: its install's weapons too), its
//!   grenades that kill (class `grenade`), its knife if there, its
//!   movement if registered, its claimed characters and HUD, and its
//!   maps (claimed, and the installed ones its `map_patterns` match:
//!   `InstalledMaps`, found once by `find_installed_maps` in an app that
//!   wants them). A game is in a ladder with guns and movement
//!   (`Capabilities::playable`); the rest falls back (`fallbacks`): no
//!   HUD, `PartsRegistry::fallback_hud`; no characters, the shell's
//!   capsule (`map::CharacterModels::for_identity` finds none of its
//!   game); no maps, the other games'.
//! - `ladder`: `rungs` weapons, seeded and deterministic (SplitMix64; no
//!   hash maps): guns drawn from every game's guns (each game at least
//!   once; a game's guns weighted one over the square root of its count;
//!   at most a third of one class), climbing by class (pistols first,
//!   snipers last), then one or two grenade rungs (one per
//!   `GRENADE_EVERY` rungs at most) in the top third, never last. Two
//!   seeds share few rungs, and every gun comes up over many.
//! - `map_pool`: every map of every game whose install the players have,
//!   the games in turn; none: the greybox.
//! - `definition`: the minigame's TOML (`MinigameDef`), filtered by
//!   `installed` (the games every player has: L3's mount reports, which
//!   the app reads; `chimera_net::minigame_net::installed_by_all`). Its
//!   kit is by game (`def::Kit::ByGame`): each gun's game's own knife,
//!   the base game's for a game that lends none; its rotation random but
//!   the last maps played when `AutoOptions::avoid_recent` says
//!   (`minigame::rotation`).

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
    /// Its guns: the claimed ones and every other weapon of its own in
    /// the catalogue whose data names a gun class.
    pub guns: Vec<GunPart>,
    /// Its grenades that kill (class `Grenade`, claimed or in its data).
    pub grenades: Vec<&'static str>,
    pub knife: Option<&'static str>,
    pub movement: Option<&'static str>,
    pub characters: Option<&'static str>,
    pub hud: Option<&'static str>,
    /// Its claimed maps, then its installed maps its `map_patterns` match.
    pub maps: Vec<String>,
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
        Self::with_maps(parts, catalog, movements, &[])
    }

    /// `of`, with the maps its install holds (`InstalledMaps::of`).
    pub fn with_maps(
        parts: &GameParts,
        catalog: &WeaponCatalog,
        movements: Option<&MovementRegistry>,
        installed: &[String],
    ) -> Self {
        let mut missing = Vec::new();
        let mut all: Vec<GunPart> = parts
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
        // Its other weapons with a class in their data (its install's).
        let prefix = format!("{}:", parts.game);
        for set in &catalog.sets {
            for w in &set.weapons {
                if !w.id.starts_with(&prefix) || all.iter().any(|g| g.id == w.id) {
                    continue;
                }
                let Some(data) = catalog.get(&w.id) else { continue };
                if let Some(class) = data.class {
                    all.push(GunPart { id: &data.id, class });
                }
            }
        }
        let grenades = all.iter().filter(|g| g.class == GunClass::Grenade).map(|g| g.id).collect();
        let guns = all.into_iter().filter(|g| g.class != GunClass::Grenade).collect();
        let knife = parts.knife.filter(|k| catalog.get(k).is_some());
        let movement = parts.movement.filter(|m| {
            let ok = movements.is_some_and(|r| r.get(m).is_some());
            if !ok {
                missing.push(format!("movement {m} isn't registered"));
            }
            ok
        });
        let mut maps: Vec<String> = parts.maps.iter().map(|m| m.to_string()).collect();
        for name in installed.iter().filter(|n| parts.wants_map(n)) {
            let id = format!("{}:{}", parts.game, name.to_ascii_lowercase());
            if !maps.contains(&id) {
                maps.push(id);
            }
        }
        Self {
            game: parts.game,
            guns,
            grenades,
            knife,
            movement,
            characters: parts.characters,
            hud: parts.hud,
            maps,
            missing,
        }
    }

    /// One line for the console (`parts`).
    pub fn line(&self) -> String {
        let or = |o: Option<&str>, fallback: &str| o.map_or(format!("none ({fallback})"), str::to_string);
        format!(
            "{}: {} guns, {} grenades, knife {}, movement {}, characters {}, HUD {}, maps {}{}{}",
            self.game,
            self.guns.len(),
            self.grenades.len(),
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
    let installed = world.get_resource::<InstalledMaps>();
    registry
        .all()
        .map(|p| Capabilities::with_maps(p, catalog, movements, installed.map_or(&[], |i| i.of(p.game))))
        .collect()
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

    /// In `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// In `0..n` (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn class_index(c: GunClass) -> usize {
    GunClass::ALL.iter().position(|x| *x == c).unwrap_or(0)
}

/// A ladder gets a grenade rung for each this many rungs at most (and one
/// at least when it is that long and a game lends a grenade).
pub const GRENADE_EVERY: usize = 6;

fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        items.swap(i, rng.below(i + 1));
    }
}

/// `rungs` weapons of `games` (each playable), seeded: guns drawn from
/// every game's guns (each game at least one; a game's guns weighted by
/// one over the square root of its count, so a game with many guns gives
/// more rungs but doesn't drown the others; at most a third of the guns
/// of one class), climbing by class (pistols first, snipers last; a
/// class's guns in the drawn order), then a grenade rung or two (one
/// for each `GRENADE_EVERY` rungs at most, of the games' grenades) put
/// in the top third, never on the last rung. Shorter when the games
/// have fewer guns.
pub fn ladder(games: &[&Capabilities], rungs: usize, seed: u64) -> Vec<&'static str> {
    let games: Vec<&Capabilities> = games.iter().copied().filter(|c| c.playable()).collect();
    if games.is_empty() || rungs == 0 {
        return Vec::new();
    }
    let mut rng = Rng::new(seed);
    let mut grenades: Vec<&'static str> = Vec::new();
    for g in games.iter().flat_map(|c| &c.grenades) {
        if !grenades.contains(g) {
            grenades.push(g);
        }
    }
    let most = (rungs / GRENADE_EVERY).min(grenades.len());
    let thrown = if most == 0 { 0 } else { 1 + rng.below(most) };
    let want = rungs - thrown;
    // (gun, its game, its weight)
    let mut pool: Vec<(GunPart, usize, f64)> = Vec::new();
    for (i, c) in games.iter().enumerate() {
        let weight = 1.0 / (c.guns.len() as f64).sqrt();
        for gun in &c.guns {
            if !pool.iter().any(|(p, ..)| p.id == gun.id) {
                pool.push((*gun, i, weight));
            }
        }
    }
    let mut picked: Vec<GunPart> = Vec::new();
    // Each game once, in a seeded order.
    let mut order: Vec<usize> = (0..games.len()).collect();
    shuffle(&mut order, &mut rng);
    for g in order {
        let of: Vec<usize> = (0..pool.len()).filter(|&k| pool[k].1 == g).collect();
        if picked.len() >= want || of.is_empty() {
            continue;
        }
        picked.push(pool.remove(of[rng.below(of.len())]).0);
    }
    // The rest by weight, a class at most a third of the guns.
    let cap = want.div_ceil(3).max(2);
    while picked.len() < want && !pool.is_empty() {
        let full = |c: GunClass| picked.iter().filter(|p| p.class == c).count() >= cap;
        let mut open: Vec<usize> = (0..pool.len()).filter(|&k| !full(pool[k].0.class)).collect();
        if open.is_empty() {
            open = (0..pool.len()).collect();
        }
        let total: f64 = open.iter().map(|&k| pool[k].2).sum();
        let mut at = rng.unit() * total;
        let mut chosen = open[open.len() - 1];
        for &k in &open {
            if at < pool[k].2 {
                chosen = k;
                break;
            }
            at -= pool[k].2;
        }
        picked.push(pool.remove(chosen).0);
    }
    picked.sort_by_key(|g| class_index(g.class));
    let mut out: Vec<&'static str> = picked.into_iter().map(|g| g.id).collect();
    shuffle(&mut grenades, &mut rng);
    for g in grenades.into_iter().take(thrown) {
        let len = out.len();
        if len == 0 {
            break;
        }
        // In the top third, before the last rung.
        let low = (rungs * 2 / 3).min(len - 1);
        out.insert(low + rng.below(len - low), g);
    }
    out
}

/// Every map of the games in `caps` that `installed` says the players
/// have, the games in turn; none: the greybox.
pub fn map_pool(caps: &[Capabilities], installed: &dyn Fn(&str) -> bool) -> Vec<String> {
    let lists: Vec<&Vec<String>> = caps.iter().filter(|c| installed(c.game)).map(|c| &c.maps).collect();
    let longest = lists.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..longest {
        for l in &lists {
            if let Some(m) = l.get(i)
                && !out.contains(m)
            {
                out.push(m.clone());
            }
        }
    }
    if out.is_empty() {
        out.push("greybox".into());
    }
    out
}

/// The maps each registered game's install holds (`GameInfo::map_names`,
/// by game id), found once (`find_installed_maps`): `report` adds the
/// ones a game's `map_patterns` match to its maps. Absent (a test, an
/// app that never looked): claimed maps only.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct InstalledMaps(pub Vec<(String, Vec<String>)>);

impl InstalledMaps {
    pub fn of(&self, game: &str) -> &[String] {
        self.0.iter().find(|(g, _)| g.eq_ignore_ascii_case(game)).map_or(&[], |(_, m)| m)
    }
}

/// `InstalledMaps` from each registered game's install (mounted once;
/// kept when already there).
pub fn find_installed_maps(world: &mut World) {
    if world.contains_resource::<InstalledMaps>() {
        return;
    }
    let games: Vec<&'static chimera_core::game_registry::GameInfo> = world
        .get_resource::<chimera_core::game_registry::Games>()
        .map(|g| g.all().collect())
        .unwrap_or_default();
    let wanted = world.get_resource::<PartsRegistry>().cloned().unwrap_or_default();
    let found = games
        .into_iter()
        .filter(|g| wanted.get(g.id).is_some_and(|p| !p.map_patterns.is_empty()))
        .filter_map(|g| Some((g.id.to_string(), (g.map_names)(&g.mount()?))))
        .collect();
    world.insert_resource(InstalledMaps(found));
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
    /// After a match: a random map of the pool, not one of the last this
    /// many played (`def::Rotation::Random`); None: the pool's next.
    pub avoid_recent: Option<usize>,
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
    let rotation = opts
        .avoid_recent
        .map(|n| format!("rotation = \"random\"\navoid_recent = {n}\n"))
        .unwrap_or_default();
    Ok(format!(
        "# An automatic gun game (chimera_sim::minigame::auto), seed {seed}.\n\
         id = {id}\nname = {name}\n\
         description = \"Climb a ladder of every game's guns; each gun makes you its game.\"\n\
         players = {{ min = 2, max = 16 }}\nmaps = {maps}\nteams = \"free_for_all\"\n\
         time_limit = {time}\nrotate = true\n{rotation}\n\
         [slots]\n{hud}binds = {base}\ntick = {base}\nbots = {base}\n{movement}\
         identity = \"weapon\"\nweapons = {sets}\n\n\
         [spawn]\nrespawn = {{ delay = \"2s\", protection = \"2s\" }}\npoints = \"random_hidden\"\n\
         min_distance = 20.32\nkit = {kit}\n\n\
         [rules]\nmodule = \"chimera:ladder\"\nladder = {ladder}\nadvance_on = \"kill\"\n\
         demote_on = \"melee_death\"\nknife_finale = true\nrefill_ammo = true\nwin = {{ kind = \"ladder_end\" }}\n",
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
            grenades: vec![],
            knife: None,
            movement: Some("m"),
            characters: None,
            hud: None,
            maps: vec![],
            missing: vec![],
        }
    }

    fn with_grenade(mut c: Capabilities) -> Capabilities {
        c.grenades = vec![Box::leak(format!("{}:frag", c.game).into_boxed_str())];
        c
    }

    fn class_of(caps: &[&Capabilities], id: &str) -> Option<GunClass> {
        caps.iter().flat_map(|c| &c.guns).find(|g| g.id == id).map(|g| g.class)
    }

    #[test]
    fn a_ladder_takes_every_game_and_climbs() {
        let (a, b, c) = (guns("a", 24), guns("b", 15), guns("c", 3));
        let caps = [&a, &b, &c];
        for seed in 0..50 {
            let l = ladder(&caps, 9, seed);
            assert_eq!(l.len(), 9);
            for g in ["a", "b", "c"] {
                assert!(l.iter().any(|id| id.starts_with(&format!("{g}:"))), "{g} in {l:?}");
            }
            let mut seen = l.clone();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), l.len(), "a gun once");
            let classes: Vec<usize> = l.iter().map(|id| class_index(class_of(&caps, id).unwrap())).collect();
            assert!(classes.windows(2).all(|w| w[0] <= w[1]), "climbing: {classes:?}");
            let most = GunClass::ALL.iter().map(|k| l.iter().filter(|id| class_of(&caps, id) == Some(*k)).count()).max();
            assert!(most <= Some(3), "a class at most a third: {l:?}");
        }
        // More rungs than guns: every gun once.
        assert_eq!(ladder(&[&c], 10, 1).len(), 3);
    }

    #[test]
    fn a_ladder_is_the_seeds_and_seeds_differ_widely() {
        let (a, b) = (guns("a", 24), guns("b", 15));
        let caps = [&a, &b];
        assert_eq!(ladder(&caps, 12, 42), ladder(&caps, 12, 42));
        let ladders: Vec<Vec<&str>> = (0..400).map(|s| ladder(&caps, 12, s)).collect();
        // Two ladders share few rungs.
        let mut shared = 0;
        let mut worst = 0;
        for w in ladders.windows(2) {
            let n = w[0].iter().filter(|id| w[1].contains(id)).count();
            shared += n;
            worst = worst.max(n);
        }
        let mean = shared as f64 / (ladders.len() - 1) as f64;
        assert!(mean < 12.0 * 0.4, "on average {mean:.1} of 12 rungs shared");
        assert!(worst < 10, "never nearly the same: {worst}");
        // Every gun comes up.
        for g in a.guns.iter().chain(&b.guns) {
            assert!(ladders.iter().any(|l| l.contains(&g.id)), "{} never drawn", g.id);
        }
    }

    #[test]
    fn grenade_rungs_are_near_the_top_never_last() {
        let (a, b) = (with_grenade(guns("a", 24)), with_grenade(guns("b", 15)));
        let caps = [&a, &b];
        let mut counts = [0; 3];
        for seed in 0..300 {
            let l = ladder(&caps, 12, seed);
            assert_eq!(l.len(), 12);
            let at: Vec<usize> = (0..l.len()).filter(|&i| l[i].ends_with(":frag")).collect();
            assert!((1..=2).contains(&at.len()), "one or two grenade rungs: {l:?}");
            counts[at.len()] += 1;
            assert!(at.iter().all(|&i| (8..11).contains(&i)), "in the top third, not last: {l:?}");
            let guns: Vec<usize> =
                l.iter().filter_map(|id| class_of(&caps, id)).map(class_index).collect();
            assert!(guns.windows(2).all(|w| w[0] <= w[1]), "the guns still climb: {l:?}");
        }
        assert!(counts[1] > 0 && counts[2] > 0, "{counts:?}");
        // Short ladders: none.
        assert!(ladder(&caps, 5, 1).iter().all(|id| !id.ends_with(":frag")));
    }

    #[test]
    fn games_without_movement_or_install_stay_out() {
        let a = guns("a", 6);
        let mut b = guns("b", 6);
        b.movement = None;
        assert!(ladder(&[&a, &b], 6, 3).iter().all(|id| id.starts_with("a:")));
        let mut c = guns("c", 6);
        c.maps = vec!["c:map1".into(), "c:map2".into()];
        let mut d = guns("d", 6);
        d.maps = vec!["d:map1".into()];
        let opts = AutoOptions {
            id: "test:auto".into(),
            name: "Auto".into(),
            rungs: 6,
            seed: 9,
            base: "a",
            time_limit: 600.0,
            avoid_recent: Some(10),
        };
        let caps = [a, c, d];
        let text = definition(&caps, &opts, &|g| g != "c").unwrap();
        let def = crate::minigame::MinigameDef::parse("auto", &text).expect(&text);
        assert_eq!(def.maps, ["d:map1"], "c isn't installed: its maps go too");
        assert_eq!(def.rotation, crate::minigame::def::Rotation::Random);
        assert_eq!(def.avoid_recent, 10);
        let ladder = def.rules.params["ladder"].as_array().unwrap();
        assert!(ladder.iter().all(|id| !id.as_str().unwrap().starts_with("c:")), "{ladder:?}");
        assert_eq!(map_pool(&caps, &|_| true), ["c:map1", "d:map1", "c:map2"], "in turn");
        assert_eq!(map_pool(&caps[..1], &|_| true), ["greybox"]);
        assert!(definition(&caps, &opts, &|_| false).is_err());
    }

    #[test]
    fn installed_maps_join_by_pattern() {
        static P: GameParts = GameParts {
            game: "g",
            guns: &[],
            knife: None,
            movement: None,
            characters: None,
            hud: None,
            maps: &["g:de_one"],
            map_patterns: &["de_*"],
            announcer: None,
            match_won_sound: None,
            match_lost_sound: None,
            music: false,
            cues: None,
        };
        let installed: Vec<String> = ["de_one", "DE_Two", "cs_three", "test_x"].iter().map(|s| s.to_string()).collect();
        let c = Capabilities::with_maps(&P, &WeaponCatalog::default(), None, &installed);
        assert_eq!(c.maps, ["g:de_one", "g:de_two"]);
    }
}
