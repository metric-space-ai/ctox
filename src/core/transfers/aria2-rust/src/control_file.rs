//! C++ `DefaultBtProgressInfoFile` (`*.aria2`) — HTTP bitfield resume.
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

const VERSION: u32 = 1;

pub fn path_for(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".aria2");
    PathBuf::from(s)
}

pub fn remove(dest: &Path) {
    let _ = crate::storage::unlink(&path_for(dest));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub piece_length: u32,
    pub total_length: u64,
    pub bits: Vec<u8>,
}

impl Control {
    pub fn new(piece_length: u32, total_length: u64) -> Self {
        let n = n_pieces(piece_length, total_length);
        Self {
            piece_length,
            total_length,
            bits: vec![0u8; (n + 7) / 8],
        }
    }

    pub fn n_pieces(&self) -> usize {
        n_pieces(self.piece_length, self.total_length)
    }

    pub fn has(&self, i: u32) -> bool {
        let i = i as usize;
        let byte = match self.bits.get(i / 8) {
            Some(b) => *b,
            None => return false,
        };
        byte & (0x80 >> (i % 8)) != 0
    }

    pub fn set(&mut self, i: u32) {
        let i = i as usize;
        if let Some(b) = self.bits.get_mut(i / 8) {
            *b |= 0x80 >> (i % 8);
        }
    }

    pub fn any(&self) -> bool {
        self.bits.iter().any(|b| *b != 0)
    }

    pub fn save(&self, dest: &Path) -> std::io::Result<()> {
        let path = path_for(dest);
        let tmp = path.with_extension("aria2.tmp");
        crate::storage::write_file(&tmp, self.encode())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn load(dest: &Path) -> Option<Self> {
        let bytes = crate::storage::read_file(&path_for(dest)).ok()?;
        decode(&bytes)
    }

    fn encode(&self) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend_from_slice(&VERSION.to_be_bytes());
        o.extend_from_slice(&0u32.to_be_bytes());
        o.extend_from_slice(&0u32.to_be_bytes());
        o.extend_from_slice(&self.piece_length.to_be_bytes());
        o.extend_from_slice(&self.total_length.to_be_bytes());
        o.extend_from_slice(&0u64.to_be_bytes());
        o.extend_from_slice(&(self.bits.len() as u32).to_be_bytes());
        o.extend_from_slice(&self.bits);
        o
    }
}

fn n_pieces(piece_length: u32, total_length: u64) -> usize {
    if piece_length == 0 || total_length == 0 {
        0
    } else {
        ((total_length + piece_length as u64 - 1) / piece_length as u64) as usize
    }
}

fn decode(b: &[u8]) -> Option<Control> {
    if b.len() < 36 {
        return None;
    }
    let version = u32::from_be_bytes(b[0..4].try_into().ok()?);
    if version != VERSION {
        return None;
    }
    let ih_len = u32::from_be_bytes(b[8..12].try_into().ok()?) as usize;
    let mut off = 12 + ih_len;
    if b.len() < off + 24 {
        return None;
    }
    let piece_length = u32::from_be_bytes(b[off..off + 4].try_into().ok()?);
    off += 4;
    let total_length = u64::from_be_bytes(b[off..off + 8].try_into().ok()?);
    off += 8;
    off += 8; // upload
    let bf_len = u32::from_be_bytes(b[off..off + 4].try_into().ok()?) as usize;
    off += 4;
    if b.len() < off + bf_len {
        return None;
    }
    Some(Control {
        piece_length,
        total_length,
        bits: b[off..off + bf_len].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bitfield() {
        let mut c = Control::new(4096, 8192);
        assert_eq!(c.n_pieces(), 2);
        assert!(!c.has(0) && !c.has(1));
        c.set(1);
        assert!(c.has(1) && !c.has(0));
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("x.bin");
        crate::storage::reset_file_write();
        crate::storage::reset_file_read();
        crate::storage::reset_unlink();
        c.save(&dest).unwrap();
        assert!(
            crate::storage::last_file_write() >= 1,
            "C++ DefaultBtProgressInfoFile File::write .aria2"
        );
        let loaded = Control::load(&dest).unwrap();
        assert_eq!(loaded, c);
        assert!(
            crate::storage::last_file_read() >= 1,
            "C++ DefaultBtProgressInfoFile File::read .aria2"
        );
        remove(&dest);
        assert!(
            crate::storage::last_unlink() >= 1,
            "C++ DefaultBtProgressInfoFile File::remove .aria2"
        );
        assert!(Control::load(&dest).is_none());
    }
}
