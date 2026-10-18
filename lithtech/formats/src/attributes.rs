//! LithTech attribute files ("butes"): the INI-like text documents that
//! hold a LithTech game's numbers (Combat Arms' weapons, ammo, characters
//! and players), per specs/combat_arms/attribute_data.md ("Encoding",
//! "Document syntax", "Value types", "Tables, records and lookups").
//!
//! `Document::parse` turns the decrypted bytes into sections of typed
//! values; a lexical or grammar error rejects the whole document
//! (`ParseError`, with its line). Lookups are case-insensitive and typed
//! reads are strict, with a caller's fallback (`Section::int`,
//! `Section::number`, ...): the stored type must match, except that a
//! number read accepts integers, floats and doubles. Records (`Prefix0`,
//! `Prefix1`, ...) and lists inside a section (`Key0`, `Key1`, ...) end at
//! the first missing index (`Document::records`, `Section::string_list`).
//!
//! What a game makes of the records (which sections, which fields, their
//! built-in defaults) is the game's, not this module's.
//!
//! Known shortcuts:
//! - The event "text-change overwrite" layer ("Defaults and overrides",
//!   item 3) isn't implemented: it is optional and no shipped file has it.
//! - Read literally where the spec is silent: after `[` and before `]`
//!   only CR and LF are skipped (a tab or space there is an error, as a
//!   space after `[` is); a cast's `)` is followed directly by its number
//!   (`(DWORD) 7` is an error); integers beyond 32 bits clamp, and so do
//!   `(DWORD)` values beyond 32 bits.

use std::collections::HashMap;
use std::fmt;

/// Strings longer than this don't fit a fixed-size string field
/// ("Fixed-size string reads": a 128-byte buffer with its terminator).
pub const MAX_FIXED_STRING: usize = 127;

/// A stored value; its type is decided by its spelling ("Value types").
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i32),
    Double(f64),
    Float(f32),
    Dword(u32),
    Byte(u8),
    Bool(bool),
    /// Raw bytes (the shipped files' non-ASCII text is CP949).
    Str(Vec<u8>),
    Vector([f64; 3]),
    Point([i32; 2]),
    Rect([i32; 4]),
    Range([f64; 2]),
}

/// One `[Section]`: its keys (first one wins), compared case-insensitively.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    name: String,
    values: HashMap<String, Value>,
}

/// A parsed attribute document: its sections, in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    sections: Vec<Section>,
    index: HashMap<String, usize>,
}

/// Why a document was rejected: the error and the 1-based line it is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

fn fold(name: &str) -> String {
    name.to_ascii_lowercase()
}

impl Document {
    /// Parse a document from its decrypted bytes: up to the first 0x00
    /// (the decryptor's padding), after a UTF-8 byte-order mark.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let mut text = &bytes[..end];
        if text.len() >= 3 && text[..3] == [0xEF, 0xBB, 0xBF] {
            text = &text[3..];
        }
        Parser { b: text, at: 0 }.document()
    }

    /// The section named `name`, in any letter case.
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.index.get(&fold(name)).map(|&i| &self.sections[i])
    }

    /// Every section, in file order.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// A record kind: `prefix0`, `prefix1`, ... up to the first index
    /// with no section ("Record enumeration"). The position is the
    /// record's runtime ID.
    pub fn records(&self, prefix: &str) -> Vec<&Section> {
        (0..).map_while(|i| self.section(&format!("{prefix}{i}"))).collect()
    }
}

