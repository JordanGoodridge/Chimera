//! Structural tests: module layering and repository rules from
//! docs/ARCHITECTURE.md and CLAUDE.md. Failure messages say how to fix the
//! problem, because agents read them.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Top-level module -> top-level modules it may use via `crate::`.
/// Keep in sync with the layer diagram in docs/ARCHITECTURE.md.
const ALLOWED: &[(&str, &[&str])] = &[
    ("core", &[]),
    // Performance counters and distributions: Bevy only.
    ("metrics", &[]),
    ("console", &["core"]),
    ("mount", &[]),
    ("slots", &["core"]),
    ("character", &["core", "slots"]),
    ("movement", &["core", "slots", "character"]),
    ("greybox", &["core"]),
    ("map", &["core"]),
    // Audio: plays what the simulation's sounds say (`map::sound`); nothing
    // in the simulation may use it.
    ("audio", &["core", "map"]),
    // Rendering: draws what the map spawns and simulates (the drawn phases
    // of spawning a map, through `map::MapDrawer`, what the simulation
    // makes in play from its drawn-part requests, effects, culling, water,
    // shadows, sky, view models, the materials and shaders); nothing in the
    // simulation may use it, and the simulation names no drawing type
    // (`simulation_builds_no_drawn_parts`).
    ("render", &["core", "map"]),
    // Builds `map::MapPlugin` from the map and its presentation (`audio`,
    // `render`), its systems in `core`'s sets.
    ("map_plugin", &["core", "map", "audio", "render"]),
    ("weapon", &["core", "console", "map"]),
    // Round objectives (bomb, hostages): zones from the map, NPCs as
    // characters, the bomb as a weapon.
    ("objectives", &["core", "console", "slots", "character", "map", "weapon"]),
    ("rules", &["core", "console", "weapon", "objectives"]),
    // Map logic (entity I/O, triggers, movers): game-independent; games
    // feed it through `map::MapData::entities`.
    ("logic", &["core", "console", "map"]),
    ("bot", &["core", "console", "slots", "character", "map", "weapon", "objectives"]),
    // Network play: transport, replication, the join handshake. Above the
    // simulation it serves, below the client and harness
    // (docs/plans/active/multiplayer.md, section 3).
    (
        "net",
        &[
            "core",
            "console",
            "slots",
            "character",
            "movement",
            "map",
            "weapon",
            "objectives",
            "rules",
            "bot",
            "logic",
        ],
    ),
    // `lib` = crate-root items such as `SimPlugins`.
    (
        "harness",
        &[
            "lib",
            "core",
            "console",
            "slots",
            "character",
            "movement",
            "greybox",
            "map",
            "weapon",
            "objectives",
            "rules",
            "bot",
            "logic",
            "net",
            "metrics",
        ],
    ),
    // The client shell: window, input, camera, console UI, the VGUI-like
    // widget layer, debug tools. `games` here is the game-neutral map
    // loading (`games::load_map`); a game's own module only through
    // `CLIENT_GAME_EXCEPTIONS`. A game's menus and HUD are that game's
    // (`games_ui`), reached through `client::front_end`.
    (
        "client",
        &[
            "lib",
            "core",
            "console",
            "slots",
            "character",
            "movement",
            "greybox",
            "map",
            "audio",
            "render",
            "weapon",
            "objectives",
            "rules",
            "bot",
            "mount",
            "games",
            "logic",
            "net",
            "metrics",
        ],
    ),
    // A game's front end (`src/games/<game>/ui/`: its menus, panels and
    // HUD): built on the client shell; its own game's modules too. Nothing
    // else in the game uses it (`game_parts_do_not_use_their_ui`).
    (
        "games_ui",
        &[
            "lib",
            "core",
            "console",
            "slots",
            "character",
            "movement",
            "greybox",
            "map",
            "audio",
            "render",
            "weapon",
            "objectives",
            "rules",
            "bot",
            "mount",
            "games",
            "logic",
            "net",
            "metrics",
            "client",
        ],
    ),
    // Game plugins: may use the shared layers, never another game. `rules`:
    // a game registers its rule modules (`rules::module`).
    (
        "games",
        &[
            "core",
            "console",
            "slots",
            "character",
            "movement",
            "mount",
            "map",
            "weapon",
            "objectives",
            "rules",
        ],
    ),
    // Binaries use the library through `mashup::`, not `crate::`.
    ("bin", &[]),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The top-level module a file belongs to: `src/movement/noclip.rs` -> `movement`.
/// A game's front end (`src/games/<game>/ui/...`) is `games_ui`.
fn module_of(src: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(src).ok()?;
    let mut parts = rel.components().map(|c| c.as_os_str().to_str().unwrap_or(""));
    let first = parts.next()?;
    if first == "games" && parts.nth(1) == Some("ui") {
        return Some("games_ui".to_string());
    }
    Some(first.trim_end_matches(".rs").to_string())
}

/// The games: `src/games/<name>/` folders.
fn game_names(src: &Path) -> BTreeSet<String> {
    fs::read_dir(src.join("games"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect()
}

/// Client files that still name a game's own module (`crate::games::<game>`),
/// each with why. The client shell should reach a game only through
/// registration (`client::front_end`, the app's plugins); shrink this list.
const CLIENT_GAME_EXCEPTIONS: &[(&str, &str)] = &[
    // TODO(engine-split): a game registry (the default game, its install,
    // maps, map cycle and server tick) in place of naming CS:S.
    ("src/client/console.rs", "map loading: CS:S's install, maps, map cycle, tick"),
    ("src/client/binds.rs", "the default binds from CS:S's install"),
    ("src/client/net.rs", "the map files a server offers: CS:S's install"),
    // TODO(engine-split): movement cvars by the movement slot's registry.
    ("src/client/debug_ui.rs", "the movement tab lists Source movement's cvars"),
];

fn ident(s: &str) -> String {
    s.trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// Top-level paths used after `crate::` in `code` (comments already
/// stripped). `crate::{a::X, b}` yields `a` and `b`; `crate::games::cs::X`
/// yields `games::cs`. Crate-root items (`crate::SimPlugins`) yield `lib`.
fn crate_refs(code: &str, modules: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut add = |path: &str| {
        let first = ident(path);
        if first.is_empty() {
            return;
        }
        if first == "games" {
            let rest = path.trim_start()["games".len()..].trim_start_matches("::");
            out.insert(format!("games::{}", ident(rest)));
        } else if modules.contains(&first) {
            out.insert(first);
        } else {
            out.insert("lib".to_string());
        }
    };
    let mut rest = code;
    while let Some(i) = rest.find("crate::") {
        rest = &rest[i + "crate::".len()..];
        if let Some(group) = rest.strip_prefix('{') {
            // Split the group at top-level commas.
            let (mut depth, mut item_start) = (0usize, 0usize);
            for (j, c) in group.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' if depth == 0 => {
                        add(&group[item_start..j]);
                        break;
                    }
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        add(&group[item_start..j]);
                        item_start = j + 1;
                    }
                    _ => {}
                }
            }
        } else {
            add(rest);
        }
    }
    out
}

