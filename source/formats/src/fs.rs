//! A read-only virtual filesystem over a game's archives and loose files:
//! `FileSource` (an archive such as `vpk::Vpk`, or a `LooseDir`), `Mount`
//! (layers searched in order, like a game's search path) and path
//! normalization.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

/// One file visible through a mount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Normalized path: lowercase, `/` separators, no leading slash.
    pub path: String,
    pub size: u64,
}

/// A source of files: an archive or a loose directory.
pub trait FileSource: Send + Sync {
    /// Human-readable name for listings and errors.
    fn name(&self) -> String;
    fn entries(&self) -> Vec<Entry>;
    /// `None` if this source doesn't have `path` (already normalized).
    fn read(&self, path: &str) -> Option<io::Result<Vec<u8>>>;
    /// The files directly in folder `dir` (normalized, no trailing slash),
    /// as normalized paths. Sources override it to avoid listing
    /// everything.
    fn files_in(&self, dir: &str) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|e| e.path.rsplit_once('/').is_some_and(|(d, _)| d == dir))
            .map(|e| e.path)
            .collect()
    }
}

/// Normalize a path the way Source and LithTech resolve them:
/// case-insensitive, either slash.
pub fn normalize(path: &str) -> String {
    // Doubled separators occur in game data (e.g. a model's texture
    // directory `models\props_junk\\`); file systems ignore them, so do we.
    // So are `.` components (static prop names like `./models/...` on
    // de_nuke and de_train).
    let path = path.replace('\\', "/").to_lowercase();
    path.split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Layers searched in order; the first layer that has a file wins, like a
/// game's search path.
#[derive(Default)]
pub struct Mount {
    layers: Vec<Box<dyn FileSource>>,
}

impl Mount {
    pub fn push(&mut self, layer: impl FileSource + 'static) {
        self.layers.push(Box::new(layer));
    }

    pub fn layers(&self) -> impl Iterator<Item = &dyn FileSource> {
        self.layers.iter().map(|l| l.as_ref())
    }

    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let path = normalize(path);
        self.layers
            .iter()
            .find_map(|l| l.read(&path))
            .unwrap_or_else(|| Err(io::Error::new(io::ErrorKind::NotFound, format!("not in mount: {path}"))))
    }

    /// The files directly in folder `dir` in any layer, sorted by path
    /// (one listing of a folder, e.g. CS:S's spray logos, without listing
    /// the whole install).
    pub fn files_in(&self, dir: &str) -> Vec<String> {
        let dir = normalize(dir);
        let mut seen = std::collections::BTreeSet::new();
        for layer in &self.layers {
            seen.extend(layer.files_in(&dir));
        }
        seen.into_iter().collect()
    }

    /// Every visible file, sorted by path, with the layer it comes from.
    /// Files shadowed by an earlier layer are omitted.
    pub fn entries(&self) -> Vec<(Entry, usize)> {
        let mut seen = BTreeMap::new();
        for (i, layer) in self.layers.iter().enumerate() {
            for e in layer.entries() {
                seen.entry(e.path.clone()).or_insert((e, i));
            }
        }
        seen.into_values().collect()
    }
}

/// Loose files under a directory.
pub struct LooseDir {
    root: PathBuf,
    /// Lowercase extensions to hide, e.g. archives that are mounted as their
    /// own layers.
    hidden_exts: Vec<&'static str>,
}

impl LooseDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            hidden_exts: Vec::new(),
        }
    }

    pub fn hiding(mut self, exts: &[&'static str]) -> Self {
        self.hidden_exts.extend_from_slice(exts);
        self
    }

    fn hidden(&self, path: &str) -> bool {
        path.rsplit_once('.')
            .is_some_and(|(_, ext)| self.hidden_exts.contains(&ext))
    }

    fn walk(dir: &Path, root: &Path, out: &mut Vec<Entry>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                Self::walk(&path, root, out);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(Entry {
                    path: normalize(&rel.to_string_lossy()),
                    size: meta.len(),
                });
            }
        }
    }
}

impl FileSource for LooseDir {
    fn name(&self) -> String {
        self.root.display().to_string()
    }

    fn entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        Self::walk(&self.root, &self.root, &mut out);
        out.retain(|e| !self.hidden(&e.path));
        out
    }

    fn files_in(&self, dir: &str) -> Vec<String> {
        // The folder, matched case-insensitively as `read` does.
        let mut at = self.root.clone();
        for part in dir.split('/').filter(|p| !p.is_empty()) {
            let direct = at.join(part);
            if direct.is_dir() {
                at = direct;
                continue;
            }
            let Some(found) = fs::read_dir(&at).ok().and_then(|rd| {
                rd.flatten()
                    .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))
            }) else {
                return Vec::new();
            };
            at = found.path();
        }
        let Ok(rd) = fs::read_dir(&at) else { return Vec::new() };
        rd.flatten()
            .filter(|e| e.metadata().is_ok_and(|m| m.is_file()))
            .map(|e| normalize(&format!("{dir}/{}", e.file_name().to_string_lossy())))
            .filter(|p| !self.hidden(p))
            .collect()
    }

    fn read(&self, path: &str) -> Option<io::Result<Vec<u8>>> {
        if self.hidden(path) {
            return None;
        }
        // Fast path: exact case. Otherwise match case-insensitively.
        let direct = self.root.join(path);
        if direct.is_file() {
            return Some(fs::read(direct));
        }
        let mut dir = self.root.clone();
        for part in path.split('/') {
            let found = fs::read_dir(&dir)
                .ok()?
                .flatten()
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
            dir = found.path();
        }
        dir.is_file().then(|| fs::read(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_in_lists_one_folder_of_every_layer() {
        let root = std::env::temp_dir().join(format!("mashup_files_in_{}", std::process::id()));
        let logos = root.join("Materials").join("VGUI").join("logos");
        fs::create_dir_all(logos.join("ui")).unwrap();
        fs::write(logos.join("Spray_A.vtf"), b"a").unwrap();
        fs::write(logos.join("ui").join("spray_a.vmt"), b"b").unwrap();
        let mut mount = Mount::default();
        mount.push(LooseDir::new(&root));
        assert_eq!(
            mount.files_in("materials/vgui/logos"),
            vec!["materials/vgui/logos/spray_a.vtf"]
        );
        assert!(mount.files_in("materials/none").is_empty());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn normalize_drops_empty_and_dot_components() {
        assert_eq!(super::normalize("./Models\\props//a.MDL"), "models/props/a.mdl");
    }
}
