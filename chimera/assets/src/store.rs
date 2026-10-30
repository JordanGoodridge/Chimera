//! The record store on disk: `records/<ab>/<id>.zst` (a file's plain bytes,
//! zstd level 3; `id` = lowercase hex SHA-256 of the plain bytes) and
//! `manifests/<game>/<build>.txt`. Writes are atomic (a temporary file,
//! then a rename), so packers running at once never leave half a record;
//! a record already there is never written again (dedup by id).

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

/// zstd level for records (the plan's section 2: level 3 packs at
/// 120-220 MB/s a thread and unpacks at 530-640 MB/s).
pub const LEVEL: i32 = 3;

/// The lowercase hex SHA-256 of `bytes`: a record's id.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Whether `id` looks like a record id (64 lowercase hex characters).
pub fn is_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A store folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Store {
    pub root: PathBuf,
}

/// What `put` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Put {
    pub id: String,
    /// The record's size on disk (compressed).
    pub stored: u64,
    /// False: the record was there already (dedup).
    pub new: bool,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The record file of `id`.
    pub fn record_path(&self, id: &str) -> PathBuf {
        self.root.join("records").join(&id[..2.min(id.len())]).join(format!("{id}.zst"))
    }

    /// The folder of `game`'s manifests.
    pub fn manifest_dir(&self, game: &str) -> PathBuf {
        self.root.join("manifests").join(game)
    }

    /// The manifest file of `game` at `build`.
    pub fn manifest_path(&self, game: &str, build: &str) -> PathBuf {
        self.manifest_dir(game).join(format!("{build}.txt"))
    }

    pub fn has(&self, id: &str) -> bool {
        self.record_path(id).is_file()
    }

    /// Store `bytes` as a record (unless one with its id is there).
    pub fn put(&self, bytes: &[u8]) -> io::Result<Put> {
        let id = sha256_hex(bytes);
        let path = self.record_path(&id);
        if let Ok(meta) = fs::metadata(&path) {
            return Ok(Put { id, stored: meta.len(), new: false });
        }
        let packed = zstd::bulk::compress(bytes, LEVEL)?;
        write_atomic(&path, &packed)?;
        Ok(Put { id, stored: packed.len() as u64, new: true })
    }

    /// Record `id`'s plain bytes, checked: exactly `size` bytes whose
    /// SHA-256 is `id`, else `InvalidData` (a corrupt or wrong record);
    /// `NotFound` when the store lacks it.
    pub fn get(&self, id: &str, size: u64) -> io::Result<Vec<u8>> {
        let path = self.record_path(id);
        let packed = fs::read(&path).map_err(|e| io::Error::new(e.kind(), format!("record {id}: {e}")))?;
        let bad = |what: String| io::Error::new(io::ErrorKind::InvalidData, format!("record {id}: {what}"));
        let size = usize::try_from(size).map_err(|_| bad("too large".into()))?;
        // One byte more than wanted, so a longer record shows as one.
        let plain = zstd::bulk::decompress(&packed, size + 1).map_err(|e| bad(format!("not zstd ({e})")))?;
        if plain.len() != size {
            return Err(bad(format!("{} bytes, the manifest says {size}", plain.len())));
        }
        if sha256_hex(&plain) != id {
            return Err(bad("hash mismatch (corrupt record)".into()));
        }
        Ok(plain)
    }

    /// Every record id in the store, sorted.
    pub fn record_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Ok(dirs) = fs::read_dir(self.root.join("records")) else { return out };
        for dir in dirs.flatten() {
            let Ok(files) = fs::read_dir(dir.path()) else { continue };
            for f in files.flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                if let Some(id) = name.strip_suffix(".zst").filter(|id| is_id(id)) {
                    out.push(id.to_string());
                }
            }
        }
        out.sort();
        out
    }

    /// The games with manifests, sorted.
    pub fn games(&self) -> Vec<String> {
        let mut out: Vec<String> = fs::read_dir(self.root.join("manifests"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    }

    /// `game`'s builds with a manifest, oldest first (numeric builds by
    /// number, then the rest by name).
    pub fn builds(&self, game: &str) -> Vec<String> {
        let mut out: Vec<String> = fs::read_dir(self.manifest_dir(game))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".txt").map(String::from))
            .collect();
        out.sort_by_key(|b| (b.parse::<u64>().map_or(1, |_| 0), b.parse::<u64>().unwrap_or(0), b.clone()));
        out
    }

    /// `game`'s newest build (`builds`' last), if it has a manifest.
    pub fn latest_build(&self, game: &str) -> Option<String> {
        self.builds(game).pop()
    }
}

/// Write `bytes` to `path` through a temporary file beside it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.{:?}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        std::thread::current().id()
    ));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}
