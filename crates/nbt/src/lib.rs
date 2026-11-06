//! Minecraft NBT (binary) and Anvil region files (`.mca`).
pub mod region;

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    /// Element type id is kept so empty lists round-trip exactly.
    List(u8, Vec<Tag>),
    Compound(BTreeMap<String, Tag>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub fn id(&self) -> u8 {
        match self {
            Tag::End => 0, Tag::Byte(_) => 1, Tag::Short(_) => 2, Tag::Int(_) => 3, Tag::Long(_) => 4,
            Tag::Float(_) => 5, Tag::Double(_) => 6, Tag::ByteArray(_) => 7, Tag::String(_) => 8,
            Tag::List(..) => 9, Tag::Compound(_) => 10, Tag::IntArray(_) => 11, Tag::LongArray(_) => 12,
        }
    }
    pub fn get(&self, key: &str) -> Option<&Tag> {
        if let Tag::Compound(m) = self { m.get(key) } else { None }
    }
    pub fn as_i32(&self) -> Option<i32> {
        match self { Tag::Int(v) => Some(*v), Tag::Short(v) => Some(*v as i32), Tag::Byte(v) => Some(*v as i32), _ => None }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Tag::String(s) = self { Some(s) } else { None }
    }
}

fn bad(msg: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, msg.to_string()) }

macro_rules! rd { ($r:expr, $t:ty) => {{ let mut b = [0u8; std::mem::size_of::<$t>()]; $r.read_exact(&mut b)?; <$t>::from_be_bytes(b) }}; }

fn read_string(r: &mut impl Read) -> io::Result<String> {
    let len = rd!(r, u16) as usize;
    let mut b = vec![0; len];
    r.read_exact(&mut b)?;
    decode_mutf8(&b)
}

/// Modified UTF-8 (NUL as C0 80, supplementary chars as surrogate pairs).
fn decode_mutf8(b: &[u8]) -> io::Result<String> {
    if b.iter().all(|&c| c != 0 && c < 0x80) {
        return String::from_utf8(b.to_vec()).map_err(|_| bad("utf8"));
    }
    let mut units: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c < 0x80 { units.push(c as u16); i += 1; }
        else if c & 0xE0 == 0xC0 && i + 1 < b.len() { units.push(((c as u16 & 0x1F) << 6) | (b[i + 1] as u16 & 0x3F)); i += 2; }
        else if c & 0xF0 == 0xE0 && i + 2 < b.len() {
            units.push(((c as u16 & 0x0F) << 12) | ((b[i + 1] as u16 & 0x3F) << 6) | (b[i + 2] as u16 & 0x3F)); i += 3;
        } else { return Err(bad("mutf8")); }
    }
    String::from_utf16(&units).map_err(|_| bad("utf16"))
}

fn encode_mutf8(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for u in s.encode_utf16() {
        match u {
            1..=0x7F => out.push(u as u8),
            0 | 0x80..=0x7FF => { out.push(0xC0 | (u >> 6) as u8); out.push(0x80 | (u & 0x3F) as u8); }
            _ => { out.push(0xE0 | (u >> 12) as u8); out.push(0x80 | ((u >> 6) & 0x3F) as u8); out.push(0x80 | (u & 0x3F) as u8); }
        }
    }
    out
}

fn read_payload(r: &mut impl Read, id: u8, depth: u32) -> io::Result<Tag> {
    if depth > 512 { return Err(bad("nesting too deep")); }
    Ok(match id {
        1 => Tag::Byte(rd!(r, i8)),
        2 => Tag::Short(rd!(r, i16)),
        3 => Tag::Int(rd!(r, i32)),
        4 => Tag::Long(rd!(r, i64)),
        5 => Tag::Float(f32::from_bits(rd!(r, u32))),
        6 => Tag::Double(f64::from_bits(rd!(r, u64))),
        7 => { let n = len(r)?; let mut v = Vec::with_capacity(n.min(1 << 20)); for _ in 0..n { v.push(rd!(r, i8)); } Tag::ByteArray(v) }
        8 => Tag::String(read_string(r)?),
        9 => {
            let t = rd!(r, u8);
            let n = len(r)?;
            if t == 0 && n > 0 { return Err(bad("list of End")); }
            let mut v = Vec::with_capacity(n.min(1 << 16));
            for _ in 0..n { v.push(read_payload(r, t, depth + 1)?); }
            Tag::List(t, v)
        }
        10 => {
            let mut m = BTreeMap::new();
            loop {
                let t = rd!(r, u8);
                if t == 0 { break; }
                let k = read_string(r)?;
                m.insert(k, read_payload(r, t, depth + 1)?);
            }
            Tag::Compound(m)
        }
        11 => { let n = len(r)?; let mut v = Vec::with_capacity(n.min(1 << 18)); for _ in 0..n { v.push(rd!(r, i32)); } Tag::IntArray(v) }
        12 => { let n = len(r)?; let mut v = Vec::with_capacity(n.min(1 << 17)); for _ in 0..n { v.push(rd!(r, i64)); } Tag::LongArray(v) }
        _ => return Err(bad("unknown tag id")),
    })
}

fn len(r: &mut impl Read) -> io::Result<usize> {
    let n = rd!(r, i32);
    if n < 0 { Err(bad("negative length")) } else { Ok(n as usize) }
}

