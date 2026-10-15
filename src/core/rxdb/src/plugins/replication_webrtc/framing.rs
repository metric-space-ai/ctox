//! Negotiated V4 framing. Compression and allocation bounds are per frame;
//! messages are reassembled independently, so one bulk message cannot block control.
use super::hello::{Codec, TransportSelection, WireSeq};
use super::lanes::{Lane, FRAME_BYTES};
use std::collections::HashMap;
use std::io::{Read, Write};

const MAGIC: &[u8; 4] = b"CTX4";
const HEADER: usize = 37;
pub const COMPRESSION_MIN: usize = 1024;
pub const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const MAX_TRANSFERS: usize = 32;
const MAX_REASSEMBLY: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("invalid or incompatible V4 frame")]
    Invalid,
    #[error("V4 framing allocation or size limit exceeded")]
    Limit,
    #[error("V4 frame codec failed")]
    Codec,
}

#[derive(Clone, Debug)]
pub struct Framer {
    transport: TransportSelection,
}
impl Framer {
    pub fn new(transport: TransportSelection) -> Result<Self, FrameError> {
        if !(1024..=FRAME_BYTES as u32).contains(&transport.frame_bytes)
            || !transport.fragmentation
            || !transport.reassembly
        {
            return Err(FrameError::Invalid);
        }
        Ok(Self { transport })
    }
    pub fn frame_bytes(&self) -> usize {
        self.transport.frame_bytes as usize
    }
    pub fn encode(
        &self,
        lane: Lane,
        id: WireSeq,
        message: &[u8],
    ) -> Result<Vec<Vec<u8>>, FrameError> {
        if message.len() > MAX_MESSAGE {
            return Err(FrameError::Limit);
        }
        // Compression may expand; raw fragments always fit, including headers.
        let size = self.frame_bytes() - HEADER;
        let total = message.len().max(1).div_ceil(size);
        let mut out = Vec::with_capacity(total);
        for index in 0..total {
            let raw = &message
                [(index * size).min(message.len())..((index + 1) * size).min(message.len())];
            let compressed = if raw.len() >= COMPRESSION_MIN {
                Some(match self.transport.codec {
                    Codec::Zstd => {
                        zstd::stream::encode_all(raw, 1).map_err(|_| FrameError::Codec)?
                    }
                    Codec::Gzip => {
                        let mut encoder =
                            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                        encoder.write_all(raw).map_err(|_| FrameError::Codec)?;
                        encoder.finish().map_err(|_| FrameError::Codec)?
                    }
                })
            } else {
                None
            };
            let (codec, payload) = match compressed.as_ref() {
                Some(data) if data.len() < raw.len() => (
                    match self.transport.codec {
                        Codec::Zstd => 1,
                        Codec::Gzip => 2,
                    },
                    data.as_slice(),
                ),
                _ => (0, raw),
            };
            // Fixed-width decimal ASCII, not a JSON number or a lossy JS integer.
            let mut frame = Vec::with_capacity(HEADER + payload.len());
            frame.extend_from_slice(MAGIC);
            frame.push(lane as u8);
            frame.push(codec);
            let decimal = id.0.to_string();
            frame.push(decimal.len() as u8);
            frame.extend_from_slice(decimal.as_bytes());
            frame.resize(27, 0);
            frame.extend_from_slice(&(index as u32).to_be_bytes());
            frame.extend_from_slice(&(total as u32).to_be_bytes());
            frame.extend_from_slice(&(raw.len() as u16).to_be_bytes());
            frame.extend_from_slice(payload);
            out.push(frame);
        }
        Ok(out)
    }
    fn decode(&self, frame: &[u8]) -> Result<Fragment, FrameError> {
        if frame.len() < HEADER || frame.len() > self.frame_bytes() || &frame[..4] != MAGIC {
            return Err(FrameError::Invalid);
        }
        let lane = Lane::from_index(frame[4] as usize).ok_or(FrameError::Invalid)?;
        let id_len = frame[6] as usize;
        if !(1..=20).contains(&id_len) || frame[7 + id_len..27].iter().any(|v| *v != 0) {
            return Err(FrameError::Invalid);
        }
        let decimal =
            std::str::from_utf8(&frame[7..7 + id_len]).map_err(|_| FrameError::Invalid)?;
        if !decimal.bytes().all(|v| v.is_ascii_digit()) || (id_len > 1 && decimal.starts_with('0'))
        {
            return Err(FrameError::Invalid);
        }
        let id = decimal.parse::<u64>().map_err(|_| FrameError::Invalid)?;
        let index = u32::from_be_bytes(frame[27..31].try_into().unwrap()) as usize;
        let total = u32::from_be_bytes(frame[31..35].try_into().unwrap()) as usize;
        let raw_len = u16::from_be_bytes(frame[35..37].try_into().unwrap()) as usize;
        let chunk_size = self.frame_bytes() - HEADER;
        if total == 0
            || total > MAX_MESSAGE.div_ceil(chunk_size)
            || index >= total
            || raw_len > chunk_size
            || (index + 1 < total && raw_len != chunk_size)
        {
            return Err(FrameError::Limit);
        }
        let payload = &frame[HEADER..];
        let mut raw = Vec::with_capacity(raw_len);
        match frame[5] {
            0 => raw.extend_from_slice(payload),
            1 if self.transport.codec == Codec::Zstd && raw_len >= COMPRESSION_MIN => {
                zstd::stream::read::Decoder::new(payload)
                    .map_err(|_| FrameError::Codec)?
                    .take((raw_len + 1) as u64)
                    .read_to_end(&mut raw)
                    .map_err(|_| FrameError::Codec)?;
            }
            2 if self.transport.codec == Codec::Gzip && raw_len >= COMPRESSION_MIN => {
                flate2::read::GzDecoder::new(payload)
                    .take((raw_len + 1) as u64)
                    .read_to_end(&mut raw)
                    .map_err(|_| FrameError::Codec)?;
            }
            _ => return Err(FrameError::Invalid),
        }
        if raw.len() != raw_len {
            return Err(FrameError::Invalid);
        }
        Ok(Fragment {
            lane,
            id,
            index,
            total,
            raw,
        })
    }
}
struct Fragment {
    lane: Lane,
    id: u64,
    index: usize,
    total: usize,
    raw: Vec<u8>,
}
struct Partial {
    total: usize,
    next: usize,
    data: Vec<u8>,
}
pub struct Reassembler {
    framer: Framer,
    pending: HashMap<(usize, u64), Partial>,
    bytes: usize,
}
impl Reassembler {
    pub fn new(framer: Framer) -> Self {
        Self {
            framer,
            pending: HashMap::new(),
            bytes: 0,
        }
    }
    pub fn accept(&mut self, frame: &[u8]) -> Result<Option<(Lane, WireSeq, Vec<u8>)>, FrameError> {
        let f = self.framer.decode(frame)?;
        let key = (f.lane.index(), f.id);
        if f.index == 0 {
            if self.pending.contains_key(&key) {
                return Err(FrameError::Invalid);
            }
            if self.pending.len() >= MAX_TRANSFERS {
                return Err(FrameError::Limit);
            }
            self.pending.insert(
                key,
                Partial {
                    total: f.total,
                    next: 0,
                    data: Vec::new(),
                },
            );
        }
        let p = self.pending.get_mut(&key).ok_or(FrameError::Invalid)?;
        if p.total != f.total || p.next != f.index {
            return Err(FrameError::Invalid);
        }
        if p.data.len() + f.raw.len() > MAX_MESSAGE || self.bytes + f.raw.len() > MAX_REASSEMBLY {
            return Err(FrameError::Limit);
        }
        self.bytes += f.raw.len();
        p.data.extend_from_slice(&f.raw);
        p.next += 1;
        if p.next != p.total {
            return Ok(None);
        }
        let data = self.pending.remove(&key).unwrap().data;
        self.bytes -= data.len();
        Ok(Some((f.lane, WireSeq(f.id), data)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v4_framing_contract_codec_bounds_order_and_decimal_wire_identity() {
        v4_frames_negotiate_codecs_bound_frames_and_roundtrip_decimal_seq();
        v4_frames_allow_control_between_bulk_fragments_and_reject_bad_order();
        v4_frames_reject_expansion_size_and_unnegotiated_codec();
        super::fixture_tests::v4_frames_shared_fixture_uses_canonical_decimal_strings_and_exact_wire_bytes();
    }

    fn framer(codec: Codec) -> Framer {
        Framer::new(TransportSelection {
            codec,
            frame_bytes: FRAME_BYTES as u32,
            fragmentation: true,
            reassembly: true,
        })
        .unwrap()
    }
    fn v4_frames_negotiate_codecs_bound_frames_and_roundtrip_decimal_seq() {
        for codec in [Codec::Zstd, Codec::Gzip] {
            let f = framer(codec);
            assert_eq!(f.encode(Lane::Live, WireSeq(11), &vec![7; 1023]).unwrap()[0][5], 0);
            assert_ne!(f.encode(Lane::Live, WireSeq(12), &vec![7; 1024]).unwrap()[0][5], 0);
            let mut r = Reassembler::new(f.clone());
            let payload = vec![42; 200_000];
            let frames = f
                .encode(Lane::Transfer, WireSeq(u64::MAX), &payload)
                .unwrap();
            assert!(frames.iter().all(|f| f.len() <= FRAME_BYTES && f[5] != 0));
            assert_eq!(&frames[0][7..27], u64::MAX.to_string().as_bytes());
            let mut result = None;
            for frame in frames {
                result = r.accept(&frame).unwrap();
            }
            assert_eq!(
                result.unwrap(),
                (Lane::Transfer, WireSeq(u64::MAX), payload)
            );
            assert_eq!(r.bytes, 0);
        }
    }
    fn v4_frames_allow_control_between_bulk_fragments_and_reject_bad_order() {
        let f = framer(Codec::Zstd);
        let mut r = Reassembler::new(f.clone());
        let bulk = f
            .encode(Lane::Transfer, WireSeq(1), &vec![3; FRAME_BYTES * 2])
            .unwrap();
        assert!(r.accept(&bulk[0]).unwrap().is_none());
        let ctrl = f.encode(Lane::Control, WireSeq(2), b"ack").unwrap();
        assert_eq!(ctrl[0][5], 0);
        assert_eq!(r.accept(&ctrl[0]).unwrap().unwrap().2, b"ack");
        assert!(r.accept(&bulk[2]).is_err());
        assert!(r.accept(&bulk[0]).is_err());
    }
    fn v4_frames_reject_expansion_size_and_unnegotiated_codec() {
        let f = framer(Codec::Zstd);
        let frames = f.encode(Lane::Live, WireSeq(0), &vec![1; 2000]).unwrap();
        assert!(framer(Codec::Gzip).decode(&frames[0]).is_err());
        let mut bad = frames[0].clone();
        bad[35..37].copy_from_slice(&1u16.to_be_bytes());
        assert!(f.decode(&bad).is_err());
        assert!(f.decode(&vec![0; FRAME_BYTES + 1]).is_err());
        let mut tiny = f.transport.clone();
        tiny.frame_bytes = 100;
        assert!(Framer::new(tiny).is_err());
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    use base64::Engine;
    pub(super) fn v4_frames_shared_fixture_uses_canonical_decimal_strings_and_exact_wire_bytes() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("framing_fixture.json")).unwrap();
        let f = Framer::new(TransportSelection {
            codec: Codec::Zstd,
            frame_bytes: fixture["frame_bytes"].as_u64().unwrap() as u32,
            fragmentation: true,
            reassembly: true,
        })
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let lane = Lane::from_index(case["lane"].as_u64().unwrap() as usize).unwrap();
            let seq: WireSeq = serde_json::from_value(case["seq"].clone()).unwrap();
            let text = case["text"].as_str().unwrap().as_bytes();
            let expected = base64::engine::general_purpose::STANDARD
                .decode(case["frame_base64"].as_str().unwrap())
                .unwrap();
            assert_eq!(f.encode(lane, seq, text).unwrap(), vec![expected.clone()]);
            assert_eq!(
                Reassembler::new(f.clone())
                    .accept(&expected)
                    .unwrap()
                    .unwrap(),
                (lane, seq, text.to_vec())
            );
        }
        let mut noncanonical = f
            .encode(Lane::Control, WireSeq(0), b"ack")
            .unwrap()
            .remove(0);
        noncanonical[6] = 2;
        noncanonical[8] = b'0';
        assert!(f.decode(&noncanonical).is_err());
    }
}
