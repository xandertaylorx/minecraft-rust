//! Anvil region files (`r.<x>.<z>.mca`): 32x32 chunks in 4 KiB sectors.
use std::io::{self, Read, Write};

use crate::{read_root, write_root, Tag};

const SECTOR: usize = 4096;

pub struct Region {
    /// Per-chunk (compression, payload) as stored; chunk index = x + z*32.
    chunks: Vec<Option<(u8, Vec<u8>)>>,
    timestamps: Vec<u32>,
}

fn bad(msg: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, msg.to_string()) }

impl Region {
    pub fn empty() -> Self { Self { chunks: vec![None; 1024], timestamps: vec![0; 1024] } }

    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let mut r = Self::empty();
        if data.len() < 2 * SECTOR { return if data.is_empty() { Ok(r) } else { Err(bad("truncated header")) }; }
        for i in 0..1024 {
            let e = u32::from_be_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
            r.timestamps[i] = u32::from_be_bytes(data[SECTOR + i * 4..SECTOR + i * 4 + 4].try_into().unwrap());
            let (offset, count) = ((e >> 8) as usize, (e & 0xFF) as usize);
            if offset == 0 || count == 0 { continue; }
            let start = offset * SECTOR;
            if start + 5 > data.len() { return Err(bad("chunk offset out of range")); }
            let length = u32::from_be_bytes(data[start..start + 4].try_into().unwrap()) as usize;
            if length == 0 || length > count * SECTOR || start + 4 + length > data.len() { return Err(bad("bad chunk length")); }
            r.chunks[i] = Some((data[start + 4], data[start + 5..start + 4 + length].to_vec()));
        }
        Ok(r)
    }

    pub fn has_chunk(&self, x: usize, z: usize) -> bool { self.chunks[x + z * 32].is_some() }
    pub fn chunk_count(&self) -> usize { self.chunks.iter().flatten().count() }

    pub fn read_chunk(&self, x: usize, z: usize) -> io::Result<Option<Tag>> {
        let Some((comp, payload)) = &self.chunks[x + z * 32] else { return Ok(None) };
        let tag = match comp {
            1 => read_root(&mut flate2::read::GzDecoder::new(&payload[..]))?.1,
            2 => read_root(&mut flate2::read::ZlibDecoder::new(&payload[..]))?.1,
            3 => read_root(&mut &payload[..])?.1,
            _ => return Err(bad("unsupported compression (LZ4/external chunks not handled yet)")),
        };
        Ok(Some(tag))
    }

    pub fn write_chunk(&mut self, x: usize, z: usize, tag: &Tag, timestamp: u32) -> io::Result<()> {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        write_root(&mut enc, "", tag)?;
        self.chunks[x + z * 32] = Some((2, enc.finish()?));
        self.timestamps[x + z * 32] = timestamp;
        Ok(())
    }

    pub fn remove_chunk(&mut self, x: usize, z: usize) { self.chunks[x + z * 32] = None; }

    /// Serialise with chunks packed contiguously after the two header sectors.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut header = vec![0u8; 2 * SECTOR];
        let mut body: Vec<u8> = Vec::new();
        let mut next = 2usize;
        for (i, c) in self.chunks.iter().enumerate() {
            header[SECTOR + i * 4..SECTOR + i * 4 + 4].copy_from_slice(&self.timestamps[i].to_be_bytes());
            let Some((comp, payload)) = c else { continue };
            let total = 5 + payload.len();
            let sectors = total.div_ceil(SECTOR);
            header[i * 4..i * 4 + 4].copy_from_slice(&(((next as u32) << 8) | sectors as u32).to_be_bytes());
            body.extend_from_slice(&((payload.len() + 1) as u32).to_be_bytes());
            body.push(*comp);
            body.extend_from_slice(payload);
            body.resize(body.len() + sectors * SECTOR - total, 0);
            next += sectors;
        }
        let mut out = header;
        out.write_all(&body).unwrap();
        out
    }

    pub fn from_reader(mut r: impl Read) -> io::Result<Self> {
        let mut v = Vec::new();
        r.read_to_end(&mut v)?;
        Self::parse(&v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn region_round_trip() {
        let mut r = Region::empty();
        let mut m = BTreeMap::new();
        m.insert("xPos".into(), Tag::Int(5));
        m.insert("big".into(), Tag::LongArray((0..2000).map(|i| i * 31).collect()));
        let t = Tag::Compound(m);
        r.write_chunk(3, 7, &t, 99).unwrap();
        r.write_chunk(31, 31, &t, 100).unwrap();
        let bytes = r.to_bytes();
        assert_eq!(bytes.len() % SECTOR, 0);
        let back = Region::parse(&bytes).unwrap();
        assert_eq!(back.chunk_count(), 2);
        assert!(!back.has_chunk(0, 0));
        assert_eq!(back.read_chunk(3, 7).unwrap().unwrap(), t);
        assert_eq!(back.read_chunk(31, 31).unwrap().unwrap(), t);
        assert_eq!(back.to_bytes(), bytes);
    }

    #[test]
    fn rejects_corrupt_offsets() {
        let mut data = vec![0u8; 2 * SECTOR];
        data[0..4].copy_from_slice(&((50u32 << 8) | 1).to_be_bytes());
        assert!(Region::parse(&data).is_err());
    }
}
