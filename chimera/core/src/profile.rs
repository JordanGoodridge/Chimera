//! A player profile file: sections in square brackets, one `Key = value`
//! per line, `//` comments (the format Combat Arms' options keep, made
//! generic so any game's settings can use it). Section and key names are
//! looked up ignoring case. Strings are quoted, floats may carry a
//! trailing `f`, booleans are `TRUE`/`FALSE` (nonzero integers read as
//! true).
//!
//! A `Profile` keeps the file's lines as they came: a value that is set
//! replaces its line in place (or is added at the end of its section, the
//! section at the end of the file), and every other line, comments and
//! entries nothing reads included, is written back untouched.
//!
//! `read`/`write` are the file helpers (a missing file reads as `None`;
//! writing makes the folder). Nothing here knows a game's keys.
//!
//! Known shortcuts: a comment is only recognised at the start of a line
//! or after the value outside quotes; a value with `//` in an unquoted
//! string (a URL) is cut there; one `Key = value` per line, no
//! multi-line values.

use std::path::Path;

/// A parsed profile (see the module doc).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    lines: Vec<String>,
}

/// One line classified.
enum Kind<'a> {
    Section(&'a str),
    Entry(&'a str, &'a str),
    Other,
}

fn classify(line: &str) -> Kind<'_> {
    let t = line.trim();
    if t.is_empty() || t.starts_with("//") || t.starts_with(';') {
        return Kind::Other;
    }
    if let Some(rest) = t.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        return Kind::Section(rest[..end].trim());
    }
    match t.split_once('=') {
        Some((k, v)) => Kind::Entry(k.trim(), v.trim()),
        None => Kind::Other,
    }
}

/// A value without its trailing `//` comment (outside quotes).
fn strip_comment(v: &str) -> &str {
    let mut quoted = false;
    let b = v.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'"' => quoted = !quoted,
            b'/' if !quoted && b.get(i + 1) == Some(&b'/') => return v[..i].trim_end(),
            _ => {}
        }
    }
    v
}

/// Quote a string for a profile.
pub fn quote(s: &str) -> String {
    format!("\"{s}\"")
}

/// A boolean as written: `TRUE` or `FALSE`.
pub fn boolean(b: bool) -> String {
    if b { "TRUE" } else { "FALSE" }.to_string()
}

impl Profile {
    /// Parse a file's text.
    pub fn parse(text: &str) -> Self {
        Self {
            lines: text.lines().map(|l| l.trim_end_matches('\r').to_string()).collect(),
        }
    }

    /// The file's text, lines as kept.
    pub fn to_text(&self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }

    /// Whether it holds no section or entry.
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| matches!(classify(l), Kind::Other))
    }

    /// The line range of a section: its header and the line after its
    /// body (the next header or the end); the first section of that name.
    fn section(&self, name: &str) -> Option<(usize, usize)> {
        let start = self
            .lines
            .iter()
            .position(|l| matches!(classify(l), Kind::Section(s) if s.eq_ignore_ascii_case(name)))?;
        let end = self.lines[start + 1..]
            .iter()
            .position(|l| matches!(classify(l), Kind::Section(_)))
            .map_or(self.lines.len(), |p| start + 1 + p);
        Some((start, end))
    }

    pub fn has_section(&self, name: &str) -> bool {
        self.section(name).is_some()
    }

    /// The names of the sections, in file order.
    pub fn sections(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|l| match classify(l) {
                Kind::Section(s) => Some(s.to_string()),
                _ => None,
            })
            .collect()
    }

    fn entry_line(&self, section: &str, key: &str) -> Option<usize> {
        let (start, end) = self.section(section)?;
        (start + 1..end).find(|&i| matches!(classify(&self.lines[i]), Kind::Entry(k, _) if k.eq_ignore_ascii_case(key)))
    }

    /// The raw value (comment cut, quotes kept) of `key` in `section`.
    pub fn raw(&self, section: &str, key: &str) -> Option<String> {
        let i = self.entry_line(section, key)?;
        match classify(&self.lines[i]) {
            Kind::Entry(_, v) => Some(strip_comment(v).to_string()),
            _ => None,
        }
    }

    /// The value as text: quotes taken off.
    pub fn string(&self, section: &str, key: &str) -> Option<String> {
        let v = self.raw(section, key)?;
        let v = v.trim();
        Some(v.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(v).to_string())
    }

    pub fn int(&self, section: &str, key: &str) -> Option<i32> {
        let v = self.raw(section, key)?;
        let v = v.trim();
        v.parse::<i32>()
            .ok()
            .or_else(|| v.trim_end_matches(['f', 'F']).parse::<f32>().ok().map(|f| f as i32))
            .or_else(|| match v.to_ascii_uppercase().as_str() {
                "TRUE" => Some(1),
                "FALSE" => Some(0),
                _ => None,
            })
    }

    pub fn float(&self, section: &str, key: &str) -> Option<f32> {
        let v = self.raw(section, key)?;
        v.trim().trim_end_matches(['f', 'F']).parse::<f32>().ok()
    }

    pub fn bool(&self, section: &str, key: &str) -> Option<bool> {
        let v = self.raw(section, key)?;
        match v.trim().to_ascii_uppercase().as_str() {
            "TRUE" => Some(true),
            "FALSE" => Some(false),
            o => o.trim_end_matches('F').parse::<f32>().ok().map(|n| n != 0.0),
        }
    }

    /// Set `key` in `section` to `value` (as it is written: quote a
    /// string yourself, `quote`).
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        if let Some(i) = self.entry_line(section, key) {
            let name = match classify(&self.lines[i]) {
                Kind::Entry(k, _) => k.to_string(),
                _ => key.to_string(),
            };
            self.lines[i] = format!("{name} = {value}");
            return;
        }
        let line = format!("{key} = {value}");
        match self.section(section) {
            Some((_, end)) => {
                // After the section's last line with something on it.
                let mut at = end;
                while at > 0 && self.lines[at - 1].trim().is_empty() {
                    at -= 1;
                }
                self.lines.insert(at, line);
            }
            None => {
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.push(format!("[{section}]"));
                self.lines.push(line);
            }
        }
    }

    /// Remove a key; whether it was there.
    pub fn remove(&mut self, section: &str, key: &str) -> bool {
        match self.entry_line(section, key) {
            Some(i) => {
                self.lines.remove(i);
                true
            }
            None => false,
        }
    }

    /// A section's entries, `(key, raw value)` in file order.
    pub fn entries(&self, section: &str) -> Vec<(String, String)> {
        let Some((start, end)) = self.section(section) else {
            return Vec::new();
        };
        (start + 1..end)
            .filter_map(|i| match classify(&self.lines[i]) {
                Kind::Entry(k, v) => Some((k.to_string(), strip_comment(v).to_string())),
                _ => None,
            })
            .collect()
    }

    /// Replace a section (its header and body) by `other`'s of the same
    /// name, or add it; no change when `other` has none.
    pub fn replace_section(&mut self, name: &str, other: &Profile) {
        let Some((ostart, oend)) = other.section(name) else { return };
        let new: Vec<String> = other.lines[ostart..oend].to_vec();
        match self.section(name) {
            Some((start, end)) => {
                self.lines.splice(start..end, new);
            }
            None => {
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.extend(new);
            }
        }
    }
}

/// Read a profile file; None when it can't be read.
pub fn read(path: &Path) -> Option<Profile> {
    std::fs::read_to_string(path).ok().map(|t| Profile::parse(&t))
}

/// Write a profile file (making its folder).
pub fn write(path: &Path, profile: &Profile) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, profile.to_text())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "// a comment\n[Display]\nScreenWidth = 1024 // wide\nScreenGamma = 1.22f\nVSync = FALSE\n\n[Bindings]\nForward = \"##17@ @ \"\n";

    #[test]
    fn reads_ignoring_case_and_comments() {
        let p = Profile::parse(TEXT);
        assert_eq!(p.int("display", "screenwidth"), Some(1024));
        assert_eq!(p.float("Display", "ScreenGamma"), Some(1.22));
        assert_eq!(p.bool("Display", "vsync"), Some(false));
        assert_eq!(p.string("Bindings", "FORWARD").as_deref(), Some("##17@ @ "));
        assert_eq!(p.int("Display", "Nope"), None);
        assert_eq!(p.sections(), ["Display", "Bindings"]);
    }

    #[test]
    fn set_keeps_other_lines_and_places_new_keys() {
        let mut p = Profile::parse(TEXT);
        p.set("Display", "ScreenWidth", "1280");
        p.set("Display", "ScreenHeight", "720");
        p.set("Sound", "soundvolume", "70");
        let t = p.to_text();
        assert!(t.contains("// a comment\n"));
        assert!(t.contains("ScreenWidth = 1280\n"));
        assert!(t.contains("ScreenHeight = 720\n\n[Bindings]"), "{t}");
        assert!(t.ends_with("[Sound]\nsoundvolume = 70\n"), "{t}");
        assert_eq!(Profile::parse(&t).int("Display", "ScreenHeight"), Some(720));
    }

    #[test]
    fn replacing_a_section() {
        let mut p = Profile::parse(TEXT);
        let o = Profile::parse("[Bindings]\nForward = \" @ @ \"\nJump = \"##57@ @ \"\n");
        p.replace_section("bindings", &o);
        assert_eq!(p.string("Bindings", "Jump").as_deref(), Some("##57@ @ "));
        assert_eq!(p.int("Display", "ScreenWidth"), Some(1024));
    }
}
