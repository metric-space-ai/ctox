use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::error::BinaryError;

/// Inverse of the leading-byte trick in `marshal`: byte 0 indicates whether
/// the rest is zlib-compressed (`& 2 != 0`), and the remainder is the actual
/// payload.
pub fn unpack(data: &[u8]) -> Result<Vec<u8>, BinaryError> {
    if data.is_empty() {
        return Err(BinaryError::UnexpectedEof);
    }
    let dtype = data[0];
    let body = &data[1..];
    if dtype & 2 != 0 {
        let mut dec = ZlibDecoder::new(body);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).map_err(|e| BinaryError::Zlib(e.to_string()))?;
        Ok(out)
    } else {
        Ok(body.to_vec())
    }
}
