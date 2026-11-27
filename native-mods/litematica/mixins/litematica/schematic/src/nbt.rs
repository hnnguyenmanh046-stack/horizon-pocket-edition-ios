//! A small reader for Minecraft's NBT (big-endian, as in .litematic files),
//! with limits so a broken or hostile file can't make it allocate wildly.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<Tag>),
    Compound(BTreeMap<String, Tag>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(map) => map.get(key),
            _ => None,
        }
    }
    pub fn compound(&self) -> Option<&BTreeMap<String, Tag>> {
        match self {
            Tag::Compound(map) => Some(map),
            _ => None,
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn list(&self) -> Option<&[Tag]> {
        match self {
            Tag::List(l) => Some(l),
            _ => None,
        }
    }
    pub fn longs(&self) -> Option<&[i64]> {
        match self {
            Tag::LongArray(l) => Some(l),
            _ => None,
        }
    }
    /// Any whole number as an i64.
    pub fn int(&self) -> Option<i64> {
        match *self {
            Tag::Byte(v) => Some(v.into()),
            Tag::Short(v) => Some(v.into()),
            Tag::Int(v) => Some(v.into()),
            Tag::Long(v) => Some(v),
            _ => None,
        }
    }
}

/// The most a file may hold: 256 MiB of arrays, nesting 64 deep.
const MAX_BYTES: usize = 256 << 20;
const MAX_DEPTH: usize = 64;

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    budget: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        if n > self.data.len() - self.at {
            return Err("the file ends too early".into());
        }
        let out = &self.data[self.at..self.at + n];
        self.at += n;
        Ok(out)
    }
    fn spend(&mut self, n: usize) -> Result<(), String> {
        self.budget = self.budget.checked_sub(n).ok_or("the file is too large")?;
        Ok(())
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn len(&mut self, each: usize) -> Result<usize, String> {
        let n = self.i32()?;
        let n = usize::try_from(n).map_err(|_| "a negative length")?;
        self.spend(n.saturating_mul(each))?;
        Ok(n)
    }
    fn string(&mut self) -> Result<String, String> {
        let n = u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as usize;
        // Java's modified UTF-8 is UTF-8 for everything a schematic names.
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn payload(&mut self, id: u8, depth: usize) -> Result<Tag, String> {
        if depth > MAX_DEPTH {
            return Err("the file nests too deep".into());
        }
        Ok(match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len(1)?;
                Tag::ByteArray(self.take(n)?.to_vec())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let kind = self.u8()?;
                let n = self.len(1)?;
                if kind == 0 && n > 0 {
                    return Err("a list of nothing".into());
                }
                let mut out = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    out.push(self.payload(kind, depth + 1)?);
                }
                Tag::List(out)
            }
            10 => {
                let mut map = BTreeMap::new();
                loop {
                    let kind = self.u8()?;
                    if kind == 0 {
                        break;
                    }
                    let name = self.string()?;
                    let value = self.payload(kind, depth + 1)?;
                    map.insert(name, value);
                }
                Tag::Compound(map)
            }
            11 => {
                let n = self.len(4)?;
                Tag::IntArray((0..n).map(|_| self.i32()).collect::<Result<_, _>>()?)
            }
            12 => {
                let n = self.len(8)?;
                Tag::LongArray((0..n).map(|_| self.i64()).collect::<Result<_, _>>()?)
            }
            other => return Err(format!("an unknown tag type {other}")),
        })
    }
}

/// The root compound of uncompressed NBT bytes.
pub fn read(data: &[u8]) -> Result<Tag, String> {
    let mut r = Reader { data, at: 0, budget: MAX_BYTES };
    if r.u8()? != 10 {
        return Err("it doesn't start with a compound".into());
    }
    let _name = r.string()?;
    r.payload(10, 0)
}

/// The root compound of gzip-compressed NBT (a .litematic file).
pub fn read_gzip(data: &[u8]) -> Result<Tag, String> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(data)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("it isn't a gzip file ({e})"))?;
    if out.len() > MAX_BYTES {
        return Err("the file is too large".into());
    }
    read(&out)
}

/// Writing, for the tests (and saving schematics later).
pub mod write {
    use super::Tag;

    fn string(out: &mut Vec<u8>, s: &str) {
        out.extend((s.len() as u16).to_be_bytes());
        out.extend(s.as_bytes());
    }

    fn id(t: &Tag) -> u8 {
        match t {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(_) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }

    fn payload(out: &mut Vec<u8>, t: &Tag) {
        match t {
            Tag::Byte(v) => out.push(*v as u8),
            Tag::Short(v) => out.extend(v.to_be_bytes()),
            Tag::Int(v) => out.extend(v.to_be_bytes()),
            Tag::Long(v) => out.extend(v.to_be_bytes()),
            Tag::Float(v) => out.extend(v.to_bits().to_be_bytes()),
            Tag::Double(v) => out.extend(v.to_bits().to_be_bytes()),
            Tag::ByteArray(v) => {
                out.extend((v.len() as i32).to_be_bytes());
                out.extend(v);
            }
            Tag::String(s) => string(out, s),
            Tag::List(l) => {
                out.push(l.first().map_or(0, id));
                out.extend((l.len() as i32).to_be_bytes());
                for t in l {
                    payload(out, t);
                }
            }
            Tag::Compound(map) => {
                for (k, v) in map {
                    out.push(id(v));
                    string(out, k);
                    payload(out, v);
                }
                out.push(0);
            }
            Tag::IntArray(v) => {
                out.extend((v.len() as i32).to_be_bytes());
                for x in v {
                    out.extend(x.to_be_bytes());
                }
            }
            Tag::LongArray(v) => {
                out.extend((v.len() as i32).to_be_bytes());
                for x in v {
                    out.extend(x.to_be_bytes());
                }
            }
        }
    }

    /// `root` (a compound) as uncompressed NBT.
    pub fn to_bytes(root: &Tag) -> Vec<u8> {
        let mut out = vec![10];
        string(&mut out, "");
        payload(&mut out, root);
        out
    }

    /// `root` as gzip-compressed NBT.
    pub fn to_gzip(root: &Tag) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(&to_bytes(root)).unwrap();
        e.finish().unwrap()
    }
}
