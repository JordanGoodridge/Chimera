//! The chat's spam limits (specs/combat_arms/radio_chat.md L1 to L3),
//! all as state machines on an explicit clock in milliseconds: the
//! sender's client (a rate window with a lock, a repeat lock), the
//! whispers' limits (global and per recipient), the lobby's repeat lock
//! and the server's silent limiter.

/// The limiter constants of a region (Open question 4: non-BR is the
/// default).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    /// Sends allowed in one window, and the window, ms.
    pub rate_slots: usize,
    pub rate_window: f64,
    /// The lock after a refused send, ms.
    pub rate_block: f64,
    pub whisper_slots: usize,
    pub whisper_window: f64,
    pub whisper_repeat_lock: f64,
}

impl Region {
    pub const NON_BR: Region = Region {
        rate_slots: 20,
        rate_window: 60_000.0,
        rate_block: 180_000.0,
        whisper_slots: 20,
        whisper_window: 180_000.0,
        whisper_repeat_lock: 60_000.0,
    };
    pub const BR: Region = Region {
        rate_slots: 5,
        rate_window: 10_000.0,
        rate_block: 60_000.0,
        whisper_slots: 10,
        whisper_window: 60_000.0,
        whisper_repeat_lock: 5_000.0,
    };
}

impl Default for Region {
    fn default() -> Self {
        Self::NON_BR
    }
}

/// Identical consecutive sends before the lock, and the lock, ms.
pub const REPEAT_COUNT: i32 = 10;
pub const REPEAT_LOCK: f64 = 60_000.0;
/// The lobby's: identical consecutive lines before its lock.
pub const LOBBY_REPEAT_COUNT: i32 = 3;
/// Identical whispers to one recipient, after the first copy.
pub const WHISPER_REPEAT_LIMIT: u32 = 3;
/// Per-recipient whisper records kept (plus the one being added).
pub const WHISPER_RECORDS: usize = 25;
/// Names in the Up-arrow list.
pub const RECENT_NAMES: usize = 5;

/// The repeat lock (L2, client and lobby): a counter and the last text.
#[derive(Clone, Debug, PartialEq)]
pub struct Repeat {
    count: i32,
    start: i32,
    last: Option<String>,
    locked_at: f64,
}

impl Repeat {
    pub fn new(count: i32) -> Self {
        Self {
            count,
            start: count,
            last: None,
            locked_at: 0.0,
        }
    }

    /// Before a send: true when it is refused (the counter spent and the
    /// lock not yet 60000 ms old; past it the counter starts over).
    pub fn refused(&mut self, now: f64) -> bool {
        if self.count <= 0 {
            if now - self.locked_at >= REPEAT_LOCK {
                self.count = self.start;
            } else {
                return true;
            }
        }
        false
    }

    /// After a successful send of `text`.
    pub fn sent(&mut self, text: &str, now: f64) {
        if self.last.as_deref() == Some(text) {
            self.count -= 1;
            if self.count <= 0 {
                self.locked_at = now;
            }
        } else {
            self.last = Some(text.to_string());
            self.count = self.start - 1;
        }
    }
}

/// The sender's client limits for all and team chat and lobby lines.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientLimits {
    pub region: Region,
    sent: Vec<f64>,
    block_start: Option<f64>,
    pub repeat: Repeat,
    pub lobby: Repeat,
}

impl Default for ClientLimits {
    fn default() -> Self {
        Self::new(Region::NON_BR)
    }
}

impl ClientLimits {
    pub fn new(region: Region) -> Self {
        Self {
            region,
            sent: Vec::new(),
            block_start: None,
            repeat: Repeat::new(REPEAT_COUNT),
            lobby: Repeat::new(LOBBY_REPEAT_COUNT),
        }
    }

    /// L1: whether a send at `now` is refused. A refusal with no block
    /// running starts one; refusals in a block do not extend it. Refused
    /// attempts take no slot.
    pub fn rate_refused(&mut self, now: f64) -> bool {
        let r = self.region;
        let blocked = self.block_start.is_some_and(|b| now < b + r.rate_block);
        let held = self.sent.iter().filter(|s| now - **s < r.rate_window).count();
        let refused = blocked || held >= r.rate_slots;
        if refused && !blocked {
            self.block_start = Some(now);
        }
        refused
    }

    /// Take a slot for a send at `now`.
    pub fn rate_take(&mut self, now: f64) {
        let window = self.region.rate_window;
        self.sent.retain(|s| now - *s < window);
        self.sent.push(now);
    }
}

/// Why a whisper was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhisperRefusal {
    /// Every slot held: `$BlockWhisper`.
    Global,
    /// This recipient is locked: `$UserSpamBlockWhisper`.
    Recipient,
}

#[derive(Clone, Debug, PartialEq)]
struct Record {
    to: String,
    count: u32,
    last: String,
    locked_at: Option<f64>,
}

