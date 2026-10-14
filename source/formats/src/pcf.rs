//! Particle files (`.pcf`): Valve's DMX, binary encoding 2 (the public
//! "DMX/Binary format" description), read as elements and their typed
//! attributes; and the particle manifests' file lists. What the systems
//! mean (operators, emitters, renderers) is the game's side
//! (`mashup::games::cs_source::pcf`).

use glam::Vec3;

use crate::keyvalues::{Escapes, tokens};

/// A DMX attribute value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Element(Option<usize>),
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
    Binary(Vec<u8>),
    Time(f32),
    Color([u8; 4]),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Matrix,
    Array(Vec<Value>),
}

/// A DMX element: its type, name and attributes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub kind: String,
    pub name: String,
    pub attributes: Vec<(String, Value)>,
}

impl Element {
    /// An attribute by name: exactly, else ignoring case and spaces
    /// against underscores (files of different tool versions spell
    /// `fade out time min` and `fade_out_time_min`).
    pub fn get(&self, key: &str) -> Option<&Value> {
        let norm = |s: &str| s.to_ascii_lowercase().replace(' ', "_");
        self.attributes
            .iter()
            .find(|(k, _)| k == key)
            .or_else(|| {
                let want = norm(key);
                self.attributes.iter().find(|(k, _)| norm(k) == want)
            })
            .map(|(_, v)| v)
    }

    pub fn float(&self, key: &str, default: f32) -> f32 {
        match self.get(key) {
            Some(Value::Float(f)) | Some(Value::Time(f)) => *f,
            Some(Value::Int(i)) => *i as f32,
            Some(Value::Bool(b)) => f32::from(u8::from(*b)),
            _ => default,
        }
    }

    pub fn int(&self, key: &str, default: i32) -> i32 {
        match self.get(key) {
            Some(Value::Int(i)) => *i,
            Some(Value::Float(f)) => *f as i32,
            Some(Value::Bool(b)) => i32::from(*b),
            _ => default,
        }
    }

    pub fn bool(&self, key: &str) -> bool {
        match self.get(key) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Int(i)) => *i != 0,
            _ => false,
        }
    }

    pub fn vec3(&self, key: &str, default: Vec3) -> Vec3 {
        match self.get(key) {
            Some(Value::Vec3(v)) => Vec3::from_array(*v),
            _ => default,
        }
    }

    /// A colour as 0-1 rgb and alpha.
    pub fn color(&self, key: &str) -> Option<(Vec3, f32)> {
        match self.get(key) {
            Some(Value::Color(c)) => Some((
                Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0,
                c[3] as f32 / 255.0,
            )),
            _ => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Value::Str(s)) => Some(s),
            _ => None,
        }
    }

    pub fn elements(&self, key: &str) -> Vec<usize> {
        match self.get(key) {
            Some(Value::Array(v)) => v
                .iter()
                .filter_map(|e| match e {
                    Value::Element(Some(i)) => Some(*i),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn element(&self, key: &str) -> Option<usize> {
        match self.get(key) {
            Some(Value::Element(i)) => *i,
            _ => None,
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.at..self.at + n).ok_or("truncated")?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn cstr(&mut self) -> Result<String, String> {
        let end = self.b[self.at..]
            .iter()
            .position(|c| *c == 0)
            .ok_or("unterminated string")?;
        let s = String::from_utf8_lossy(&self.b[self.at..self.at + end]).into_owned();
        self.at += end + 1;
        Ok(s)
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], String> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.f32()?;
        }
        Ok(out)
    }
}

/// Read a binary DMX file (encoding 2): its elements in file order.
pub fn read_dmx(bytes: &[u8]) -> Result<Vec<Element>, String> {
    let mut r = Reader { b: bytes, at: 0 };
    let header = r.cstr()?;
    if !header.contains("binary 2") {
        return Err(format!("unsupported DMX encoding: {}", header.trim()));
    }
    let n = r.i16()?.max(0) as usize;
    let mut strings = Vec::with_capacity(n);
    for _ in 0..n {
        strings.push(r.cstr()?);
    }
    let string = |i: i16| {
        strings
            .get(i as usize)
            .cloned()
            .ok_or_else(|| format!("bad string index {i}"))
    };
    let count = r.i32()?.max(0) as usize;
    if count > 1 << 20 {
        return Err("too many elements".into());
    }
    let mut elements = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = string(r.i16()?)?;
        let name = r.cstr()?;
        r.take(16)?;
        elements.push(Element {
            kind,
            name,
            attributes: Vec::new(),
        });
    }
    fn value(r: &mut Reader, t: u8) -> Result<Value, String> {
        Ok(match t {
            1 => {
                let i = r.i32()?;
                Value::Element((i >= 0).then_some(i as usize))
            }
            2 => Value::Int(r.i32()?),
            3 => Value::Float(r.f32()?),
            4 => Value::Bool(r.u8()? != 0),
            5 => Value::Str(r.cstr()?),
            6 => {
                let n = r.i32()?.max(0) as usize;
                Value::Binary(r.take(n)?.to_vec())
            }
            7 => Value::Time(r.i32()? as f32 / 10_000.0),
            8 => {
                let c = r.take(4)?;
                Value::Color([c[0], c[1], c[2], c[3]])
            }
            9 => Value::Vec2(r.floats::<2>()?),
            10 | 12 => Value::Vec3(r.floats::<3>()?),
            11 | 13 => Value::Vec4(r.floats::<4>()?),
            14 => {
                r.floats::<16>()?;
                Value::Matrix
            }
            15..=28 => {
                let n = r.i32()?.max(0) as usize;
                if n > 1 << 20 {
                    return Err("array too long".into());
                }
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(value(r, t - 14)?);
                }
                Value::Array(v)
            }
            _ => return Err(format!("unknown attribute type {t}")),
        })
    }
    for e in &mut elements {
        let n = r.i32()?.max(0) as usize;
        for _ in 0..n {
            let key = string(r.i16()?)?;
            let t = r.u8()?;
            let v = value(&mut r, t)?;
            e.attributes.push((key, v));
        }
    }
    Ok(elements)
}

