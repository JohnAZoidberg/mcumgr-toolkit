use serde::Serialize;

use super::{cbor_encode, check_smp_error};
use crate::transport::{TransportError, WebSerialTransport};

/// OS group ID
const GROUP_OS: u16 = 0;
/// System reset command ID
const CMD_RESET: u8 = 5;

#[derive(Serialize)]
struct SystemResetRequest {}

pub async fn reset(transport: &mut WebSerialTransport) -> Result<(), TransportError> {
    let payload = cbor_encode(&SystemResetRequest {}).map_err(TransportError::Js)?;

    // Device may reset before responding, so a timeout/error here is expected
    match transport
        .transceive(true, GROUP_OS, CMD_RESET, &payload)
        .await
    {
        Ok(data) => {
            let _ = check_smp_error(&data);
            Ok(())
        }
        Err(_) => {
            // Device likely reset before it could respond -- this is normal
            Ok(())
        }
    }
}
