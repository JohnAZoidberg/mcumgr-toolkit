use serde::{Deserialize, Serialize};

use super::{cbor_decode, cbor_encode, check_smp_error};
use crate::transport::{TransportError, WebSerialTransport};

/// OS group ID
const GROUP_OS: u16 = 0;
/// Echo command ID
const CMD_ECHO: u8 = 0;

#[derive(Serialize)]
struct EchoRequest<'a> {
    d: &'a str,
}

#[derive(Deserialize)]
struct EchoResponse {
    r: String,
}

pub async fn echo(transport: &mut WebSerialTransport, msg: &str) -> Result<String, TransportError> {
    let payload = cbor_encode(&EchoRequest { d: msg }).map_err(TransportError::Js)?;
    let response_data = transport
        .transceive(false, GROUP_OS, CMD_ECHO, &payload)
        .await?;
    check_smp_error(&response_data).map_err(TransportError::Js)?;
    let response: EchoResponse = cbor_decode(&response_data).map_err(TransportError::Js)?;
    Ok(response.r)
}
