//! Localization files (`resource/<game>_english.txt` and the like): UTF-16
//! or UTF-8 text of `"key" "string"` pairs, with escapes of their own
//! (`\n`, `\t`, `\"`, `\\`; the other KeyValues files are `keyvalues`).

use std::collections::HashMap;

/// Text from a localization file: UTF-16 (with a byte-order mark) or UTF-8.
pub fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xFF, 0xFE, rest @ ..] => {
            let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&units)
        }
        [0xFE, 0xFF, rest @ ..] => {
            let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&units)
        }
        _ => String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned(),
    }
}

/// A localization file's strings (lower-case keys; the first of a repeated
/// key wins, so platform variants after it are ignored). Quoted strings
/// may span lines and escape `\"`, `\n` and `\\`; `//` starts a comment
/// outside quotes; `[$X360]` conditions are dropped.
pub fn localization(text: &str) -> HashMap<String, String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => s.push(o),
                            None => {}
                        },
                        c => s.push(c),
                    }
                }
                tokens.push(Some(s));
            }
            '{' | '}' => tokens.push(None),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            c => {
                let mut s = c.to_string();
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || n == '"' || n == '{' || n == '}' {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                if !(s.starts_with("[$") || s.starts_with("[!$")) {
                    tokens.push(Some(s));
                }
            }
        }
    }
    // Key-value pairs: a string followed by a string (braces break pairs).
    let mut out = HashMap::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        if let (Some(k), Some(v)) = (&tokens[i], &tokens[i + 1]) {
            out.entry(k.to_lowercase()).or_insert_with(|| v.clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\"lang\"\n{\n\"Language\" \"English\"\n\"Tokens\"\n{\n\
        \"Cstrike_TitlesTXT_Cover_me\"\t\t\"Cover Me!\"\n\
        // a comment \"Cstrike_TitlesTXT_Cover_me\" \"no\"\n\
        \"RadioC\"\n\"Radio Responses/Reports\n\n1. \\\"Affirmative/Roger\\\"\n2. \\\"Enemy Spotted\\\"\n\n0. Exit\n\"\n\
        \"Tab\" \"a\\tb\\nc\"\n\
        \"Other\" \"x\" [$X360]\n\"Other\" \"y\"\n}\n}\n";

    #[test]
    fn localization_strings_with_escapes_and_conditions() {
        let s = localization(SAMPLE);
        assert_eq!(s["cstrike_titlestxt_cover_me"], "Cover Me!");
        assert_eq!(s["other"], "x");
        assert_eq!(s["tab"], "a\tb\nc");
        assert!(s["radioc"].contains("1. \"Affirmative/Roger\""));
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("\"a\" \"b\"".encode_utf16().flat_map(|u| u.to_le_bytes()))
            .collect();
        assert_eq!(localization(&decode(&utf16))["a"], "b");
    }
}
