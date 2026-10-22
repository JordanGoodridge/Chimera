//! CS:S's guns' display names from the install's own strings
//! (`weapons::display_names`), what an app showing them under another
//! game's HUD uses (the mashup, Lucker Party): every gun has one, not its
//! `Cstrike_WPNHUD_*` token. Skips without a CS:S install.

use cs_source::game::{INFO, weapons};

#[test]
fn every_gun_is_named_by_the_installs_strings() {
    let Some(mount) = INFO.mount() else {
        eprintln!("skipping: no CS:S install configured");
        return;
    };
    let names = weapons::display_names(&mount);
    for gun in weapons::guns() {
        let name = names
            .iter()
            .find(|(id, _)| *id == gun.id)
            .unwrap_or_else(|| panic!("{} has no display name in {names:?}", gun.id));
        assert!(!name.1.to_lowercase().contains("wpnhud"), "{}: {}", gun.id, name.1);
    }
    let glock = names.iter().find(|(id, _)| id == weapons::GLOCK).expect("the Glock");
    assert_eq!(glock.1, "9X19MM SIDEARM", "CS:S's own name for the Glock");
}