/// Reads one named root tag (`name`, tag), as in `level.dat` or a chunk.
pub fn read_root(r: &mut impl Read) -> io::Result<(String, Tag)> {
    let id = rd!(r, u8);
    if id != 10 { return Err(bad("root must be a compound")); }
    let name = read_string(r)?;
    Ok((name, read_payload(r, id, 0)?))
}

fn write_payload(w: &mut impl Write, t: &Tag) -> io::Result<()> {
    match t {
        Tag::End => {}
        Tag::Byte(v) => w.write_all(&v.to_be_bytes())?,
        Tag::Short(v) => w.write_all(&v.to_be_bytes())?,
        Tag::Int(v) => w.write_all(&v.to_be_bytes())?,
        Tag::Long(v) => w.write_all(&v.to_be_bytes())?,
        Tag::Float(v) => w.write_all(&v.to_bits().to_be_bytes())?,
        Tag::Double(v) => w.write_all(&v.to_bits().to_be_bytes())?,
        Tag::ByteArray(v) => { w.write_all(&(v.len() as i32).to_be_bytes())?; for b in v { w.write_all(&b.to_be_bytes())?; } }
        Tag::String(s) => write_string(w, s)?,
        Tag::List(id, v) => {
            w.write_all(&[*id])?;
            w.write_all(&(v.len() as i32).to_be_bytes())?;
            for e in v { write_payload(w, e)?; }
        }
        Tag::Compound(m) => {
            for (k, v) in m { w.write_all(&[v.id()])?; write_string(w, k)?; write_payload(w, v)?; }
            w.write_all(&[0])?;
        }
        Tag::IntArray(v) => { w.write_all(&(v.len() as i32).to_be_bytes())?; for x in v { w.write_all(&x.to_be_bytes())?; } }
        Tag::LongArray(v) => { w.write_all(&(v.len() as i32).to_be_bytes())?; for x in v { w.write_all(&x.to_be_bytes())?; } }
    }
    Ok(())
}

fn write_string(w: &mut impl Write, s: &str) -> io::Result<()> {
    let b = encode_mutf8(s);
    if b.len() > u16::MAX as usize { return Err(bad("string too long")); }
    w.write_all(&(b.len() as u16).to_be_bytes())?;
    w.write_all(&b)
}

pub fn write_root(w: &mut impl Write, name: &str, tag: &Tag) -> io::Result<()> {
    if !matches!(tag, Tag::Compound(_)) { return Err(bad("root must be a compound")); }
    w.write_all(&[10])?;
    write_string(w, name)?;
    write_payload(w, tag)
}

/// gzip-compressed root, as used by `level.dat`.
pub fn read_gzip(bytes: &[u8]) -> io::Result<(String, Tag)> {
    read_root(&mut flate2::read::GzDecoder::new(bytes))
}

pub fn write_gzip(name: &str, tag: &Tag) -> io::Result<Vec<u8>> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    write_root(&mut enc, name, tag)?;
    enc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Tag {
        let mut m = BTreeMap::new();
        m.insert("b".into(), Tag::Byte(-3));
        m.insert("s".into(), Tag::String("nul\0 and \u{1F600} and é".into()));
        m.insert("l".into(), Tag::List(3, vec![Tag::Int(1), Tag::Int(-2)]));
        m.insert("empty".into(), Tag::List(0, vec![]));
        m.insert("la".into(), Tag::LongArray(vec![i64::MIN, 7]));
        m.insert("d".into(), Tag::Double(-0.5));
        let mut inner = BTreeMap::new();
        inner.insert("ia".into(), Tag::IntArray(vec![1, 2, 3]));
        m.insert("c".into(), Tag::Compound(inner));
        Tag::Compound(m)
    }

    #[test]
    fn round_trip_plain_and_gzip() {
        let t = sample();
        let mut buf = Vec::new();
        write_root(&mut buf, "root", &t).unwrap();
        let (n, back) = read_root(&mut &buf[..]).unwrap();
        assert_eq!((n.as_str(), &back), ("root", &t));
        let gz = write_gzip("", &t).unwrap();
        assert_eq!(read_gzip(&gz).unwrap().1, t);
    }

    #[test]
    fn known_bytes_from_the_wiki_hello_world() {
        // https://minecraft.wiki NBT format example: {hello world: {name: "Bananrama"}}
        let bytes: [u8; 33] = [
            0x0a, 0x00, 0x0b, b'h', b'e', b'l', b'l', b'o', b' ', b'w', b'o', b'r', b'l', b'd', 0x08, 0x00, 0x04,
            b'n', b'a', b'm', b'e', 0x00, 0x09, b'B', b'a', b'n', b'a', b'n', b'r', b'a', b'm', b'a', 0x00,
        ];
        let (name, tag) = read_root(&mut &bytes[..]).unwrap();
        assert_eq!(name, "hello world");
        assert_eq!(tag.get("name").unwrap().as_str(), Some("Bananrama"));
        let mut out = Vec::new();
        write_root(&mut out, &name, &tag).unwrap();
        assert_eq!(out, bytes);
    }

    #[test]
    fn rejects_garbage() {
        assert!(read_root(&mut &[0x0a, 0, 0, 9, 0, 0xff, 0xff, 0xff, 0xff][..]).is_err());
        assert!(read_root(&mut &[1u8, 0, 0][..]).is_err());
    }
}
