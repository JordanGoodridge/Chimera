# Plan: Combat Arms weapons' own fire, reload and draw timings

Status: 2026-10-29, parts 1 and 2 implemented (below); backlog
item "Combat Arms guns' own sounds, scopes and models: audit", fix 3.

Today every install copy fires, reloads and draws at its template's
numbers (every rifle copy at the M4A1's 105 ms, every sniper copy at the
L96A1's 1.4 s bolt) while its view model plays its own animations.

## What the specs say decides the times

- **Fire cycle** = the view model's `fire_0` length / $r_{fire}$,
  $r_{fire} = 1 + $ the record's `FireAniRate` $+$ the parts' fire bonuses
  (weapon_firing.md "Trigger and fire cycle"; view_models.md 7,
  Corrections 6). A long `fire_0` *is* the pump or bolt
  (weapon_classes.md "Fire cycle"). Any rate above 2.0 plays at 1.0.
- **Reload** = `reload` / $r_{reload}$, $r_{reload} = 1 +$ parts $+$ the
  vest's `ReloadAniRate` $+$ set bonus (no weapon-record field). A shell
  reloader (`ReloadPerBullet` 1) plays `PreReload`, `reload` per shell,
  `PostReload` (weapon_classes.md 5.5).
- **Draw** = `select` / $r_{draw}$, $r_{draw} = 1 +$ parts $+$ the vest's
  `SelectAniRate` $+$ set bonus (inventory.md 6.4).
- `FireDelay` is only the server's spacing check (already mapped, own on
  every copy); the client never reads it (view_models.md Q9).
- Each animation is found by name, first match, case ignored, in the
  model's combined list: its own animations, then its children's (the
  `ANI_*` child files; `model::load`, animation.md).

So the lengths live in the **model** (the `PVModelNormal` file and its
children); the rates in the **record** (`FireAniRate`), the parts and the
vest. Our documents' numbers (`trigger.cycle`, `magazine.reload`,
`magazine.shells`, `draw`) are those lengths at rate 1 for our weapons.

## How, without reading ~2,000 models at merge time

- The merge reads no model. It writes an empty `combat_arms:timing`
  table on each **copy** that is a gun (a trigger and a view model; not a
  knife or a grenade, whose hooks time them). Our documents get none:
  their numbers are the spec's and stay.
- The `combat_arms:timing` hook (`weapons::timing`) runs when a weapon
  entity is built, on whichever side builds it: the server's `give`, the
  client's copy of its inventory (`sync`), a remote player's held gun. It
  reads the weapon's `[models] view` and its children through the
  install mount (`world::mount`, headless: no render needed, the server
  already mounts the install for its maps), takes `fire_0`, `reload`,
  `PreReload`, `PostReload`, `select`, and caches them per model path in
  the world (`ViewTimes`, a resource: the server's and each client's;
  failures too). A missing or 0 ms animation, or a model
  that doesn't read, keeps the template's number.
- Rates on top: the cycle is divided by `fire_animation_rate`
  (`CaItem::fire_rate` = `FireAniRate`); with an attachments table the
  lengths become the `Stock` the parts' rates apply to (`apply_fitted`,
  `apply_draw`). The hook runs after the attachments, damage and
  inventory hooks (hooks build in name order), so it also replaces the
  server's reload window (`CaDamage::reload_third_person`) when that was
  the template's reload.
- `FireAniRate` everywhere: the merge makes `combat_arms:inventory` where
  the template has none (83 copies, and our documents without one: the
  MP5A4, M60, M92FS, R870, L96A1, Dragunov and the launchers), so the
  record's other item fields land with it.

## Client and server agreement

Shots are server-authoritative and the client predicts its own weapon
with its own `Trigger`, `Magazine` and `ShellReload` (not sent: only clip
and reserve are). Both sides build the weapon from the same document and
read the same install file, so they get the same numbers before the
first shot; the scenario test checks prediction stays error-free.
If the client can't read the model (no install, or no V2 key for the
rifle, SMG and sniper archives), or reads another version, the server's
numbers win: the hook leaves what it settled on the weapon
(`CaTimingValues`), the server sends it once per weapon id per client
(`weapon::params`, `net::weapon_params`) and the client's weapons of
that id take it over their own (open question 1, answered).

