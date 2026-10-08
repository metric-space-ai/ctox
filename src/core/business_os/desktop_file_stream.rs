//! Bounded reads that preserve the existing whole-file Base64 chunk format.
use sha2::{Digest, Sha256};
use std::io::{self, Read, Seek, SeekFrom};

pub(super) const CHUNK_BYTES: usize = super::DESKTOP_FILE_CHUNK_DECODED_SIZE as usize;
pub(super) const BATCH_CHUNKS: usize = 64;

pub(super) fn hash_and_rewind(reader: &mut (impl Read + Seek)) -> io::Result<(u64, String)> {
    reader.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        size = size.checked_add(count as u64).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "desktop file is too large")
        })?;
    }
    reader.seek(SeekFrom::Start(0))?;
    Ok((size, format!("{:x}", hash.finalize())))
}

pub(super) fn read_chunk(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(CHUNK_BYTES);
    reader.take(CHUNK_BYTES as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::io::Cursor;

    struct ShortReads<R>(R);
    impl<R: Read> Read for ShortReads<R> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let limit = out.len().min(7);
            self.0.read(&mut out[..limit])
        }
    }

    #[test]
    fn streamed_base64_matches_stored_format_across_short_reads_and_batch_boundaries() {
        for size in [
            0,
            1,
            2,
            3,
            CHUNK_BYTES - 1,
            CHUNK_BYTES,
            CHUNK_BYTES + 1,
            2 * CHUNK_BYTES + 1,
            BATCH_CHUNKS * CHUNK_BYTES + 5,
        ] {
            let bytes: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            let mut reader = ShortReads(Cursor::new(&bytes));
            let mut encoded = String::new();
            loop {
                let chunk = read_chunk(&mut reader).unwrap();
                if chunk.is_empty() {
                    break;
                }
                assert!(chunk.len() <= CHUNK_BYTES);
                encoded.push_str(&base64::engine::general_purpose::STANDARD.encode(chunk));
            }
            assert_eq!(
                encoded,
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            );
        }
    }

    #[test]
    fn content_hash_pass_rewinds_the_same_open_file() {
        let bytes = vec![0xa5; CHUNK_BYTES + 17];
        let mut reader = Cursor::new(&bytes);
        reader.set_position(19);
        let (size, hash) = hash_and_rewind(&mut reader).unwrap();
        assert_eq!(size, bytes.len() as u64);
        assert_eq!(hash, format!("{:x}", Sha256::digest(&bytes)));
        assert_eq!(reader.position(), 0);
        assert_eq!(read_chunk(&mut reader).unwrap(), bytes[..CHUNK_BYTES]);
    }
}