/// The `file` entries of a particle manifest, without the precache `!`.
pub fn manifest_files(text: &str) -> Vec<String> {
    let t = tokens(text, Escapes::Off);
    t.windows(2)
        .filter(|w| w[0].eq_ignore_ascii_case("file"))
        .map(|w| w[1].trim_start_matches('!').replace('\\', "/"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny binary-2 DMX: a root with a system holding one emitter.
    fn sample() -> Vec<u8> {
        let mut b = b"<!-- dmx encoding binary 2 format pcf 1 -->\n\0".to_vec();
        let strings = [
            "DmElement",
            "DmeParticleSystemDefinition",
            "DmeParticleOperator",
            "particleSystemDefinitions",
            "emitters",
            "functionName",
            "emission_rate",
            "material",
        ];
        b.extend((strings.len() as i16).to_le_bytes());
        for s in strings {
            b.extend(s.as_bytes());
            b.push(0);
        }
        b.extend(3i32.to_le_bytes());
        for (kind, name) in [(0i16, "root"), (1, "fx"), (2, "emit")] {
            b.extend(kind.to_le_bytes());
            b.extend(name.as_bytes());
            b.push(0);
            b.extend([0u8; 16]);
        }
        // root: particleSystemDefinitions = [1]
        b.extend(1i32.to_le_bytes());
        b.extend(3i16.to_le_bytes());
        b.push(15);
        b.extend(1i32.to_le_bytes());
        b.extend(1i32.to_le_bytes());
        // fx: emitters = [2], material = "x"
        b.extend(2i32.to_le_bytes());
        b.extend(4i16.to_le_bytes());
        b.push(15);
        b.extend(1i32.to_le_bytes());
        b.extend(2i32.to_le_bytes());
        b.extend(7i16.to_le_bytes());
        b.push(5);
        b.extend(b"x\0");
        // emit: functionName, emission_rate 25
        b.extend(2i32.to_le_bytes());
        b.extend(5i16.to_le_bytes());
        b.push(5);
        b.extend(b"emit_continuously\0");
        b.extend(6i16.to_le_bytes());
        b.push(3);
        b.extend(25f32.to_le_bytes());
        b
    }

    #[test]
    fn reads_binary_dmx() {
        let e = read_dmx(&sample()).unwrap();
        assert_eq!(e.len(), 3);
        assert_eq!(e[1].kind, "DmeParticleSystemDefinition");
        assert_eq!(e[0].elements("particleSystemDefinitions"), vec![1]);
        assert_eq!(e[1].str("material"), Some("x"));
        assert_eq!(e[1].elements("emitters"), vec![2]);
        assert_eq!(e[2].str("functionName"), Some("emit_continuously"));
        assert_eq!(e[2].float("emission_rate", 0.0), 25.0);
        // Spaces and underscores, any case, name the same attribute.
        assert_eq!(e[2].float("Emission Rate", 0.0), 25.0);
        assert!(read_dmx(b"<!-- dmx encoding keyvalues2 1 format pcf 1 -->\n\0").is_err());
    }

    #[test]
    fn manifest_entries() {
        let text = "particles_manifest\n{\n\t\"file\"\t\"!particles/a.pcf\"\n\tfile \"particles/b.pcf\"\n}";
        assert_eq!(manifest_files(text), vec!["particles/a.pcf", "particles/b.pcf"]);
    }
}
