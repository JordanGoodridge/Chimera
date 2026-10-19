//! The client effect file (`CLIENTFX.FXF`, in the `CLIENTFX` archive) in
//! its text form, per specs/combat_arms/effects_sounds.md, "How effects
//! are described as data", "The effect file": `Groups: G`, then G groups,
//! each `GroupName`, `FxInGroup` (K), `Phase` (the group's length in ms)
//! and K keys; a key is `FxName: Type;Label`, its ids and link, `StartTime`
//! and `EndTime` (seconds, clamped to the group's length), `Repeat`, three
//! values the game ignores, `Properties: N` and N typed properties. One
//! `Tag: value` per line; indentation and line endings are ignored.
//!
//! Group names are looked up without regard to case (`FxFile::group`); a
//! missing name is `None` (the game plays nothing). Colour keys (`Ck`)
//! keep the file's numbers, alpha **inverted** (0 opaque); `FxKey::
//! color_keys` turns it into opacity. Scale keys are `Sk`.
//!
//! Floats may be written as the C runtime prints them (`-nan(ind)` in
//! the stock file's scale keys): read as not-a-number or infinity.
//!
//! Known shortcuts: the binary variant (a file that doesn't start with
//! `Groups:`) is refused; unknown property types keep their text as a
//! string.

use std::fmt;

/// A typed property value ("A property: `PropName`, `Type`, one value line").
#[derive(Clone, Debug, PartialEq)]
pub enum FxValue {
    /// Type 0.
    String(String),
    /// Type 1.
    Int(i64),
    /// Type 2.
    Float(f32),
    /// Type 3: the index is the value; the choices name them.
    Combo { index: i32, choices: Vec<String> },
    /// Type 4.
    Vector([f32; 3]),
    /// Type 5 (scale keys: time, value, 0, 0).
    Vector4([f32; 4]),
    /// Type 6: time, r, g, b, a (alpha inverted: 0 opaque).
    ColorKey([f32; 5]),
    /// Type 7: `ext|in-archive path` (`...` or empty: none).
    Path { ext: String, path: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxProp {
    pub name: String,
    pub value: FxValue,
}

/// One effect inside a group.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxKey {
    /// The effect type (`ParticleSystem`, `Sprite`, `DynaLight`, ...).
    pub kind: String,
    pub label: String,
    pub id: i64,
    pub linked: bool,
    pub link_id: i64,
    pub link_node: String,
    /// Seconds into the group, clamped to 0..its length.
    pub start: f32,
    pub end: f32,
    pub repeat: i64,
    pub props: Vec<FxProp>,
}

/// A named effect group.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxGroup {
    pub name: String,
    /// The group's length, ms (`Phase`).
    pub phase_ms: u32,
    pub keys: Vec<FxKey>,
}

impl FxGroup {
    /// The group's length in seconds.
    pub fn length(&self) -> f32 {
        self.phase_ms as f32 / 1000.0
    }
}

/// The whole file.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxFile {
    pub groups: Vec<FxGroup>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FxfError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for FxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for FxfError {}

/// The lines as (line number, tag, value), blank lines skipped.
struct Lines<'a> {
    lines: Vec<(usize, &'a str, &'a str)>,
    at: usize,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let lines = text
            .lines()
            .enumerate()
            .filter_map(|(i, l)| {
                let l = l.trim();
                if l.is_empty() {
                    return None;
                }
                let (tag, value) = l.split_once(':').unwrap_or((l, ""));
                Some((i + 1, tag.trim(), value.trim()))
            })
            .collect();
        Self { lines, at: 0 }
    }

    fn line(&self) -> usize {
        self.lines.get(self.at).or(self.lines.last()).map_or(0, |l| l.0)
    }

    fn err(&self, message: impl Into<String>) -> FxfError {
        FxfError {
            line: self.line(),
            message: message.into(),
        }
    }

    /// The next line, which must have tag `tag`: its value.
    fn expect(&mut self, tag: &str) -> Result<&'a str, FxfError> {
        match self.lines.get(self.at) {
            Some((_, t, v)) if t.eq_ignore_ascii_case(tag) => {
                self.at += 1;
                Ok(v)
            }
            Some((_, t, _)) => Err(self.err(format!("expected `{tag}:`, found `{t}:`"))),
            None => Err(self.err(format!("expected `{tag}:`, found the end"))),
        }
    }

    fn number<T: std::str::FromStr>(&mut self, tag: &str) -> Result<T, FxfError> {
        let v = self.expect(tag)?;
        v.parse().map_err(|_| self.err(format!("`{tag}`: not a number: `{v}`")))
    }

    /// The next line's (tag, value) whatever its tag.
    fn any(&mut self) -> Result<(&'a str, &'a str), FxfError> {
        let l = self.lines.get(self.at).ok_or_else(|| self.err("unexpected end"))?;
        self.at += 1;
        Ok((l.1, l.2))
    }
}

