//! The menu playlist (music_ambience.md 3): the options' music list with
//! its ticks, the built-in entries first and the player's own MP3s
//! (`Jukebox/`) after, and the choice of the next track in rotation or at
//! a random skip. Pure: the caller says which files can play.

/// One entry of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    /// The player's path (`music::data::normalize`d; a Jukebox file is
    /// `../jukebox/<name>.mp3`, below the install and not `Game/`).
    pub file: String,
    pub ticked: bool,
    pub jukebox: bool,
}

/// The saved ticks (`Game/Etc/BGMUserProfile.txt` as we keep it): the list's
/// version, one tick per built-in entry in order, and name and tick per
/// Jukebox file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedTicks {
    pub version: String,
    pub builtin: Vec<bool>,
    pub jukebox: Vec<(String, bool)>,
}

/// The playlist.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Playlist {
    pub version: String,
    pub entries: Vec<Entry>,
    /// The entry that played last; -1 before the first.
    pub cursor: i64,
    /// Random mode (`bgmrotation` 0); the default is rotation.
    pub random: bool,
}

/// Steps the search for the next track takes at most.
pub const GUARD: usize = 5000;

impl Playlist {
    pub fn new(version: &str, builtin: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            version: version.to_string(),
            entries: builtin
                .into_iter()
                .map(|(name, file)| Entry {
                    name,
                    file,
                    ticked: true,
                    jukebox: false,
                })
                .collect(),
            cursor: -1,
            random: false,
        }
    }

    /// Replace the Jukebox entries by `names` (file names without
    /// extension): files that have gone are dropped, new ones added
    /// ticked, the ones kept keep their tick.
    pub fn set_jukebox(&mut self, names: &[String]) {
        let old: Vec<Entry> = self.entries.iter().filter(|e| e.jukebox).cloned().collect();
        let playing = self.entries.get(self.cursor.max(0) as usize).map(|e| e.file.clone());
        self.entries.retain(|e| !e.jukebox);
        for n in names {
            let ticked = old.iter().find(|e| e.name == *n).is_none_or(|e| e.ticked);
            self.entries.push(Entry {
                name: n.clone(),
                file: format!("../jukebox/{}.mp3", n.to_ascii_lowercase()),
                ticked,
                jukebox: true,
            });
        }
        // The cursor follows the entry it was on.
        if let Some(f) = playing
            && self.cursor >= 0
        {
            self.cursor = self.entries.iter().position(|e| e.file == f).map_or(-1, |i| i as i64);
        }
    }

    /// An entry plays: ticked and `playable` (its file opens).
    pub fn eligible(&self, i: usize, playable: &dyn Fn(&str) -> bool) -> bool {
        self.entries.get(i).is_some_and(|e| e.ticked && playable(&e.file))
    }

    /// N: how many entries are eligible.
    pub fn count(&self, playable: &dyn Fn(&str) -> bool) -> usize {
        (0..self.entries.len()).filter(|&i| self.eligible(i, playable)).count()
    }

    /// Whether the cursor's entry is eligible (false at -1).
    pub fn cursor_eligible(&self, playable: &dyn Fn(&str) -> bool) -> bool {
        self.cursor >= 0 && self.eligible(self.cursor as usize, playable)
    }

    /// The next track: its file and whether it loops (N = 1). `draw(n)`
    /// gives a uniform integer in 0..n. None when nothing is eligible.
    pub fn next(
        &mut self,
        playable: &dyn Fn(&str) -> bool,
        mut draw: impl FnMut(usize) -> usize,
    ) -> Option<(String, bool)> {
        let n = self.count(playable);
        if n == 0 {
            return None;
        }
        let looping = n == 1;
        let mut skip = if self.random && n > 1 { draw(n).min(n - 1) } else { 0 };
        let len = self.entries.len() as i64;
        for _ in 0..GUARD {
            self.cursor = (self.cursor + 1).rem_euclid(len);
            if self.eligible(self.cursor as usize, playable) {
                if skip == 0 {
                    return Some((self.entries[self.cursor as usize].file.clone(), looping));
                }
                skip -= 1;
            }
        }
        None
    }

    /// The ticks to save.
    pub fn saved(&self) -> SavedTicks {
        SavedTicks {
            version: self.version.clone(),
            builtin: self.entries.iter().filter(|e| !e.jukebox).map(|e| e.ticked).collect(),
            jukebox: self
                .entries
                .iter()
                .filter(|e| e.jukebox)
                .map(|e| (e.name.clone(), e.ticked))
                .collect(),
        }
    }

    /// Apply saved ticks: of another version they are thrown away (the
    /// defaults stay); Jukebox files not in the save keep their default
    /// tick.
    pub fn apply(&mut self, saved: &SavedTicks) {
        if saved.version != self.version {
            return;
        }
        let mut i = 0;
        for e in self.entries.iter_mut().filter(|e| !e.jukebox) {
            if let Some(t) = saved.builtin.get(i) {
                e.ticked = *t;
            }
            i += 1;
        }
        for e in self.entries.iter_mut().filter(|e| e.jukebox) {
            if let Some((_, t)) = saved.jukebox.iter().find(|(n, _)| *n == e.name) {
                e.ticked = *t;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Playlist {
        let mut p = Playlist::new("v1", ["a", "b", "c"].map(|n| (n.to_string(), format!("bgm/{n}.mp3"))));
        p.entries[1].ticked = false;
        p.set_jukebox(&["J".to_string()]);
        p
    }

    fn order(p: &mut Playlist, playable: &dyn Fn(&str) -> bool, n: usize) -> Vec<String> {
        (0..n).map(|_| p.next(playable, |_| 0).unwrap().0).collect()
    }

    /// Rotation plays the eligible entries in order and wraps; a missing
    /// file is skipped like an unticked one.
    #[test]
    fn rotation_order() {
        let all = |_: &str| true;
        let mut p = list();
        assert_eq!(
            order(&mut p, &all, 4),
            ["bgm/a.mp3", "bgm/c.mp3", "../jukebox/j.mp3", "bgm/a.mp3"]
        );
        let no_b = |f: &str| f != "bgm/b.mp3";
        let mut p = list();
        p.entries[1].ticked = true;
        assert_eq!(
            order(&mut p, &no_b, 4),
            ["bgm/a.mp3", "bgm/c.mp3", "../jukebox/j.mp3", "bgm/a.mp3"]
        );
    }

    /// Random mode skips k eligible entries forward from the cursor; it
    /// can land on the current track.
    #[test]
    fn random_draws() {
        let all = |_: &str| true;
        let mut p = Playlist::new("v", ["a", "b", "c"].map(|n| (n.to_string(), n.to_string())));
        p.random = true;
        p.cursor = 0;
        for (k, want) in [(0, "b"), (1, "c"), (2, "a")] {
            p.cursor = 0;
            assert_eq!(
                p.next(&all, |n| {
                    assert_eq!(n, 3);
                    k
                })
                .unwrap()
                .0,
                want
            );
        }
    }

    /// One eligible track loops; none: no music.
    #[test]
    fn single_and_none() {
        let mut p = Playlist::new("v", [("a".to_string(), "a".to_string())]);
        assert_eq!(p.next(&|_| true, |_| 0), Some(("a".into(), true)));
        assert_eq!(p.next(&|_| false, |_| 0), None);
        p.entries[0].ticked = false;
        assert_eq!(p.next(&|_| true, |_| 0), None);
    }

    /// Saved ticks of another version are thrown away; Jukebox files
    /// come and go.
    #[test]
    fn saved_ticks() {
        let mut p = list();
        p.entries[0].ticked = false;
        let mut saved = p.saved();
        assert_eq!(saved.builtin, [false, false, true]);
        let mut q = list();
        q.entries[1].ticked = true;
        q.apply(&saved);
        assert!(!q.entries[0].ticked && !q.entries[1].ticked);
        saved.version = "v2".into();
        let mut r = list();
        r.apply(&saved);
        assert!(r.entries[0].ticked, "another version: the defaults");
        p.set_jukebox(&["K".to_string()]);
        assert_eq!(
            p.entries
                .iter()
                .filter(|e| e.jukebox)
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            ["K"]
        );
        assert!(p.entries[3].ticked, "a new file is ticked");
    }
}
