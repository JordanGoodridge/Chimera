//! A background-music player for any game: two stream slots with a
//! linear fade-in or cross-fade, on a wall clock the caller supplies
//! (milliseconds), pure state with no audio so a headless test reads the
//! gains the presentation will play. Combat Arms' music is its first user
//! (specs/combat_arms/music_ambience.md 2); a game with streamed music
//! (not the mixer's sound entries) can use it as it is.
//!
//! - `start` puts a track in a slot: at once at full volume (fade 0) or
//!   fading in while the previous one fades out; the same file as the one
//!   last started is ignored; a file the caller cannot open leaves the
//!   slot empty.
//! - `update` (once a frame) moves the fade: while the elapsed time is
//!   below the fade the new stream has `e / T`, the old `1 - e / T`; past
//!   it a cross-fade's new stream becomes the only one at gain 1.
//! - `near_end` says when the only stream has less than the fade left in
//!   its current pass (a playlist starts the next track then).
//!
//! A stream remembers when it started, so its position (and whether it
//! has ended) follows the clock whether or not anyone hears it: a muted
//! player still advances.
//!
//! Known shortcuts: none beyond the caller's (the master volume is the
//! presentation's: `gains` are the slots' alone).

/// One playing stream.
#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub file: String,
    pub looping: bool,
    /// Wall clock (ms) when it started from its beginning.
    pub started: f64,
    /// Slot gain 0..1 (the fade; the master volume is not in it).
    pub gain: f32,
}

impl Stream {
    /// How far into the file it is (ms), looping past `length` (ms) when
    /// it loops; a one-shot stays at its end.
    pub fn position(&self, now: f64, length: Option<f64>) -> f64 {
        let e = (now - self.started).max(0.0);
        match length {
            Some(len) if len > 0.0 => {
                if self.looping {
                    e % len
                } else {
                    e.min(len)
                }
            }
            _ => e,
        }
    }

    /// A one-shot that has played to its end (the slot stays occupied).
    pub fn ended(&self, now: f64, length: Option<f64>) -> bool {
        !self.looping && length.is_some_and(|len| now - self.started >= len)
    }
}

/// The player: a primary and a secondary slot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicPlayer {
    pub primary: Option<Stream>,
    pub secondary: Option<Stream>,
    /// The file last started (compared without case); cleared by `stop`.
    pub last: Option<String>,
    /// When the last change began (ms) and its fade time T (ms).
    pub changed: f64,
    pub fade: f64,
}