/// A float as the file writes it, C-runtime spellings of not-a-number and
/// infinity (`-nan(ind)`, `1.#INF`) included.
fn float(t: &str) -> Option<f32> {
    if let Ok(v) = t.parse() {
        return Some(v);
    }
    let l = t.to_ascii_lowercase();
    if l.contains("nan") || l.contains("#ind") {
        Some(f32::NAN)
    } else if l.contains("inf") {
        Some(if l.starts_with('-') { f32::NEG_INFINITY } else { f32::INFINITY })
    } else {
        None
    }
}

fn floats<const N: usize>(v: &str) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    let mut it = v.split_whitespace();
    for o in &mut out {
        *o = float(it.next()?)?;
    }
    Some(out)
}

impl FxFile {
    /// Parse the text form.
    pub fn parse(text: &str) -> Result<Self, FxfError> {
        let text = text.trim_start_matches('\u{feff}');
        if !text.trim_start().starts_with("Groups:") {
            return Err(FxfError {
                line: 1,
                message: "not the text form (no `Groups:` first)".into(),
            });
        }
        let mut l = Lines::new(text);
        let count: usize = l.number("Groups")?;
        let mut groups = Vec::with_capacity(count);
        for _ in 0..count {
            let name = l.expect("GroupName")?.to_string();
            let keys: usize = l.number("FxInGroup")?;
            let phase_ms: u32 = l.number("Phase")?;
            let length = phase_ms as f32 / 1000.0;
            let mut group = FxGroup {
                name,
                phase_ms,
                keys: Vec::with_capacity(keys),
            };
            for _ in 0..keys {
                let fx = l.expect("FxName")?;
                let (kind, label) = fx.split_once(';').unwrap_or((fx, ""));
                let mut key = FxKey {
                    kind: kind.trim().to_string(),
                    label: label.trim().to_string(),
                    id: l.number("FxID")?,
                    linked: l.number::<i64>("Linked")? != 0,
                    link_id: l.number("LinkID")?,
                    link_node: l.expect("LinkNode")?.to_string(),
                    start: l.number::<f32>("StartTime")?.clamp(0.0, length),
                    end: l.number::<f32>("EndTime")?.clamp(0.0, length),
                    repeat: l.number("Repeat")?,
                    props: Vec::new(),
                };
                l.expect("TrackID")?;
                l.expect("MinScale")?;
                l.expect("MaxScale")?;
                let n: usize = l.number("Properties")?;
                key.props.reserve(n);
                for _ in 0..n {
                    let name = l.expect("PropName")?.to_string();
                    let ty: i32 = l.number("Type")?;
                    let (_, v) = l.any()?;
                    let bad = |l: &Lines| l.err(format!("{name}: bad value `{v}`"));
                    let value = match ty {
                        0 => FxValue::String(v.to_string()),
                        1 => FxValue::Int(v.parse().map_err(|_| bad(&l))?),
                        2 => FxValue::Float(float(v).ok_or_else(|| bad(&l))?),
                        3 => {
                            let mut parts = v.split(',');
                            let index = parts.next().and_then(|i| i.trim().parse().ok()).ok_or_else(|| bad(&l))?;
                            FxValue::Combo {
                                index,
                                choices: parts.map(|c| c.trim().to_string()).collect(),
                            }
                        }
                        4 => FxValue::Vector(floats(v).ok_or_else(|| bad(&l))?),
                        5 => FxValue::Vector4(floats(v).ok_or_else(|| bad(&l))?),
                        6 => FxValue::ColorKey(floats(v).ok_or_else(|| bad(&l))?),
                        7 => {
                            let (ext, path) = v.split_once('|').unwrap_or(("", v));
                            FxValue::Path {
                                ext: ext.trim().to_string(),
                                path: path.trim().to_string(),
                            }
                        }
                        _ => FxValue::String(v.to_string()),
                    };
                    key.props.push(FxProp { name, value });
                }
                group.keys.push(key);
            }
            groups.push(group);
        }
        Ok(Self { groups })
    }