#[test]
fn module_layering() {
    let src = root().join("src");
    // The engine's crates keep their modules' names (`engine_core`'s
    // `crate::core`, re-exported by `mashup` at the same paths), so one
    // table covers them all.
    let mut files = Vec::new();
    for dir in ["src", "engine/core/src"] {
        let mut more = Vec::new();
        rust_files(&root().join(dir), &mut more);
        files.extend(more.into_iter().map(|f| (root().join(dir), f)));
    }
    let modules: BTreeSet<String> = ALLOWED.iter().map(|(m, _)| m.to_string()).collect();
    let games = game_names(&src);
    let mut errors = Vec::new();

    for (base, file) in files {
        let Some(module) = module_of(&base, &file) else { continue };
        if module == "lib" || module == "main" {
            continue;
        }
        let Some((_, allowed)) = ALLOWED.iter().find(|(m, _)| *m == module) else {
            errors.push(format!(
                "{}: module `{module}` has no layering rule. Add it to ALLOWED in tests/it/architecture.rs \
                 and to the layer diagram in docs/ARCHITECTURE.md.",
                file.display()
            ));
            continue;
        };
        let text = fs::read_to_string(&file).unwrap();
        // Strip line comments and join, so grouped imports can span lines.
        let code = text
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        let rel = file.strip_prefix(root()).unwrap().to_string_lossy().replace('\\', "/");
        for r in crate_refs(&code, &modules) {
            // `games::load_map` and the like: the game-neutral map loading.
            let r = match r.strip_prefix("games::") {
                Some(name) if !games.contains(name) => "games".to_string(),
                _ => r,
            };
            if let Some(game) = r.strip_prefix("games::") {
                let own = file
                    .strip_prefix(src.join("games"))
                    .ok()
                    .and_then(|p| p.components().next());
                let own = own.map(|c| c.as_os_str().to_string_lossy().trim_end_matches(".rs").to_string());
                if module == "client" {
                    if !CLIENT_GAME_EXCEPTIONS.iter().any(|(f, _)| *f == rel) {
                        errors.push(format!(
                            "{rel}: the client shell uses game `{game}`. A game's menus, panels and HUD live in \
                             `src/games/{game}/ui/` and the shell reaches them through `client::front_end` \
                             (hooks the game registers); the app adds the game's plugins (main.rs). Add a hook \
                             there, or (with a TODO) list the file in CLIENT_GAME_EXCEPTIONS in \
                             tests/it/architecture.rs."
                        ));
                    }
                } else if own.as_deref() != Some(game) && module != "lib" {
                    errors.push(format!(
                        "{}: uses game `{game}`. Games must not depend on each other, and shared layers must \
                         not depend on any game. Move the shared concept into `core` (rule of three) or add a \
                         bridge; see docs/ARCHITECTURE.md.",
                        file.display()
                    ));
                }
                continue;
            }
            if r == module || allowed.contains(&r.as_str()) {
                continue;
            }
            errors.push(format!(
                "{}: `{module}` uses `crate::{r}`, which its layer may not depend on (allowed: {allowed:?}). \
                 Move the shared piece down a layer or invert the dependency (e.g. register through `slots`); \
                 see docs/ARCHITECTURE.md.",
                file.display()
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// Drawing types: what builds or holds meshes and materials. The
/// simulation never names them; it says in plain data what its parts look
/// like and `render` draws them (docs/ARCHITECTURE.md, "Drawn-part
/// requests").
const DRAWING_TYPES: &[&str] = &["Mesh3d", "MeshMaterial3d", "Assets<Mesh>", "StandardMaterial", "MaterialPlugin"];

/// Files of the simulation's layers that still name one of
/// `DRAWING_TYPES` (file, why).
const MAP_DRAWING_EXCEPTIONS: &[(&str, &str)] = &[
    // TODO(engine-split): merged brush entities are drawing only; they
    // move to render with copies' merged pieces shown by a render hook.
    ("src/map/merge.rs", "combined meshes of brush entities"),
    // TODO(engine-split): the placed ragdoll's meshes never cull; a render
    // system can do that.
    ("src/map/placed_ragdoll.rs", "its meshes' `NoFrustumCulling`"),
];

/// The simulation's layers (map, and a game's parts outside its `ui/`)
/// name no drawing type (`DRAWING_TYPES`) and hold no shader, so a
/// dedicated server can leave `render` out. (They can't use `render` at
/// all: `ALLOWED`.)
#[test]
fn simulation_builds_no_drawn_parts() {
    const SIM: &[&str] = &[
        "core",
        "map",
        "logic",
        "net",
        "weapon",
        "rules",
        "objectives",
        "bot",
        "movement",
        "games",
    ];
    let src = root().join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut errors = Vec::new();
    let mut used_exceptions = BTreeSet::new();
    for file in files {
        let Some(module) = module_of(&src, &file) else { continue };
        if !SIM.contains(&module.as_str()) {
            continue;
        }
        let rel = file.strip_prefix(root()).unwrap().to_string_lossy().replace('\\', "/");
        let code = fs::read_to_string(&file)
            .unwrap()
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        let named: Vec<&str> = DRAWING_TYPES.iter().copied().filter(|t| code.contains(t)).collect();
        if named.is_empty() {
            continue;
        }
        if MAP_DRAWING_EXCEPTIONS.iter().any(|(f, _)| *f == rel) {
            used_exceptions.insert(rel.clone());
            continue;
        }
        errors.push(format!(
            "{rel}: the simulation layer `{module}` names {named:?}, which build or hold drawn parts. \
             Say what the part looks like in plain data (a component or message `render` fulfils, \
             docs/ARCHITECTURE.md \"Drawn-part requests\") and draw it in `render`, or (with a TODO) \
             list the file in MAP_DRAWING_EXCEPTIONS in tests/it/architecture.rs."
        ));
    }
    for (f, _) in MAP_DRAWING_EXCEPTIONS {
        if !used_exceptions.contains(*f) {
            errors.push(format!(
                "MAP_DRAWING_EXCEPTIONS lists {f}, which no longer names a drawing type: remove the entry."
            ));
        }
    }
    let wgsl: Vec<String> = fs::read_dir(src.join("map"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".wgsl"))
        .collect();
    if !wgsl.is_empty() {
        errors.push(format!("src/map holds shaders {wgsl:?}: shaders are `render`'s (src/render)."));
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// A game's parts (weapons, movement, map loading ...) work without its
/// front end: only its `ui/` uses `ui`. And `CLIENT_GAME_EXCEPTIONS` lists
/// only files that still need it.
#[test]
fn game_parts_do_not_use_their_ui() {
    let src = root().join("src");
    let mut errors = Vec::new();
    for game in game_names(&src) {
        let mut files = Vec::new();
        rust_files(&src.join("games").join(&game), &mut files);
        for file in files {
            if module_of(&src, &file).as_deref() == Some("games_ui") {
                continue;
            }
            let code: String = fs::read_to_string(&file)
                .unwrap()
                .lines()
                .map(|l| l.split("//").next().unwrap_or(""))
                .collect::<Vec<_>>()
                .join(" ");
            let names_ui = [format!("{game}::ui::"), "super::ui::".into(), "self::ui::".into()]
                .iter()
                .any(|p| code.contains(p.as_str()));
            if names_ui {
                errors.push(format!(
                    "{}: uses `{game}::ui`, the game's front end. A game's parts must work without its menus \
                     and HUD (a dedicated server, another game's front end): move what both need out of `ui/`.",
                    file.display()
                ));
            }
        }
    }
    for (file, _) in CLIENT_GAME_EXCEPTIONS {
        let text = fs::read_to_string(root().join(file)).unwrap_or_default();
        if !game_names(&src).iter().any(|g| text.contains(&format!("games::{g}"))) {
            errors.push(format!(
                "{file} no longer uses a game's module: remove it from CLIENT_GAME_EXCEPTIONS in \
                 tests/it/architecture.rs."
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

fn tracked_files() -> Vec<String> {
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root())
        .output()
        .expect("git ls-files");
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Other games' assets must never be committed (README, "Assets, code and legal").
#[test]
fn no_game_assets_tracked() {
    const FORBIDDEN: &[&str] = &[
        "vpk", "bsp", "vtf", "vmt", "mdl", "vvd", "vtx", "phy", "rez", "ltb", "dtx", "dat", "wav", "mp3", "ogg",
    ];
    const MAX_BYTES: u64 = 1024 * 1024;
    let mut errors = Vec::new();
    for f in tracked_files() {
        let ext = Path::new(&f)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if FORBIDDEN.contains(&ext.as_str()) {
            errors.push(format!("{f}: game asset type .{ext} is tracked. Remove it with `git rm --cached`; assets load at runtime from the user's install."));
        }
        if let Ok(meta) = fs::metadata(root().join(&f))
            && meta.len() > MAX_BYTES
            && f != "Cargo.lock"
        {
            errors.push(format!(
                "{f}: {} bytes. Large files are probably assets or extracted data; keep them out of git.",
                meta.len()
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// Specs describe behavior; they must not carry source code
/// (specs/README.md, "Rules for spec writers").
#[test]
fn specs_contain_no_code_blocks() {
    let mut errors = Vec::new();
    for f in tracked_files() {
        if !f.starts_with("specs/") || !f.ends_with(".md") || f == "specs/README.md" {
            continue;
        }
        let text = fs::read_to_string(root().join(&f)).unwrap();
        for (i, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("```") && !t.trim_start_matches('`').trim().is_empty() && !t.contains("math") {
                errors.push(format!(
                    "{f}:{}: fenced code block `{t}`. Specs describe behavior as prose, tables and math; \
                     do not include source or pseudocode that mirrors it. Use ```math for formulas.",
                    i + 1
                ));
            }
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// Documents that CLAUDE.md points to must exist, so the map never rots.
#[test]
fn docs_map_links_resolve() {
    let text = fs::read_to_string(root().join("CLAUDE.md")).unwrap();
    let mut errors = Vec::new();
    for part in text.split("](").skip(1) {
        let target = part.split(')').next().unwrap_or("");
        if target.starts_with("http") || target.is_empty() {
            continue;
        }
        let path = target.split('#').next().unwrap();
        if !root().join(path).exists() {
            errors.push(format!(
                "CLAUDE.md links to {target}, which does not exist. Fix the link or restore the file."
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// The docs every session reads stay short (architecture review, P2): the
/// module list in docs/ARCHITECTURE.md and docs/tech-debt.md are one short
/// line per entry, with the detail in each file's `//!` doc, so features
/// don't edit (and parallel branches don't conflict on) paragraph-long rows.
#[test]
fn docs_stay_one_line_per_entry() {
    const MODULE_LINE: usize = 160;
    const DEBT_LINE: usize = 300;
    let mut errors = Vec::new();
    let arch = fs::read_to_string(root().join("docs/ARCHITECTURE.md")).unwrap();
    let modules = arch
        .split("\n## Modules\n")
        .nth(1)
        .expect("docs/ARCHITECTURE.md has a \"## Modules\" section")
        .split("\n## ")
        .next()
        .unwrap();
    for line in modules.lines().filter(|l| l.starts_with("- ")) {
        if line.chars().count() > MODULE_LINE {
            errors.push(format!(
                "docs/ARCHITECTURE.md module entry longer than {MODULE_LINE} characters: \
                 {line:.80}... Keep one short line (what it is, its layer) and put the \
                 detail in the file's `//!` doc."
            ));
        }
        let path = line[2..].split('`').nth(1).unwrap_or("");
        if ["src/", "source/", "engine/"].iter().any(|p| path.starts_with(p)) && !root().join(path).exists() {
            errors.push(format!(
                "docs/ARCHITECTURE.md lists {path}, which does not exist. Update the entry."
            ));
        }
    }
    let debt = fs::read_to_string(root().join("docs/tech-debt.md")).unwrap();
    for line in debt.lines().filter(|l| l.starts_with("- ")) {
        if line.chars().count() > DEBT_LINE {
            errors.push(format!(
                "docs/tech-debt.md entry longer than {DEBT_LINE} characters: {line:.80}... \
                 Keep one line (area, shortcut, where the detail is) and move the detail to \
                 the module's `//!` doc under \"Known shortcuts\"."
            ));
        }
        if let Some(path) = line.rsplit("(`").next().and_then(|p| p.strip_suffix("`)"))
            && path.starts_with("src/")
            && !root().join(path).exists()
        {
            errors.push(format!(
                "docs/tech-debt.md points to {path}, which does not exist. Update the entry."
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// Binaries built without a test harness (`test = false` in Cargo.toml)
/// must have no unit tests, or those would never run.
#[test]
fn untested_binaries_have_no_tests() {
    let manifest: toml::Table = fs::read_to_string(root().join("Cargo.toml")).unwrap().parse().unwrap();
    let mut errors = Vec::new();
    for bin in manifest.get("bin").and_then(|b| b.as_array()).into_iter().flatten() {
        if bin.get("test").and_then(|t| t.as_bool()) != Some(false) {
            continue;
        }
        let name = bin["name"].as_str().unwrap();
        let path = bin
            .get("path")
            .and_then(|p| p.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("src/bin/{name}.rs"));
        let text = fs::read_to_string(root().join(&path)).unwrap();
        if text.contains("#[test]") || text.contains("#[cfg(test)]") {
            errors.push(format!(
                "{path} has tests, but its [[bin]] `{name}` in Cargo.toml says `test = false`, so they never \
                 run. Remove that `test = false`."
            ));
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// The workspace (docs/ARCHITECTURE.md, "Crates"): every member is a
/// default member, so `cargo test` and `cargo nextest run` at the root (the
/// hooks' commands) test them all; the format crates stay pure data, with
/// no Bevy, physics or game crate under them; and the engine's crates
/// depend on no game crate (`mashup` is the game today).
#[test]
fn workspace_crates() {
    let manifest: toml::Table = fs::read_to_string(root().join("Cargo.toml")).unwrap().parse().unwrap();
    let ws = manifest["workspace"].as_table().expect("[workspace] in Cargo.toml");
    let list = |key: &str| -> BTreeSet<String> {
        ws.get(key)
            .and_then(|m| m.as_array())
            .into_iter()
            .flatten()
            .filter_map(|m| m.as_str().map(String::from))
            .collect()
    };
    let (members, defaults) = (list("members"), list("default-members"));
    let mut errors = Vec::new();
    for m in members.difference(&defaults) {
        errors.push(format!(
            "workspace member `{m}` isn't in `default-members` (Cargo.toml), so `cargo test` at the root \
             (and the git hooks) never test it. Add it there."
        ));
    }
    // Each member's dependencies that may not be (name prefixes), and why.
    const PURE_DATA: &str = "is pure data (docs/ARCHITECTURE.md, \"Crates\"). Keep the parsing there and \
        move what needs it (ECS types, components, conversion to engine types) into the crate that uses \
        it, e.g. `mashup::games::cs_source`.";
    const ENGINE: &str = "is the engine's base (docs/ARCHITECTURE.md, \"Crates\"): games build on it, \
        never the other way. Move the piece that needs the game up into `mashup`, or give the engine a \
        hook (a registry, a trait, a resource) that the game fills.";
    const FORBIDDEN: &[(&str, &[&str], &str)] = &[
        ("source/formats", &["bevy", "avian", "mashup"], PURE_DATA),
        ("engine/core", &["mashup", "cs_source", "combat_arms", "games"], ENGINE),
    ];
    for (krate, forbidden, why) in FORBIDDEN {
        let path = format!("{krate}/Cargo.toml");
        let text = fs::read_to_string(root().join(&path)).unwrap_or_else(|e| panic!("{path}: {e}"));
        let m: toml::Table = text.parse().unwrap();
        let mut tables: Vec<&toml::Table> = ["dependencies", "dev-dependencies", "build-dependencies"]
            .iter()
            .filter_map(|k| m.get(*k).and_then(|t| t.as_table()))
            .collect();
        for target in m.get("target").and_then(|t| t.as_table()).into_iter().flat_map(|t| t.values()) {
            tables.extend(
                ["dependencies", "dev-dependencies"]
                    .iter()
                    .filter_map(|k| target.get(*k).and_then(|t| t.as_table())),
            );
        }
        for dep in tables.iter().flat_map(|t| t.keys()) {
            if forbidden.iter().any(|f| dep.starts_with(f)) {
                errors.push(format!("{path} depends on `{dep}`: {krate} {why}"));
            }
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}

/// Integration tests are one test binary (tests/it/main.rs): any other
/// file in `tests/` would be its own binary, linking the whole game again
/// (docs/performance.md, "Test cycle"). And a file in tests/it/ without
/// its `mod` line is never compiled.
#[test]
fn tests_are_one_crate() {
    let tests = root().join("tests");
    let it = tests.join("it");
    let main = fs::read_to_string(it.join("main.rs")).unwrap();
    let (light, heavy) = main
        .split_once("mod heavy {")
        .expect("tests/it/main.rs has a `mod heavy { ... }` block");
    let declared = |text: &str, stem: &str| text.lines().any(|l| l.trim() == format!("mod {stem};"));
    let mut errors = Vec::new();
    for entry in fs::read_dir(&tests).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let own_binary = if path.is_dir() {
            name != "it" && path.join("main.rs").exists()
        } else {
            name.ends_with(".rs")
        };
        if own_binary {
            let stem = name.trim_end_matches(".rs");
            errors.push(format!(
                "tests/{name} is a test binary of its own, which links the whole game again. Move it into \
                 the one test crate: `git mv tests/{name} tests/it/` (or tests/it/heavy/ if it loads real \
                 maps or simulates for long: the fast tier skips those) and add `mod {stem};` to \
                 tests/it/main.rs (inside `mod heavy {{ }}` for heavy/). Run its tests with \
                 `cargo test --features dev --test it {stem}::`."
            ));
        }
    }
    for (dir, text, place) in [(it.clone(), light, "outside"), (it.join("heavy"), heavy, "inside")] {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let Some(stem) = name.strip_suffix(".rs") else { continue };
            if stem != "main" && !declared(text, stem) {
                errors.push(format!(
                    "{}: never compiled: add `mod {stem};` to tests/it/main.rs ({place} `mod heavy {{ }}`).",
                    path.strip_prefix(root()).unwrap().display()
                ));
            }
        }
    }
    assert!(errors.is_empty(), "\n{}\n", errors.join("\n"));
}