impl MusicPlayer {
    /// Start `file` (looping or not) with a fade of `fade_ms`. `opens`:
    /// whether the file can be opened. Returns whether a stream started.
    pub fn start(&mut self, file: &str, looping: bool, fade_ms: f64, now: f64, opens: bool) -> bool {
        if file.is_empty() || self.last.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(file)) {
            return false;
        }
        let stream = |gain: f32| Stream {
            file: file.to_string(),
            looping,
            started: now,
            gain,
        };
        let new = opens.then(|| stream(if fade_ms <= 0.0 { 1.0 } else { 0.0 }));
        if self.primary.is_none() || fade_ms <= 0.0 {
            // The slot choice's first branch. (The first game left the old
            // primary playing unowned when this replaced it; it is closed
            // here.)
            self.primary = new;
            self.secondary = None;
        } else if self.secondary.is_some() {
            // A cross-fade is running: the one fading out is cut, the one
            // fading in is the outgoing one at the gain it reached.
            self.primary = self.secondary.take();
            self.secondary = new;
        } else {
            self.secondary = new;
        }
        if !opens {
            return false;
        }
        self.changed = now;
        self.fade = fade_ms.max(0.0);
        self.last = Some(file.to_string());
        true
    }

    /// Close both slots.
    pub fn stop(&mut self) {
        self.primary = None;
        self.secondary = None;
        self.fade = 0.0;
        self.last = None;
    }

    /// Whether anything is loaded (a one-shot that ended still is).
    pub fn playing(&self) -> bool {
        self.primary.is_some() || self.secondary.is_some()
    }

    /// Move the fade to `now` (once a frame).
    pub fn update(&mut self, now: f64) {
        let e = now - self.changed;
        if e < self.fade {
            let new = (e / self.fade).clamp(0.0, 1.0) as f32;
            match (&mut self.primary, &mut self.secondary) {
                (Some(p), None) => p.gain = new,
                (Some(p), Some(s)) => {
                    p.gain = 1.0 - new;
                    s.gain = new;
                }
                (None, Some(s)) => s.gain = new,
                (None, None) => {}
            }
        } else if let Some(mut s) = self.secondary.take() {
            s.gain = 1.0;
            self.primary = Some(s);
        } else if let Some(p) = &mut self.primary
            && p.gain < 0.99
        {
            p.gain = 1.0;
        }
    }

    /// The only stream has at most the fade time left in its current pass
    /// (a fade of 0 is never near its end). `length` gives a file's
    /// length in ms.
    pub fn near_end(&self, now: f64, length: impl Fn(&str) -> Option<f64>) -> bool {
        if self.secondary.is_some() || self.fade <= 0.0 {
            return false;
        }
        self.near_end_within(now, self.fade, length)
    }

    /// `near_end` with a window of its own (ms) instead of the last
    /// change's fade: the only stream has at most `window_ms` left in its
    /// current pass. For a stream that started with no fade (a party
    /// match's start track) whose successor does fade in.
    pub fn near_end_within(&self, now: f64, window_ms: f64, length: impl Fn(&str) -> Option<f64>) -> bool {
        if self.secondary.is_some() || window_ms <= 0.0 {
            return false;
        }
        let Some(p) = &self.primary else { return false };
        let Some(len) = length(&p.file).filter(|l| *l > 0.0) else {
            return false;
        };
        let left = len - p.position(now, Some(len));
        // A one-shot at its end has nothing left in a pass of its own.
        left <= window_ms
    }

    /// The streams and their slot gains, old first.
    pub fn gains(&self) -> impl Iterator<Item = (&Stream, f32)> {
        self.primary.iter().chain(self.secondary.iter()).map(|s| (s, s.gain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gains(p: &MusicPlayer) -> Vec<(String, f32)> {
        p.gains().map(|(s, g)| (s.file.clone(), g)).collect()
    }

    /// A fade-in from silence, linear in time, then full.
    #[test]
    fn fades_in_linearly() {
        let mut p = MusicPlayer::default();
        assert!(p.start("a.mp3", true, 10_000.0, 0.0, true));
        for (t, want) in [
            (0.0, 0.0),
            (2_500.0, 0.25),
            (5_000.0, 0.5),
            (10_000.0, 1.0),
            (30_000.0, 1.0),
        ] {
            p.update(t);
            assert!((p.primary.as_ref().unwrap().gain - want).abs() < 1e-6, "t = {t}");
        }
    }

    /// Fade 0 plays at once; the same file is ignored whatever its case.
    #[test]
    fn immediate_and_same_file() {
        let mut p = MusicPlayer::default();
        assert!(p.start("A.mp3", false, 0.0, 0.0, true));
        assert_eq!(gains(&p), [("A.mp3".into(), 1.0)]);
        assert!(!p.start("a.MP3", false, 0.0, 5.0, true));
        assert!(!p.start("", false, 0.0, 5.0, true));
        p.stop();
        assert!(p.start("a.mp3", false, 0.0, 6.0, true), "stop forgets the last name");
    }

    /// A cross-fade: old and new share the fade; at its end the new one
    /// is alone at 1; two starts in one fade cut the outgoing one.
    #[test]
    fn cross_fades() {
        let mut p = MusicPlayer::default();
        p.start("a", true, 10_000.0, 0.0, true);
        p.update(10_000.0);
        p.start("b", true, 10_000.0, 50_000.0, true);
        p.update(52_500.0);
        assert_eq!(gains(&p), [("a".into(), 0.75), ("b".into(), 0.25)]);
        p.update(60_000.0);
        assert_eq!(gains(&p), [("b".into(), 1.0)]);
        // Two starts during one cross-fade.
        p.start("c", true, 10_000.0, 70_000.0, true);
        p.update(74_000.0);
        p.start("d", true, 10_000.0, 74_000.0, true);
        assert_eq!(
            p.primary.as_ref().unwrap().file,
            "c",
            "the fading-in one is outgoing now"
        );
        assert_eq!(p.secondary.as_ref().unwrap().file, "d");
        p.update(74_000.0);
        assert!(
            (p.primary.as_ref().unwrap().gain - 1.0).abs() < 1e-6,
            "from the new fade's start"
        );
        assert_eq!(p.secondary.as_ref().unwrap().gain, 0.0);
    }

    /// A file that cannot open leaves its slot empty; the one-shot that
    /// ends keeps the slot.
    #[test]
    fn unopenable_and_ended() {
        let mut p = MusicPlayer::default();
        assert!(!p.start("missing", true, 0.0, 0.0, false));
        assert!(!p.playing());
        assert_eq!(p.last, None);
        p.start("a", false, 0.0, 0.0, true);
        let s = p.primary.clone().unwrap();
        assert!(!s.ended(1_000.0, Some(2_000.0)));
        assert!(s.ended(2_000.0, Some(2_000.0)));
        assert!(p.playing());
    }

    /// Near the end: the fade or less left; never with a zero fade or
    /// two streams.
    #[test]
    fn near_the_end() {
        let len = |_: &str| Some(60_000.0);
        let mut p = MusicPlayer::default();
        p.start("a", false, 10_000.0, 0.0, true);
        assert!(!p.near_end(49_999.0, len));
        assert!(p.near_end(50_000.0, len));
        let mut z = MusicPlayer::default();
        z.start("a", false, 0.0, 0.0, true);
        assert!(!z.near_end(59_000.0, len));
        p.update(10_000.0);
        p.start("b", false, 10_000.0, 50_000.0, true);
        assert!(!p.near_end(51_000.0, len));
    }
}