impl Section {
    /// The name as written in the file.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The stored value of `key`, in any letter case.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(&fold(key))
    }

    /// Every key (folded to lower case) and its value, in no order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// An integer read: integers only, else `fallback`.
    pub fn int(&self, key: &str, fallback: i32) -> i32 {
        match self.get(key) {
            Some(Value::Int(v)) => *v,
            _ => fallback,
        }
    }

    /// A number read: integers, floats and doubles, else `fallback`.
    pub fn number(&self, key: &str, fallback: f64) -> f64 {
        match self.get(key) {
            Some(Value::Int(v)) => *v as f64,
            Some(Value::Float(v)) => *v as f64,
            Some(Value::Double(v)) => *v,
            _ => fallback,
        }
    }

    /// The number when `key` holds an accepted type, else `None`.
    pub fn number_opt(&self, key: &str) -> Option<f64> {
        match self.get(key) {
            Some(Value::Int(_) | Value::Float(_) | Value::Double(_)) => Some(self.number(key, 0.0)),
            _ => None,
        }
    }

    pub fn dword(&self, key: &str, fallback: u32) -> u32 {
        match self.get(key) {
            Some(Value::Dword(v)) => *v,
            _ => fallback,
        }
    }

    pub fn byte(&self, key: &str, fallback: u8) -> u8 {
        match self.get(key) {
            Some(Value::Byte(v)) => *v,
            _ => fallback,
        }
    }

    pub fn bool(&self, key: &str, fallback: bool) -> bool {
        match self.get(key) {
            Some(Value::Bool(v)) => *v,
            _ => fallback,
        }
    }

    /// A string read: the raw bytes, or `None` when the key is missing or
    /// not a string.
    pub fn string(&self, key: &str) -> Option<&[u8]> {
        match self.get(key) {
            Some(Value::Str(v)) => Some(v),
            _ => None,
        }
    }

    /// A fixed-size string field: the string when it has at most
    /// `MAX_FIXED_STRING` bytes, else `fallback` (not a truncation).
    /// Bytes outside ASCII are kept as Latin-1 characters (display text
    /// needs CP949; names and paths are ASCII).
    pub fn fixed_string(&self, key: &str, fallback: &str) -> String {
        match self.string(key) {
            Some(s) if s.len() <= MAX_FIXED_STRING => s.iter().map(|&b| b as char).collect(),
            _ => fallback.to_string(),
        }
    }

    pub fn vector(&self, key: &str) -> Option<[f64; 3]> {
        match self.get(key) {
            Some(Value::Vector(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn point(&self, key: &str) -> Option<[i32; 2]> {
        match self.get(key) {
            Some(Value::Point(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn rect(&self, key: &str) -> Option<[i32; 4]> {
        match self.get(key) {
            Some(Value::Rect(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn range(&self, key: &str) -> Option<[f64; 2]> {
        match self.get(key) {
            Some(Value::Range(v)) => Some(*v),
            _ => None,
        }
    }

    /// How many of `name0`, `name1`, ... exist before the first gap.
    fn list_len(&self, name: &str) -> usize {
        (0..).take_while(|i| self.get(&format!("{name}{i}")).is_some()).count()
    }

    /// A string list ("Lists inside a record"): an element that isn't a
    /// string reads as the empty string.
    pub fn string_list(&self, name: &str) -> Vec<Vec<u8>> {
        (0..self.list_len(name))
            .map(|i| self.string(&format!("{name}{i}")).unwrap_or_default().to_vec())
            .collect()
    }

    /// An integer list: non-integers read as 0.
    pub fn int_list(&self, name: &str) -> Vec<i32> {
        (0..self.list_len(name)).map(|i| self.int(&format!("{name}{i}"), 0)).collect()
    }

    /// A number list: non-numbers read as 0.0.
    pub fn number_list(&self, name: &str) -> Vec<f64> {
        (0..self.list_len(name)).map(|i| self.number(&format!("{name}{i}"), 0.0)).collect()
    }
}

/// C `atoi` on a string value: leading blanks, a sign, digits; anything
/// else ends it (an empty or non-numeric string is 0). Clamps to 32 bits.
pub fn c_atoi(text: &[u8]) -> i32 {
    let s = trim_start(text);
    let (neg, s) = sign(s);
    let digits = s.iter().take_while(|b| b.is_ascii_digit()).count();
    clamp_int(&s[..digits], neg)
}

/// C `atof` on a string value: the longest leading decimal number (0.0
/// when there is none).
pub fn c_atof(text: &[u8]) -> f64 {
    let s = trim_start(text);
    let digits_from = |i: usize| s[i.min(s.len())..].iter().take_while(|b| b.is_ascii_digit()).count();
    let mut end = usize::from(matches!(s.first(), Some(b'+' | b'-')));
    let int = digits_from(end);
    end += int;
    let mut frac = 0;
    if s.get(end) == Some(&b'.') {
        frac = digits_from(end + 1);
        end += 1 + frac;
    }
    if int + frac == 0 {
        return 0.0;
    }
    if matches!(s.get(end), Some(b'e' | b'E')) {
        let signed = usize::from(matches!(s.get(end + 1), Some(b'+' | b'-')));
        let exp = digits_from(end + 1 + signed);
        if exp > 0 {
            end += 1 + signed + exp;
        }
    }
    std::str::from_utf8(&s[..end])
        .ok()
        .and_then(|t| t.trim_end_matches('.').parse().ok())
        .unwrap_or(0.0)
}

fn trim_start(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|b| b" \t\r\n\x0b\x0c".contains(b)).count();
    &s[n..]
}

fn sign(s: &[u8]) -> (bool, &[u8]) {
    match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    }
}

/// Decimal digits as a clamped 32-bit integer.
fn clamp_int(digits: &[u8], neg: bool) -> i32 {
    let mut v: i64 = 0;
    for &d in digits {
        v = (v * 10 + (d - b'0') as i64).min(1 << 40);
    }
    let v = if neg { -v } else { v };
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

struct Parser<'a> {
    b: &'a [u8],
    at: usize,
}

type Res<T> = Result<T, ParseError>;

impl Parser<'_> {
    fn err<T>(&self, message: impl Into<String>) -> Res<T> {
        let line = 1 + self.b[..self.at.min(self.b.len())].iter().filter(|&&c| c == b'\n').count();
        Err(ParseError {
            line,
            message: message.into(),
        })
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    fn peek_at(&self, n: usize) -> Option<u8> {
        self.b.get(self.at + n).copied()
    }

    /// Whitespace and comments ("Comments", "Whitespace").
    fn skip(&mut self) -> Res<()> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.at += 1,
                Some(b'/') => match self.peek_at(1) {
                    Some(b'/') => {
                        while self.peek().is_some_and(|c| c != b'\n') {
                            self.at += 1;
                        }
                    }
                    Some(b'*') => {
                        let start = self.at;
                        self.at += 2;
                        loop {
                            match self.peek() {
                                None => {
                                    self.at = start;
                                    return self.err("unterminated /* comment");
                                }
                                Some(b'*') if self.peek_at(1) == Some(b'/') => {
                                    self.at += 2;
                                    break;
                                }
                                _ => self.at += 1,
                            }
                        }
                    }
                    _ => return self.err("a lone '/'"),
                },
                _ => return Ok(()),
            }
        }
    }

    fn document(mut self) -> Res<Document> {
        let mut doc = Document::default();
        loop {
            self.skip()?;
            match self.peek() {
                None => return Ok(doc),
                Some(b'[') => {
                    let name = self.header()?;
                    let key = fold(&name);
                    if doc.index.contains_key(&key) {
                        return self.err(format!("section [{name}] given twice"));
                    }
                    doc.index.insert(key, doc.sections.len());
                    doc.sections.push(Section {
                        name,
                        values: HashMap::new(),
                    });
                }
                Some(c) if is_ident(c) => {
                    let key = self.ident()?;
                    let Some(section) = doc.sections.last_mut() else {
                        return self.err(format!("'{key}' before the first section"));
                    };
                    self.skip()?;
                    if self.peek() != Some(b'=') {
                        return self.err(format!("expected '=' after '{key}'"));
                    }
                    self.at += 1;
                    self.skip()?;
                    let value = self.value()?;
                    // First one wins ("Duplicates").
                    section.values.entry(fold(&key)).or_insert(value);
                }
                Some(c) => return self.err(format!("unexpected byte 0x{c:02X}")),
            }
        }
    }

    /// `[Name]`: an identifier starting with a letter or `_` right after
    /// the bracket (newlines aside).
    fn header(&mut self) -> Res<String> {
        self.at += 1;
        self.skip_newlines();
        if !self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
            return self.err("expected a section name after '['");
        }
        let name = self.ident()?;
        self.skip_newlines();
        if self.peek() != Some(b']') {
            return self.err(format!("expected ']' after [{name}"));
        }
        self.at += 1;
        Ok(name)
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Some(b'\r' | b'\n')) {
            self.at += 1;
        }
    }

    /// An identifier: `A-Z a-z 0-9 _`, not a pure number.
    fn ident(&mut self) -> Res<String> {
        let start = self.at;
        while self.peek().is_some_and(is_ident) {
            self.at += 1;
        }
        let word = &self.b[start..self.at];
        if word.iter().all(u8::is_ascii_digit) {
            self.at = start;
            return self.err("a number where a name belongs");
        }
        Ok(String::from_utf8_lossy(word).into_owned())
    }

    fn value(&mut self) -> Res<Value> {
        match self.peek() {
            Some(b'"') => self.string(),
            Some(b'<') => {
                self.at += 1;
                let v = self.components(b'>')?;
                match v[..] {
                    [x, y, z] => Ok(Value::Vector([x, y, z])),
                    _ => self.err("a vector has three components"),
                }
            }
            Some(b'[') => {
                self.at += 1;
                let v = self.components(b']')?;
                match v[..] {
                    [a, b] => Ok(Value::Range([a, b])),
                    _ => self.err("a range has two components"),
                }
            }
            Some(b'(') => self.paren(),
            Some(b'+' | b'-') | Some(b'0'..=b'9') => self.number(),
            Some(c) if is_ident(c) => {
                let start = self.at;
                while self.peek().is_some_and(is_ident) {
                    self.at += 1;
                }
                let word = &self.b[start..self.at];
                if word.eq_ignore_ascii_case(b"true") {
                    Ok(Value::Bool(true))
                } else if word.eq_ignore_ascii_case(b"false") {
                    Ok(Value::Bool(false))
                } else {
                    self.at = start;
                    self.err("a name where a value belongs")
                }
            }
            None => self.err("a value is missing at the end"),
            Some(c) => self.err(format!("unexpected byte 0x{c:02X} in a value")),
        }
    }

    fn string(&mut self) -> Res<Value> {
        self.at += 1;
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return self.err("unterminated string"),
                Some(b'\r' | b'\n') => return self.err("a line break inside a string"),
                Some(b'"') if self.peek_at(1) == Some(b'"') => {
                    out.push(b'"');
                    self.at += 2;
                }
                Some(b'"') => {
                    self.at += 1;
                    return Ok(Value::Str(out));
                }
                Some(c) => {
                    out.push(c);
                    self.at += 1;
                }
            }
        }
    }

    fn digits(&mut self) -> &[u8] {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        &self.b[start..self.at]
    }

    /// The double spelling after its sign: digits `.` digits, an optional
    /// exponent. `None` (position unchanged) when there's no `.digit`.
    fn fraction_and_exponent(&mut self) -> Res<bool> {
        if self.peek() != Some(b'.') {
            return Ok(false);
        }
        if !self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            return self.err("a decimal point needs digits after it");
        }
        self.at += 1;
        self.digits();
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits().is_empty() {
                return self.err("an exponent needs digits");
            }
        }
        Ok(true)
    }

    /// Nothing that would continue a word may touch a number.
    fn end_of_number(&self) -> Res<()> {
        match self.peek() {
            Some(c) if is_ident(c) || c == b'.' => self.err("a number runs into other text"),
            _ => Ok(()),
        }
    }

    /// An integer, double or float (`1.5f`).
    fn number(&mut self) -> Res<Value> {
        let start = self.at;
        let (neg, _) = sign(&self.b[self.at..]);
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.at += 1;
        }
        let dstart = self.at;
        if self.digits().is_empty() {
            return self.err("a sign needs digits right after it");
        }
        // `12ab`, `1e5`: identifiers, not values.
        if self.peek().is_some_and(|c| is_ident(c)) {
            return self.err("a name where a value belongs");
        }
        let int_end = self.at;
        if !self.fraction_and_exponent()? {
            let v = clamp_int(&self.b[dstart..int_end], neg);
            return Ok(Value::Int(v));
        }
        let text = std::str::from_utf8(&self.b[start..self.at]).unwrap();
        let v: f64 = text.parse().unwrap();
        if matches!(self.peek(), Some(b'f' | b'F')) {
            self.at += 1;
            self.end_of_number()?;
            return Ok(Value::Float(v as f32));
        }
        self.end_of_number()?;
        Ok(Value::Double(v))
    }

    fn spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.at += 1;
        }
    }

    /// `(DWORD)n`, `(BYTE)n`, `(FLOAT)x`, a point `(a, b)` or a rect
    /// `(a, b, c, d)`.
    fn paren(&mut self) -> Res<Value> {
        self.at += 1;
        self.spaces();
        if self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            let start = self.at;
            while self.peek().is_some_and(is_ident) {
                self.at += 1;
            }
            let word = self.b[start..self.at].to_ascii_uppercase();
            self.spaces();
            if self.peek() != Some(b')') {
                return self.err("expected ')' after a cast");
            }
            self.at += 1;
            return match &word[..] {
                b"DWORD" | b"BYTE" => {
                    let d = self.digits().to_vec();
                    if d.is_empty() {
                        return self.err("a cast needs an unsigned integer");
                    }
                    self.end_of_number()?;
                    let mut v: u64 = 0;
                    for c in d {
                        v = (v * 10 + (c - b'0') as u64).min(1 << 40);
                    }
                    Ok(if word == b"BYTE" {
                        Value::Byte((v % 256) as u8)
                    } else {
                        Value::Dword(v.min(u32::MAX as u64) as u32)
                    })
                }
                b"FLOAT" => {
                    let start = self.at;
                    if matches!(self.peek(), Some(b'+' | b'-')) {
                        self.at += 1;
                    }
                    if self.digits().is_empty() || !self.fraction_and_exponent()? {
                        return self.err("(FLOAT) needs a decimal number");
                    }
                    let v: f64 = std::str::from_utf8(&self.b[start..self.at]).unwrap().parse().unwrap();
                    if matches!(self.peek(), Some(b'f' | b'F')) {
                        self.at += 1;
                    }
                    self.end_of_number()?;
                    Ok(Value::Float(v as f32))
                }
                _ => self.err("unknown cast"),
            };
        }
        let mut v = Vec::new();
        loop {
            self.spaces();
            let (neg, _) = sign(&self.b[self.at..]);
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            let dstart = self.at;
            if self.digits().is_empty() {
                return self.err("expected an integer");
            }
            v.push(clamp_int(&self.b[dstart..self.at], neg));
            self.spaces();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b')') => {
                    self.at += 1;
                    break;
                }
                _ => return self.err("expected ',' or ')'"),
            }
        }
        match v[..] {
            [a, b] => Ok(Value::Point([a, b])),
            [a, b, c, d] => Ok(Value::Rect([a, b, c, d])),
            _ => self.err("a point has two integers, a rect four"),
        }
    }

    /// Comma-separated numbers up to `close` (vectors and ranges): sign,
    /// digits, optional `.digits`, optional exponent; spaces only.
    fn components(&mut self, close: u8) -> Res<Vec<f64>> {
        let mut v = Vec::new();
        loop {
            self.spaces();
            let start = self.at;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits().is_empty() {
                return self.err("expected a number");
            }
            if self.peek() == Some(b'.') {
                self.at += 1;
                if self.digits().is_empty() {
                    return self.err("a decimal point needs digits after it");
                }
            }
            if matches!(self.peek(), Some(b'e' | b'E')) {
                self.at += 1;
                if matches!(self.peek(), Some(b'+' | b'-')) {
                    self.at += 1;
                }
                if self.digits().is_empty() {
                    return self.err("an exponent needs digits");
                }
            }
            v.push(std::str::from_utf8(&self.b[start..self.at]).unwrap().parse().unwrap());
            self.spaces();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(c) if c == close => {
                    self.at += 1;
                    return Ok(v);
                }
                _ => return self.err("expected ',' or the closing bracket"),
            }
        }
    }
}