## Cost

One model read (about 2 ms, at most ~18 ms) when a weapon is first
built, once per model per world: in the gun game, the next weapon on a
kill. See "Measurements".

## Measurements

2026-10-29, this box, debug build, both keys (`cargo test -p
combat_arms --features dev --test it heavy::timings -- --nocapture`):

- The gun game's pool: 1,989 weapons with a `class`; 1,974 copies get
  the timing table (the rest are ours).
- 1,971 now fire, reload and draw on their own model's lengths; before,
  591 had the right fire cycle by chance (their template's equals
  theirs). 3 keep the template's: 2 whose model doesn't read
  (`abnormal_fortress_fire_target` and `_area`, the minigun launcher
  model) and 1 whose model has no `fire_0` (`groza_tropical`).
- 476 distinct view models (variants share them): 0.91 s to read all,
  1.9 ms a model on average, the slowest 17.7 ms. Done at merge time
  that would add about a second to every start; done when a weapon is
  first built it is a few milliseconds once per model.

## Parts

1. (Done 2026-10-29.) The hook, the merge's table, `FireAniRate`
   everywhere, tests: `tests/it/weapon_timings.rs` (copies with their
   view models' lengths written in the test fire, reload and draw on
   them, on the server and predicted; ours unchanged), the heavy
   `heavy::timings` over the install's pool.
2. (Done 2026-10-29.) The vest's `ReloadAniRate` / `SelectAniRate` and
   the sets' rates (effect kinds 10, 11, 12, player-wide and per weapon)
   in the weapon's own cycle, reload (a shell reload's three parts too)
   and draw: one sum with the parts' (attachments.md "Animation rates":
   $r_{reload} = 1 + \sum_{socket}$ `Reloadanirate` $+ r_{model} + r_{set}$,
   capped as the rest). `gear::CaGearRates` on every weapon with an
   attachments table, derived each predicted tick from its owner's look
   (`Selection::model`: the vest) and set tiers (`CaGearEffects`), on the
   server and the client alike from the replicated look and loadout;
   `attachments::apply_fitted` / `apply_draw` add it, the view model
   plays at the same rates. The gear records now go to a client whose
   own differ (`data::served::GEAR`, `GearData::to_doc`, net34), so both
   derive the same rates and movement penalties. Not pushed per weapon:
   derived from replicated data. Test:
   `weapon_timings::a_vests_rates_shorten_the_reload_and_draw_on_both_sides`
   (the M4A1's 2.3 s reload in 1.84 s and 1.2 s draw in 0.8 s on both
   sides, a client on our gear records takes the server's, 0 prediction
   errors over the reload).
3. Open: the rest of the fire sequence from the model: a semi-automatic's
   `postfire` (M92FS 180 + 150 ms), the pre-fire's presence
   (`trigger.latency`), a minigun copy's spin lengths, a knife copy's
   swing and hit keys, a flame copy's fire keys.

## Open questions

1. Answered 2026-10-29 (user: "help the clients as much as possible
   by pushing from the server where there are desyncs"): the server
   sends each weapon's settled timing once per weapon id per client
   (`weapon::params` codec `combat_arms:timing`, `WeaponParams`, net32);
   a client's own read never overrides it. Tests:
   `weapon_timings::a_client_without_the_models_predicts_on_the_servers_timing`,
   `a_client_with_its_own_models_takes_the_servers_timing`.
2. The third-person reload window (weapon_classes.md, server reload
   bookkeeping) is the third-person reload animation's length per stance;
   we use the first-person `reload` for it, as before.
3. Whether to read ahead (a background read of the next rung's model in
   the gun game) if a hitch shows on slow disks.
4. The gear's rates reach only weapons with an attachments table (every
   gun): a knife's, grenade's or special weapon's draw keeps its own
   length, though inventory.md 6.4 adds the model's select bonus to
   every draw. The view model plays those at the vest's rates as before.
