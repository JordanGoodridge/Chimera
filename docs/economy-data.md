# Economy data

The shop is data loaded at run time, as weapons are (docs/weapon-data.md;
docs/plans/active/engine-split.md, "Data-driven first"): a TOML economy
document, parsed and applied by `chimera_sim`'s `weapon::economy_data`,
gives what is for sale besides weapons, the buy menu's layout, the spawn
kit per team, the shop's sounds and buy-name aliases. A weapon's own price,
team and bots' liking stay in its weapon document's `[buy]`. The rules of
buying (refusals, armour over armour, ammo by the box, autobuy and rebuy,
bots' buying) are `weapon::economy`'s code; they find items by what they
give (armour with or without a helmet, a defuse kit, night vision), not by
a game's names.

CS:S's document is `games/cs_source/data/economy.toml`: our own numbers
from the spec and the game's menus (specs/cs_source/weapons.md, "Economy";
specs/cs_source/objectives.md 5), not a file from the game. It is built
into the game (`weapons::cs_economy`) and loaded after its weapons.

## The document

```toml
[sounds]                        # sound entries; each given replaces the app's
ammo = "BaseCombatCharacter.AmmoPickup"      # ammo bought
pickup = "Player.PickupWeapon"               # a weapon bought or picked up
item = "BaseCombatCharacter.ItemPickup2"     # an item bought
night_vision_on = "Player.NightVisionOn"
night_vision_off = "Player.NightVisionOff"

[ammo]
bought_full = true              # a gun bought where ammo is sold comes full

[aliases]                       # other names autobuy lists use
mp5 = "mp5navy"

[[item]]                        # for sale, not a weapon (economy::ShopItem)
id = "cs_source:vesthelm"       # namespaced; `buy vesthelm` finds it too
label = "Kevlar + Helmet"       # its name in a menu
price = 1000
# team = 2                      # only this team buys it
armor = { amount = 1.0, helmet = true, helmet_price = 350 }
# part = "defuse_kit"           # or a part registered by name (ItemParts)
owned = "You already have Kevlar and a helmet!"   # refused when owned
# menu_greys_owned = true       # a game-look menu greys it while carried

[[menu]]                        # a buy-menu category (economy::BuyCategory)
key = 1                         # the number key that opens it
name = "Pistols"
items = [
    { buy = "cs_source:weapon_glock", label = "Glock" },  # a weapon: label needed
    { buy = "cs_source:vest" },                           # an item: its label
]
# direct = "primammo"           # bought at once instead (ammo, item, weapon)

[spawn]                         # the spawn kit (StartingWeapons)
kits = [
    { team = 1, weapons = ["cs_source:weapon_knife", "cs_source:weapon_glock"] },
    { weapons = ["cs_source:weapon_knife", "cs_source:weapon_usp"] },   # everyone else
]
all = ["cs_source:weapon_ak47"]   # for everyone, after the kit; the last given is drawn
```

## Rules

- **Items.** An item gives one of: `armor` (an amount, 1 full, keeping a
  helmet already worn; `helmet = true` adds one; over the amount already
  worn without a helmet it sells the helmet alone for `helmet_price`), or
  `part`, a name registered in `economy::ItemParts`. The engine registers
  `defuse_kit` (`DefuseKit`) and `night_vision` (`NightVision`); a game
  registers more with `ItemParts::register` before its data loads.
- **Menus.** Lines name a weapon (its ID, or as `WeaponRegistry::find`
  takes it) or an item. Without any menu the buy menu lists pistols,
  primaries and equipment (items and grenades) by slot.
- **Loading more.** `App::add_economy_data(&[(name, text)])` after the
  weapons it names. Documents add: an item or a category with an ID or key
  already there replaces it in place, a new one comes last; `[spawn]`
  replaces the kit; aliases and sounds given replace. CS:S keeps a spawn
  kit an app set before the game's plugin (tests).
- **Errors.** Unknown keys, an unknown weapon or part, a menu weapon
  without a label, a `direct` that buys nothing, an item giving nothing or
  two things, or with a weapon's ID, are load errors naming the document;
  nothing changes when one fails. CS:S's own data failing panics at start
  (its tests load it).
- Server and clients must load the same data (as they must run the same
  build): buying goes by name.

`games/cs_source/tests/it/economy_data.rs` adds a new item and a new menu
category from a test's data file
(games/cs_source/tests/data/shop/specials.toml) and buys from it.

## Round numbers

CS:S's rule module's numbers (`rules::rounds::RoundSettings`: timers,
start and most money, bonuses and rewards) are
`games/cs_source/data/rounds.toml`, read with
`chimera_sim::rule_module::parse_settings` (unknown keys are errors); its
cvars (`mp_startmoney`, ...) set them after. The rules themselves are Rust.

## Not data yet

Bots' buying thresholds and grenade chances (`economy::BOT_*`, ours, the
engine's), the default autobuy and rebuy lists (`AUTOBUY_DEFAULT`,
`REBUY_DEFAULT`, CS:S's `cfg/*_default.txt` entries), the refusal words
that aren't an item's (`INSUFFICIENT_FUNDS`, buy zones) and spawn buy
zones' size.
