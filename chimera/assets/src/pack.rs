//! Filling a store from a mount: each path read through the mount (so the
//! game's search order and, for Combat Arms, its decryption on this host
//! apply), stored as a record, and entered in the build's manifest; a set
//! name, if given, is replaced by exactly the paths packed. Reads and
//! compression run on `threads` threads; the result doesn't depend on
//! their order (records are content-addressed, the manifest sorted).

use std::{
    io,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use chimera_vfs::{Mount, normalize};

use crate::{FileRecord, Manifest, Store};

/// What a `pack` did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackReport {
    /// Files packed (paths).
    pub files: usize,
    /// Plain bytes read through the mount.
    pub bytes_read: u64,
    /// Distinct records among them.
    pub records: usize,
    /// Records written now (the rest were in the store).
    pub new_records: usize,
    /// Stored bytes of the distinct records (compressed).
    pub stored: u64,
    /// Paths the mount doesn't have (left out).
    pub missing: Vec<String>,
    pub took: Duration,
}

/// Pack `paths` from `mount` into `store`, entering them in `manifest`
/// (and as set `set`).
pub fn pack(
    mount: &Mount,
    paths: &[String],
    store: &Store,
    manifest: &mut Manifest,
    set: Option<&str>,
    threads: usize,
) -> io::Result<PackReport> {
    let started = Instant::now();
    let mut paths: Vec<String> = paths.iter().map(|p| normalize(p)).collect();
    paths.sort();
    paths.dedup();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(String, io::Result<(FileRecord, bool)>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(i) else { break };
                    let result = mount.read(path).and_then(|bytes| {
                        let put = store.put(&bytes)?;
                        Ok((FileRecord { id: put.id, size: bytes.len() as u64, stored: put.stored }, put.new))
                    });
                    results.lock().unwrap_or_else(|e| e.into_inner()).push((path.clone(), result));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap_or_else(|e| e.into_inner());
    results.sort_by(|a, b| a.0.cmp(&b.0));
    let mut report = PackReport::default();
    let mut packed = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeMap::new();
    for (path, result) in results {
        match result {
            Ok((record, new)) => {
                report.files += 1;
                report.bytes_read += record.size;
                let (_, was_new) = ids.entry(record.id.clone()).or_insert((record.stored, false));
                *was_new |= new;
                manifest.files.insert(path.clone(), record);
                packed.insert(path);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => report.missing.push(path),
            Err(e) => return Err(io::Error::new(e.kind(), format!("{path}: {e}"))),
        }
    }
    report.records = ids.len();
    report.stored = ids.values().map(|v| v.0).sum();
    // Two threads may both have written a record with the same bytes:
    // count each record once.
    report.new_records = ids.values().filter(|v| v.1).count();
    if let Some(set) = set {
        manifest.sets.insert(set.to_string(), packed);
    }
    report.took = started.elapsed();
    Ok(report)
}
