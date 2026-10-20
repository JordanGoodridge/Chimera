//! Text in the chat (specs/combat_arms/radio_chat.md T1, F1, B1): the
//! text commands, the bad-word filter and the block list, as pure
//! functions on strings.
//!
//! The filter's words are data (`BadWords`): none are built in here (the
//! install's own lists are not read; the live list is the lobby's). The
//! tests use made-up words.

/// The nine text commands, in the order they are tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cmd {
    Clan,
    Normal,
    Block,
    UnBlock,
    Whisper,
    WhisperEnable,
    WhisperIgnore,
    WhisperUnignore,
    WhisperAutoAnswer,
}

/// Every command, in the order T1 tries them.
pub const ORDER: [Cmd; 9] = [
    Cmd::Clan,
    Cmd::Normal,
    Cmd::Block,
    Cmd::UnBlock,
    Cmd::Whisper,
    Cmd::WhisperEnable,
    Cmd::WhisperIgnore,
    Cmd::WhisperUnignore,
    Cmd::WhisperAutoAnswer,
];

impl Cmd {
    /// The string-table entry that names the command's aliases.
    pub fn string_id(self) -> &'static str {
        match self {
            Cmd::Clan => "ChatCMD_Clan",
            Cmd::Normal => "ChatCMD_Normal",
            Cmd::Block => "ChatCMD_Block",
            Cmd::UnBlock => "ChatCMD_UnBlock",
            Cmd::Whisper => "ChatCMD_Whisper",
            Cmd::WhisperEnable => "ChatCMD_WhisperEnable",
            Cmd::WhisperIgnore => "ChatCMD_WhisperIgnore",
            Cmd::WhisperUnignore => "ChatCMD_WhisperUnignore",
            Cmd::WhisperAutoAnswer => "ChatCMD_WhisperAutoAnswer",
        }
    }

    fn index(self) -> usize {
        ORDER.iter().position(|c| *c == self).expect("in ORDER")
    }
}

/// The commands' aliases: a value's first character is the separator and
/// begins every alias (`/x/xyz` gives `/x` and `/xyz`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Commands {
    aliases: [Vec<String>; 9],
}

impl Commands {
    /// Aliases from `(command, value)` pairs; a command not listed has
    /// none.
    pub fn from_values(values: &[(Cmd, &str)]) -> Self {
        let mut c = Self::default();
        for (cmd, value) in values {
            c.set(*cmd, value);
        }
        c
    }

    /// Our aliases (PLACEHOLDER: the install's string table holds the
    /// real ones, Open question 2).
    pub fn builtin() -> Self {
        Self::from_values(&[
            (Cmd::Clan, "/c/clan"),
            (Cmd::Normal, "/a/all"),
            (Cmd::Block, "/block"),
            (Cmd::UnBlock, "/unblock"),
            (Cmd::Whisper, "/w/whisper"),
            (Cmd::WhisperEnable, "/whisperenable"),
            (Cmd::WhisperIgnore, "/ignore"),
            (Cmd::WhisperUnignore, "/unignore"),
            (Cmd::WhisperAutoAnswer, "/autoanswer"),
        ])
    }

    /// Set a command's aliases from a string-table value.
    pub fn set(&mut self, cmd: Cmd, value: &str) {
        let Some(sep) = value.chars().next() else {
            self.aliases[cmd.index()].clear();
            return;
        };
        self.aliases[cmd.index()] = value[sep.len_utf8()..]
            .split(sep)
            .filter(|a| !a.is_empty())
            .map(|a| format!("{sep}{a}"))
            .collect();
    }

    /// The command a typed line starts with and what follows its alias
    /// (the leading space included). Commands are tried in order; an
    /// alias matches at the start, case-insensitively, with no word
    /// boundary; within a command the longest matching alias counts (the
    /// spec says the first, but its case 24 has `/w` before `/whisper`
    /// and still reads `/whisper Bob hi` as a whisper to Bob); a later
    /// command that matches replaces an earlier one.
    pub fn parse<'a>(&self, line: &'a str) -> Option<(Cmd, &'a str)> {
        let mut found = None;
        for cmd in ORDER {
            let best = self.aliases[cmd.index()]
                .iter()
                .filter_map(|alias| strip_prefix_ci(line, alias).map(|rest| (alias.len(), rest)))
                .max_by_key(|(len, _)| *len);
            if let Some((_, rest)) = best {
                found = Some((cmd, rest));
            }
        }
        found
    }
}

