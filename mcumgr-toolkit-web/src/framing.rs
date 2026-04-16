use base64::prelude::*;
use crc::Crc;

use crate::smp::SMP_HEADER_SIZE;

/// See Zephyr's `MCUMGR_SERIAL_MAX_FRAME`.
const ZEPHYR_MTU: usize = 127;

const INITIAL_HEADER: [u8; 2] = [0x06, 0x09];
const CONTINUATION_HEADER: [u8; 2] = [0x04, 0x14];

pub struct SerialFramer {
    crc_algo: Crc<u16>,
    /// Max raw bytes per chunk: ((mtu - 3) / 4) * 3
    body_buf_size: usize,
}

impl SerialFramer {
    pub fn new() -> Self {
        let body_buf_size = ((ZEPHYR_MTU - 3) / 4) * 3;
        Self {
            crc_algo: Crc::<u16>::new(&crc::CRC_16_XMODEM),
            body_buf_size,
        }
    }

    /// Encode an SMP frame into serial line chunks ready to send.
    ///
    /// Returns a list of byte vectors, each representing one serial line
    /// (including the 2-byte header and trailing newline).
    pub fn encode_frame(&self, header: [u8; SMP_HEADER_SIZE], data: &[u8]) -> Vec<Vec<u8>> {
        let checksum = {
            let mut digest = self.crc_algo.digest();
            digest.update(&header);
            digest.update(data);
            digest.finalize().to_be_bytes()
        };

        let size = (header.len() + data.len() + checksum.len()) as u16;
        let size_bytes = size.to_be_bytes();

        // Build the raw data stream: size ++ header ++ data ++ checksum
        let mut raw_stream =
            Vec::with_capacity(size_bytes.len() + header.len() + data.len() + checksum.len());
        raw_stream.extend_from_slice(&size_bytes);
        raw_stream.extend_from_slice(&header);
        raw_stream.extend_from_slice(data);
        raw_stream.extend_from_slice(&checksum);

        let mut lines = Vec::new();
        let mut offset = 0;
        let mut is_first = true;

        while offset < raw_stream.len() {
            let end = (offset + self.body_buf_size).min(raw_stream.len());
            let chunk = &raw_stream[offset..end];

            let b64 = BASE64_STANDARD.encode(chunk);
            let frame_header = if is_first {
                INITIAL_HEADER
            } else {
                CONTINUATION_HEADER
            };

            let mut line = Vec::with_capacity(2 + b64.len() + 1);
            line.extend_from_slice(&frame_header);
            line.extend_from_slice(b64.as_bytes());
            line.push(0x0a);

            lines.push(line);
            offset = end;
            is_first = false;
        }

        lines
    }

    /// Attempt to decode a complete SMP frame from accumulated received bytes.
    ///
    /// Returns `Ok(Some((data, consumed)))` if a complete frame was decoded,
    /// where `data` contains the SMP header + CBOR payload and `consumed` is
    /// how many bytes were consumed from the buffer.
    ///
    /// Returns `Ok(None)` if not enough data is available yet.
    ///
    /// Returns `Err` on protocol errors (bad CRC, etc).
    pub fn decode_frame(&self, buf: &[u8]) -> Result<Option<(Vec<u8>, usize)>, FrameError> {
        // Find the initial header
        let Some(start) = find_header(buf, &INITIAL_HEADER) else {
            return Ok(None);
        };

        let mut pos = start + 2;

        // Read first chunk until newline
        let Some(first_b64_end) = find_newline(buf, pos) else {
            return Ok(None);
        };
        let first_b64 = &buf[pos..first_b64_end];
        pos = first_b64_end + 1;

        let first_decoded = BASE64_STANDARD
            .decode(first_b64)
            .map_err(|_| FrameError::Base64)?;

        if first_decoded.len() < 2 {
            return Ok(None);
        }

        let total_len = u16::from_be_bytes([first_decoded[0], first_decoded[1]]) as usize;
        let mut assembled = Vec::with_capacity(total_len);
        assembled.extend_from_slice(&first_decoded[2..]);

        // Read continuation chunks
        while assembled.len() < total_len {
            let Some(cont_start) = find_header_at(buf, pos, &CONTINUATION_HEADER) else {
                return Ok(None);
            };
            pos = cont_start + 2;

            let Some(b64_end) = find_newline(buf, pos) else {
                return Ok(None);
            };
            let b64_data = &buf[pos..b64_end];
            pos = b64_end + 1;

            let decoded = BASE64_STANDARD
                .decode(b64_data)
                .map_err(|_| FrameError::Base64)?;
            assembled.extend_from_slice(&decoded);
        }

        // Verify: last 2 bytes are CRC
        if assembled.len() < 2 {
            return Err(FrameError::TooShort);
        }

        let (data, checksum_bytes) = assembled.split_at(assembled.len() - 2);
        let expected_crc = u16::from_be_bytes([checksum_bytes[0], checksum_bytes[1]]);
        let actual_crc = self.crc_algo.checksum(data);

        if expected_crc != actual_crc {
            return Err(FrameError::BadCrc);
        }

        Ok(Some((data.to_vec(), pos)))
    }
}

#[derive(Debug)]
pub enum FrameError {
    Base64,
    BadCrc,
    TooShort,
}

impl core::fmt::Display for FrameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Base64 => write!(f, "base64 decode error"),
            Self::BadCrc => write!(f, "CRC mismatch"),
            Self::TooShort => write!(f, "frame too short"),
        }
    }
}

fn find_header(buf: &[u8], header: &[u8; 2]) -> Option<usize> {
    buf.windows(2).position(|w| w == header)
}

fn find_header_at(buf: &[u8], from: usize, header: &[u8; 2]) -> Option<usize> {
    if from + 1 >= buf.len() {
        return None;
    }
    buf[from..]
        .windows(2)
        .position(|w| w == header)
        .map(|p| p + from)
}

fn find_newline(buf: &[u8], from: usize) -> Option<usize> {
    buf[from..]
        .iter()
        .position(|&b| b == 0x0a)
        .map(|p| p + from)
}
