//! WAREHOUSE from the asset store alone (docs/plans/active/asset-service.md,
//! M1): a load from the install is recorded (`mount::served::record`), the
//! files it read are packed into a scratch store, decrypted
//! (`chimera_assets::pack`; the keys stay in this process), and the map
//! loads again from a mount of only that store's records: the same map, no
//! file missing, no key needed. Skipped without a Combat Arms install and
//! its keys. The store is a scratch folder, removed after.

use std::{
    fmt::Write as _,
    hash::{DefaultHasher, Hasher},
    time::Instant,
};

use chimera_core::mount::{
    Mount,
    assets::{CachedFiles, Manifest, Recorder, Store, pack},
    config::LocalConfig,
    install, served,
};
use chimera_world::map::MapData;
use combat_arms::game::{GAME, mount, world};

/// `Debug` text fed to a hasher, so a whole map compares without holding
/// its text.
struct HashWriter(DefaultHasher);

impl std::fmt::Write for HashWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

/// What must match between two loads: every mesh and texture (texels
/// included), the triangle count, the kit's characters.
fn summary(map: &MapData) -> (usize, usize, usize, usize, u64) {
    let mut h = HashWriter(DefaultHasher::new());
    write!(h, "{:?}{:?}", map.meshes, map.textures).unwrap();
    (map.triangle_count(), map.meshes.len(), map.textures.len(), map.kit.characters.len(), h.0.finish())
}

#[test]
fn warehouse_loads_from_the_store_alone() {
    let Some(found) = LocalConfig::load().ok().and_then(|c| install::resolve(&c, GAME, None).install) else {
        eprintln!("skipping: no Combat Arms install configured");
        return;
    };
    let Ok(installed) = mount::open_install(&found.path) else {
        eprintln!("skipping: no Combat Arms install at {}", found.path.display());
        return;
    };
    let recorder = Recorder::in_memory();
    let recorded = served::record(installed, GAME, &recorder);
    let started = Instant::now();
    let map = world::load_from(&recorded, "warehouse").unwrap();
    let install_took = started.elapsed();
    let from_install = summary(&map);
    drop(map);
    let paths: Vec<String> = recorder.lines().into_iter().map(|l| l.path).collect();
    let read: u64 = recorder.lines().iter().map(|l| l.size).sum();

    let dir = std::env::temp_dir().join(format!("chimera_served_warehouse_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::new(&dir);
    let mut manifest = Manifest::new(GAME, "test", None);
    let report = pack(&recorded, &paths, &store, &mut manifest, Some("warehouse"), 4).unwrap();
    manifest.save(&store.manifest_path(GAME, "test")).unwrap();
    assert!(report.missing.is_empty(), "{:?}", report.missing);

    let mut from_store = Mount::default();
    from_store.push(CachedFiles::open(&store, GAME, "test").unwrap());
    let started = Instant::now();
    let map = world::load_from(&from_store, "warehouse").unwrap();
    let store_took = started.elapsed();
    let served_map = summary(&map);
    eprintln!(
        "warehouse: {} files, {:.1} MB read, {} records, {:.1} MB stored, packed in {:.2} s; \
         load {:.2} s from the install (first in the process), {:.2} s from the store",
        report.files,
        read as f64 / 1e6,
        report.records,
        report.stored as f64 / 1e6,
        report.took.as_secs_f64(),
        install_took.as_secs_f64(),
        store_took.as_secs_f64()
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(served_map, from_install, "the same map from the store");
    assert!(report.files > 100, "{} files", report.files);
}