/// `line` without `prefix` at its start, compared case-insensitively.
fn strip_prefix_ci<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let mut l = line.char_indices();
    for p in prefix.chars() {
        let (_, c) = l.next()?;
        if !c.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
    }
    Some(l.next().map_or("", |(i, _)| &line[i..]))
}

/// The bad-word filter (F1): a convert list (each word becomes `**`), a
/// whitelist of safe words and the live list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BadWords {
    pub convert: Vec<String>,
    pub whitelist: Vec<String>,
    /// The lobby's live list, stored lower-cased; no whitelist.
    pub live: Vec<String>,
}

/// What a replaced word becomes, whatever its length.
pub const REPLACEMENT: &str = "**";

impl BadWords {
    /// `m` with the listed words replaced.
    pub fn filter(&self, m: &str) -> String {
        if self.convert.is_empty() && self.live.is_empty() {
            return m.to_string();
        }
        let mut m = m.to_string();
        let mut b = m.to_ascii_lowercase();
        let safe: Vec<&str> = self
            .whitelist
            .iter()
            .map(String::as_str)
            .filter(|w| !w.is_empty() && b.contains(*w))
            .collect();
        let mut sorted: Vec<&str> = self.convert.iter().map(String::as_str).collect();
        // Longest first; ties keep their list order.
        sorted.sort_by_key(|w| std::cmp::Reverse(w.len()));
        scan(&sorted, &safe, &mut m, &mut b);
        let live: Vec<&str> = self.live.iter().map(String::as_str).collect();
        scan(&live, &[], &mut m, &mut b);
        m
    }
}

/// One list's pass: replace the first occurrence of the first word that
/// is found and not protected, then restart at the *second* word
/// (Quirk 5).
fn scan(words: &[&str], safe: &[&str], m: &mut String, b: &mut String) {
    let mut k = 0;
    // A word of only `*` would be found in its own replacement for ever.
    let mut guard = 10_000;
    while k < words.len() && guard > 0 {
        guard -= 1;
        let w = words[k];
        if w.is_empty() || w.chars().all(|c| c == '*') {
            k += 1;
            continue;
        }
        match b.find(w) {
            Some(at) if !safe.iter().any(|s| s.contains(w)) => {
                b.replace_range(at..at + w.len(), REPLACEMENT);
                m.replace_range(at..at + w.len(), REPLACEMENT);
                k = 1;
            }
            _ => k += 1,
        }
    }
}

/// The block list (B1): wildcard patterns (`*` any run, `?` one
/// character), case-insensitive.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockList {
    pub entries: Vec<String>,
}

impl BlockList {
    /// Add `name`: refused for your own name and an exact duplicate.
    pub fn add(&mut self, name: &str, own: &str) -> bool {
        if name.is_empty() || name == own || self.entries.iter().any(|e| e == name) {
            return false;
        }
        self.entries.push(name.to_string());
        true
    }

    /// Remove `name` (exact); false when it was not there.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e != name);
        self.entries.len() != before
    }

    /// Whether lines from `name` are dropped: a name starting `GM`
    /// (any case) and your own name are never blocked.
    pub fn blocks(&self, name: &str, own: &str) -> bool {
        if name == own || name.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("gm")) {
            return false;
        }
        self.entries.iter().any(|e| wildcard(e, name))
    }
}

/// Case-insensitive `*` / `?` match of the whole `text`.
pub fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_split_on_the_separator() {
        let c = Commands::from_values(&[(Cmd::Whisper, "/x/xyz")]);
        assert_eq!(c.aliases[Cmd::Whisper.index()], ["/x", "/xyz"]);
    }

    #[test]
    fn wildcards() {
        assert!(wildcard("bo?", "BOX"));
        assert!(!wildcard("bo?", "Bobby"));
        assert!(wildcard("*", "anything"));
        assert!(wildcard("a*c", "abbbc"));
    }
}
