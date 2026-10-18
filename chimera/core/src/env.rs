//! The names Chimera had before its rename, still read: environment
//! variables are `CHIMERA_*`, and each one's old name (`MASHUP_*`) is read
//! when the new one isn't set (`var`, `var_os`); the per-user folders are
//! `chimera` (`user_folder`), and a user with only the old `mashup` folder
//! keeps using it.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// The prefix of our environment variables.
pub const PREFIX: &str = "CHIMERA_";

/// Their prefix before the rename (read when the new name isn't set).
pub const OLD_PREFIX: &str = "MASHUP_";

/// The per-user folder's name (in the user's data folders).
pub const FOLDER: &str = "chimera";

/// Its name before the rename (`user_folder`).
pub const OLD_FOLDER: &str = "mashup";

/// The variable's old name: `CHIMERA_DEMOS` was `MASHUP_DEMOS`.
pub fn old_name(name: &str) -> Option<String> {
    name.strip_prefix(PREFIX).map(|rest| format!("{OLD_PREFIX}{rest}"))
}

/// `std::env::var_os(name)`, else (for a `CHIMERA_*` name) its old
/// `MASHUP_*` name's value.
pub fn var_os(name: &str) -> Option<OsString> {
    std::env::var_os(name).or_else(|| std::env::var_os(old_name(name)?))
}

/// `std::env::var(name)`, else (for a `CHIMERA_*` name) its old
/// `MASHUP_*` name's value.
pub fn var(name: &str) -> Result<String, std::env::VarError> {
    match std::env::var(name) {
        Err(std::env::VarError::NotPresent) => match old_name(name) {
            Some(old) => std::env::var(old),
            None => Err(std::env::VarError::NotPresent),
        },
        found => found,
    }
}

/// Our folder in the user's data folder `base`: `base/chimera`, unless
/// only the old `base/mashup` exists (made before the rename; it is
/// used as it is, not moved: older builds may still run beside this one).
pub fn user_folder(base: &Path) -> PathBuf {
    let new = base.join(FOLDER);
    let old = base.join(OLD_FOLDER);
    if !new.exists() && old.is_dir() { old } else { new }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_names() {
        assert_eq!(old_name("CHIMERA_DEMOS").as_deref(), Some("MASHUP_DEMOS"));
        assert_eq!(old_name("HOME"), None);
    }

    #[test]
    fn the_old_folder_is_used_until_the_new_one_exists() {
        let base = std::env::temp_dir().join(format!("chimera_env_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(user_folder(&base), base.join("chimera"), "neither: the new one");
        std::fs::create_dir_all(base.join("mashup")).unwrap();
        assert_eq!(user_folder(&base), base.join("mashup"), "only the old one");
        std::fs::create_dir_all(base.join("chimera")).unwrap();
        assert_eq!(user_folder(&base), base.join("chimera"), "both: the new one");
        std::fs::remove_dir_all(&base).unwrap();
    }
}