/// The whispers' limits (L3) and the Up-arrow list (T3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WhisperLimits {
    pub region: Region,
    slots: Vec<f64>,
    /// Least recently used first.
    records: Vec<Record>,
    /// Most recent first.
    pub recent: Vec<String>,
    /// The entry Up showed last.
    pub cursor: Option<usize>,
}

impl WhisperLimits {
    pub fn new(region: Region) -> Self {
        Self {
            region,
            ..Default::default()
        }
    }

    /// W1 steps 1 and 2 for a whisper `text` to `to` at `now`; on
    /// success the global slot is taken and the record updated.
    pub fn try_send(&mut self, to: &str, text: &str, now: f64) -> Result<(), WhisperRefusal> {
        let r = self.region;
        let held = self.slots.iter().filter(|s| now - **s < r.whisper_window).count();
        if held >= r.whisper_slots {
            return Err(WhisperRefusal::Global);
        }
        self.check_recipient(to, text, now)?;
        self.slots.retain(|s| now - *s < r.whisper_window);
        self.slots.push(now);
        Ok(())
    }

    fn check_recipient(&mut self, to: &str, text: &str, now: f64) -> Result<(), WhisperRefusal> {
        let lock = self.region.whisper_repeat_lock;
        let at = self.records.iter().position(|r| r.to == to);
        let Some(at) = at else {
            if self.records.len() > WHISPER_RECORDS {
                self.records.remove(0);
            }
            self.records.push(Record {
                to: to.to_string(),
                count: 0,
                last: text.to_string(),
                locked_at: None,
            });
            return Ok(());
        };
        // Most recently used on every check.
        let mut rec = self.records.remove(at);
        let result = match rec.locked_at {
            Some(t) if now - t > lock => {
                rec.locked_at = None;
                rec.count = 0;
                rec.last.clear();
                Ok(())
            }
            Some(_) => Err(WhisperRefusal::Recipient),
            None if rec.last == text => {
                rec.count += 1;
                if rec.count >= WHISPER_REPEAT_LIMIT {
                    rec.locked_at = Some(now);
                    Err(WhisperRefusal::Recipient)
                } else {
                    Ok(())
                }
            }
            None => {
                rec.count = 0;
                rec.last = text.to_string();
                Ok(())
            }
        };
        self.records.push(rec);
        result
    }

    /// T3: remember a whisper partner (not yourself; one already listed
    /// keeps its place).
    pub fn remember(&mut self, name: &str, own: &str) {
        if name == own || self.recent.iter().any(|n| n == name) {
            return;
        }
        self.recent.insert(0, name.to_string());
        self.recent.truncate(RECENT_NAMES);
    }

    /// Up: the next partner after the last one shown (wrapping), as the
    /// text to type.
    pub fn up(&mut self) -> Option<String> {
        if self.recent.is_empty() {
            return None;
        }
        let next = self.cursor.map_or(0, |c| (c + 1) % self.recent.len());
        self.cursor = Some(next);
        Some(format!("/w {} ", self.recent[next]))
    }
}

/// The server's limiter for one player's all and team chat (L1, L2),
/// silent.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerLimiter {
    slots: [Option<f64>; 20],
    block_end: f64,
    last: Option<String>,
    n: i32,
}

impl Default for ServerLimiter {
    fn default() -> Self {
        Self {
            slots: [None; 20],
            block_end: 0.0,
            last: None,
            n: 0,
        }
    }
}

const SERVER_WINDOW: f64 = 60_000.0;
const SERVER_BLOCK: f64 = 180_000.0;
const SERVER_REPEAT_BLOCK: f64 = 60_000.0;
const SERVER_REPEAT_COUNT: i32 = 10;

impl ServerLimiter {
    /// Whether the message `text` at `now` (ms) is relayed.
    pub fn accept(&mut self, now: f64, text: &str) -> bool {
        if now < self.block_end {
            return false;
        }
        match self.slots.iter_mut().find(|s| s.is_none_or(|t| now - t > SERVER_WINDOW)) {
            Some(slot) => *slot = Some(now),
            None => {
                self.block_end = now + SERVER_BLOCK;
                return false;
            }
        }
        // A message the repeat check drops has still taken its slot.
        if self.last.as_deref() == Some(text) {
            self.n += 1;
            if self.n >= SERVER_REPEAT_COUNT {
                self.last = None;
                self.n = 0;
                self.block_end = now + SERVER_REPEAT_BLOCK;
                return false;
            }
        } else {
            self.last = Some(text.to_string());
            self.n = 1;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_repeat_drops_the_tenth() {
        let mut s = ServerLimiter::default();
        let ok: Vec<bool> = (0..10).map(|i| s.accept(i as f64 * 1000.0, "go")).collect();
        assert_eq!(ok.iter().filter(|b| **b).count(), 9);
        assert!(!s.accept(68_999.0, "x"));
        assert!(s.accept(69_000.0, "x"));
    }
}
