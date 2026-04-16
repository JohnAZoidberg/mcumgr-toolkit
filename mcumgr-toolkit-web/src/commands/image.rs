use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{cbor_decode, cbor_encode, check_smp_error};
use crate::transport::{TransportError, WebSerialTransport};

/// Image management group ID
const GROUP_IMAGE: u16 = 1;
/// Image state command ID
const CMD_IMAGE_STATE: u8 = 0;
/// Image upload command ID
const CMD_IMAGE_UPLOAD: u8 = 1;

/// Default SMP frame size (matches Zephyr's default)
const DEFAULT_SMP_FRAME_SIZE: usize = 384;
const MGMT_HDR_SIZE: usize = 8;

// --- Image State ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageState {
    #[serde(default)]
    pub image: u32,
    pub slot: u32,
    pub version: String,
    #[serde(default)]
    pub bootable: bool,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub permanent: bool,
}

#[derive(Deserialize)]
struct ImageStateResponse {
    images: Vec<ImageState>,
}

pub async fn image_list(
    transport: &mut WebSerialTransport,
) -> Result<Vec<ImageState>, TransportError> {
    // Empty map payload for read command
    let payload =
        cbor_encode(&std::collections::HashMap::<String, u8>::new()).map_err(TransportError::Js)?;
    let response_data = transport
        .transceive(false, GROUP_IMAGE, CMD_IMAGE_STATE, &payload)
        .await?;
    check_smp_error(&response_data).map_err(TransportError::Js)?;
    let response: ImageStateResponse = cbor_decode(&response_data).map_err(TransportError::Js)?;
    Ok(response.images)
}

// --- Image Upload ---

#[derive(Serialize)]
struct ImageUploadFirst<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<u32>,
    len: u64,
    off: u64,
    #[serde(with = "serde_bytes")]
    sha: &'a [u8],
    #[serde(with = "serde_bytes")]
    data: &'a [u8],
}

#[derive(Serialize)]
struct ImageUploadChunk<'a> {
    off: u64,
    #[serde(with = "serde_bytes")]
    data: &'a [u8],
}

#[derive(Deserialize)]
struct ImageUploadResponse {
    off: u64,
}

/// Compute max data chunk size for image upload, same algorithm as the native crate.
fn image_upload_max_data_chunk_size(smp_frame_size: usize) -> Result<usize, String> {
    // Serialize a maximally-sized first chunk to measure overhead
    let mut counter = CountingWriter(0);
    ciborium::into_writer(
        &ImageUploadFirst {
            off: u64::MAX,
            data: &[0u8],
            len: u64::MAX,
            image: Some(u32::MAX),
            sha: &[42u8; 32],
        },
        &mut counter,
    )
    .map_err(|e| format!("size computation error: {e}"))?;

    let size_with_one_byte = counter.0;
    let size_without_data = size_with_one_byte - 1;

    let estimated_data_size = smp_frame_size
        .checked_sub(MGMT_HDR_SIZE)
        .and_then(|s| s.checked_sub(size_without_data))
        .ok_or_else(|| "SMP frame size too small".to_string())?;

    if estimated_data_size == 0 {
        return Err("SMP frame size too small".into());
    }

    let data_length_bytes = if estimated_data_size <= u8::MAX as usize {
        1
    } else if estimated_data_size <= u16::MAX as usize {
        2
    } else if estimated_data_size <= u32::MAX as usize {
        4
    } else {
        8
    };

    let actual_data_size = estimated_data_size
        .checked_sub(data_length_bytes)
        .ok_or_else(|| "SMP frame size too small".to_string())?;

    if actual_data_size == 0 {
        return Err("SMP frame size too small".into());
    }

    Ok(actual_data_size)
}

struct CountingWriter(usize);

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Upload a firmware image to the device.
///
/// `progress` is a JS function called with (offset, total) after each chunk.
pub async fn image_upload(
    transport: &mut WebSerialTransport,
    data: &[u8],
    image: Option<u32>,
    progress: &js_sys::Function,
) -> Result<(), TransportError> {
    let chunk_size_max =
        image_upload_max_data_chunk_size(DEFAULT_SMP_FRAME_SIZE).map_err(TransportError::Js)?;

    let sha: [u8; 32] = Sha256::digest(data).into();
    let size = data.len();
    let mut offset: usize = 0;

    while offset < size {
        let current_chunk_size = (size - offset).min(chunk_size_max);
        let chunk_data = &data[offset..offset + current_chunk_size];

        let payload = if offset == 0 {
            cbor_encode(&ImageUploadFirst {
                image,
                len: size as u64,
                off: 0,
                sha: &sha,
                data: chunk_data,
            })
        } else {
            cbor_encode(&ImageUploadChunk {
                off: offset as u64,
                data: chunk_data,
            })
        }
        .map_err(TransportError::Js)?;

        let response_data = transport
            .transceive(true, GROUP_IMAGE, CMD_IMAGE_UPLOAD, &payload)
            .await?;
        check_smp_error(&response_data).map_err(TransportError::Js)?;
        let response: ImageUploadResponse =
            cbor_decode(&response_data).map_err(TransportError::Js)?;

        offset = response.off as usize;

        // Call progress callback
        let _ = progress.call2(
            &wasm_bindgen::JsValue::NULL,
            &wasm_bindgen::JsValue::from_f64(offset as f64),
            &wasm_bindgen::JsValue::from_f64(size as f64),
        );
    }

    Ok(())
}