    /// The group named `name`, without regard to case.
    pub fn group(&self, name: &str) -> Option<&FxGroup> {
        self.groups.iter().find(|g| g.name.eq_ignore_ascii_case(name))
    }
}

impl FxKey {
    /// The first property named `name` (without regard to case).
    pub fn get(&self, name: &str) -> Option<&FxValue> {
        self.props
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| &p.value)
    }

    pub fn float(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            FxValue::Float(f) => Some(*f),
            FxValue::Int(i) => Some(*i as f32),
            _ => None,
        }
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        match self.get(name)? {
            FxValue::Int(i) => Some(*i),
            FxValue::Combo { index, .. } => Some(*index as i64),
            _ => None,
        }
    }

    /// A combo's index (also an integer's value).
    pub fn combo(&self, name: &str) -> Option<i32> {
        self.int(name).map(|i| i as i32)
    }

    pub fn vector(&self, name: &str) -> Option<[f32; 3]> {
        match self.get(name)? {
            FxValue::Vector(v) => Some(*v),
            _ => None,
        }
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            FxValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// A path property's in-archive path (None when empty or `...`).
    pub fn path(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            FxValue::Path { path, .. } if !path.is_empty() && path != "..." => Some(path),
            _ => None,
        }
    }

    /// The colour keys (`Ck`) as (time, [r, g, b, opacity]): the file's
    /// alpha inverted (1 - a); a value that isn't a number is 0.
    pub fn color_keys(&self) -> Vec<(f32, [f32; 4])> {
        self.props
            .iter()
            .filter(|p| p.name == "Ck")
            .filter_map(|p| match p.value {
                FxValue::ColorKey(k) => {
                    let [t, r, g, b, a] = k.map(finite);
                    Some((t, [r, g, b, 1.0 - a]))
                }
                _ => None,
            })
            .collect()
    }

    /// The scale keys (`Sk`) as (time, value); a value that isn't a
    /// number is 0.
    pub fn scale_keys(&self) -> Vec<(f32, f32)> {
        self.props
            .iter()
            .filter(|p| p.name == "Sk")
            .filter_map(|p| match p.value {
                FxValue::Vector4([t, v, ..]) => Some((finite(t), finite(v))),
                _ => None,
            })
            .collect()
    }

    /// Whether the key is created at effect detail `level` (0 low, 1
    /// medium, 2 high), by its `DetailLevel` (0 all, 1 high, 2 medium,
    /// 3 low, 4 medium+high, 5 low+medium, 6 low+high).
    pub fn at_detail(&self, level: u8) -> bool {
        let (low, medium, high) = (level == 0, level == 1, level == 2);
        match self.combo("DetailLevel").unwrap_or(0) {
            0 => true,
            1 => high,
            2 => medium,
            3 => low,
            4 => medium || high,
            5 => low || medium,
            6 => low || high,
            _ => true,
        }
    }
}

/// A key's number, not-a-number and infinity read as 0.
fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

