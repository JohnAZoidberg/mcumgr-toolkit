pub mod echo;
pub mod image;
pub mod os_info;
pub mod reset;

use std::io::Cursor;

use serde::Deserialize;

/// SMP error response (v1)
#[derive(Clone, Debug, Deserialize)]
pub struct ErrResponse {
    pub rc: Option<i32>,
    pub rsn: Option<String>,
    pub err: Option<ErrResponseV2>,
}

/// SMP error response (v2)
#[derive(Clone, Debug, Deserialize)]
pub struct ErrResponseV2 {
    pub group: u16,
    pub rc: i32,
}

pub fn cbor_encode<T: serde::Serialize>(val: &T) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    ciborium::into_writer(val, &mut buf).map_err(|e| format!("CBOR encode error: {e}"))?;
    Ok(buf)
}

pub fn cbor_decode<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, String> {
    ciborium::from_reader(Cursor::new(data)).map_err(|e| format!("CBOR decode error: {e}"))
}

/// Check an SMP response for error codes before decoding the actual response.
pub fn check_smp_error(data: &[u8]) -> Result<(), String> {
    let err: ErrResponse = cbor_decode(data)?;

    if let Some(ErrResponseV2 { group, rc }) = err.err {
        return Err(format!("Device error (v2): group={group}, rc={rc}"));
    }

    if let Some(rc) = err.rc
        && rc != 0
    {
        let msg = err.rsn.unwrap_or_default();
        return Err(format!("Device error (v1): rc={rc} {msg}"));
    }

    Ok(())
}