/// The value of piecewise-linear keys (time, value) at `t` (held beyond
/// the ends; empty: `default`).
pub fn sample_keys(keys: &[(f32, f32)], t: f32, default: f32) -> f32 {
    let Some(first) = keys.first() else { return default };
    if t <= first.0 {
        return first.1;
    }
    for w in keys.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if t <= t1 {
            return if t1 > t0 { v0 + (v1 - v0) * (t - t0) / (t1 - t0) } else { v1 };
        }
    }
    keys.last().map_or(default, |k| k.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-group file in the text form's layout (ours, not the game's).
    pub const SAMPLE: &str = "Groups: 2\r\n\tGroupName: Flash_A\r\n\tFxInGroup: 2\r\n\tPhase: 50\r\n\
\t\tFxName: ParticleSystem;\r\n\t\tFxID: 2\r\n\t\tLinked: 0\r\n\t\tLinkID: 0\r\n\t\tLinkNode:\r\n\
\t\tStartTime: 0.000000\r\n\t\tEndTime: 0.080000\r\n\t\tRepeat: 1\r\n\t\tTrackID: 0\r\n\t\tMinScale: 0\r\n\
\t\tMaxScale: 1\r\n\t\tProperties: 7\r\n\
\t\t\tPropName: UpdatePos\r\n\t\t\tType: 3\r\n\t\t\tCOMBO: 6,Fixed,Follow\r\n\
\t\t\tPropName: Texture\r\n\t\t\tType: 7\r\n\t\t\tPATH: dtx|FX\\A\\B.DTX\r\n\
\t\t\tPropName: EmissionInterval\r\n\t\t\tType: 2\r\n\t\t\tFLOAT: -1.0\r\n\
\t\t\tPropName: DetailLevel\r\n\t\t\tType: 3\r\n\t\t\tCOMBO: 4,All,High\r\n\
\t\t\tPropName: Ck\r\n\t\t\tType: 6\r\n\t\t\tCLRKEY: 0.0 1.0 0.5 0.25 1.0\r\n\
\t\t\tPropName: Sk\r\n\t\t\tType: 5\r\n\t\t\tVECTOR4: 0.0 2.0 0 0\r\n\
\t\t\tPropName: Sk\r\n\t\t\tType: 5\r\n\t\t\tVECTOR4: 1.0 4.0 0 0\r\n\
\t\tFxName: DynaLight;Glow\r\n\t\tFxID: 5\r\n\t\tLinked: 0\r\n\t\tLinkID: 0\r\n\t\tLinkNode:\r\n\
\t\tStartTime: 0.0\r\n\t\tEndTime: 0.05\r\n\t\tRepeat: 1\r\n\t\tTrackID: 1\r\n\t\tMinScale: 0\r\n\
\t\tMaxScale: 170\r\n\t\tProperties: 1\r\n\
\t\t\tPropName: Offset\r\n\t\t\tType: 4\r\n\t\t\tVECTOR: 0.0 0.0 0.5\r\n\
\tGroupName: Empty\r\n\tFxInGroup: 0\r\n\tPhase: 1000\r\n";

    #[test]
    fn parses_groups_keys_and_typed_properties() {
        let f = FxFile::parse(SAMPLE).unwrap();
        assert_eq!(f.groups.len(), 2);
        let g = f.group("flash_a").expect("found without regard to case");
        assert_eq!((g.phase_ms, g.keys.len()), (50, 2));
        let k = &g.keys[0];
        assert_eq!(k.kind, "ParticleSystem");
        assert_eq!(k.end, 0.05, "clamped to the group's length");
        assert_eq!(k.combo("UpdatePos"), Some(6));
        assert_eq!(k.path("Texture"), Some("FX\\A\\B.DTX"));
        assert_eq!(k.float("EmissionInterval"), Some(-1.0));
        assert_eq!(k.color_keys(), vec![(0.0, [1.0, 0.5, 0.25, 0.0])], "alpha inverted");
        assert_eq!(k.scale_keys(), vec![(0.0, 2.0), (1.0, 4.0)]);
        assert!(!k.at_detail(0) && k.at_detail(1) && k.at_detail(2));
        assert_eq!((g.keys[1].kind.as_str(), g.keys[1].label.as_str()), ("DynaLight", "Glow"));
        assert_eq!(g.keys[1].vector("Offset"), Some([0.0, 0.0, 0.5]));
        assert!(f.group("Missing").is_none());
        assert_eq!(sample_keys(&k.scale_keys(), 0.5, 0.0), 3.0);
    }

    #[test]
    fn refuses_the_binary_form_and_reports_lines() {
        assert!(FxFile::parse("\u{1}\u{2}binary").is_err());
        let e = FxFile::parse("Groups: 1\nGroupName: A\nFxInGroup: x\n").unwrap_err();
        assert_eq!(e.line, 3);
        assert!(float("-nan(ind)").unwrap().is_nan());
        assert_eq!(float("1.#INF"), Some(f32::INFINITY));
        assert_eq!(float("x"), None);
    }
}
