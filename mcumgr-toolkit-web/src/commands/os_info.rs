use serde::{Deserialize, Serialize};

use super::{cbor_decode, cbor_encode, check_smp_error};
use crate::transport::{TransportError, WebSerialTransport};

const GROUP_OS: u16 = 0;
const CMD_APPLICATION_INFO: u8 = 7;
const CMD_BOOTLOADER_INFO: u8 = 8;

const FORMAT_FIELDS: &[(&str, &str)] = &[
    ("s", "kernel_name"),
    ("n", "node_name"),
    ("r", "kernel_release"),
    ("v", "kernel_version"),
    ("b", "build_time"),
    ("m", "machine"),
    ("p", "processor"),
    ("i", "hardware_platform"),
    ("o", "operating_system"),
];

#[derive(Serialize)]
struct ApplicationInfoRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a str>,
}

#[derive(Deserialize)]
struct ApplicationInfoResponse {
    output: String,
}

#[derive(Serialize, Default)]
pub struct ApplicationInfo {
    pub kernel_name: Option<String>,
    pub node_name: Option<String>,
    pub kernel_release: Option<String>,
    pub kernel_version: Option<String>,
    pub build_time: Option<String>,
    pub machine: Option<String>,
    pub processor: Option<String>,
    pub hardware_platform: Option<String>,
    pub operating_system: Option<String>,
}

async fn fetch_app_info_field(
    transport: &mut WebSerialTransport,
    format: &str,
) -> Result<String, TransportError> {
    let payload = cbor_encode(&ApplicationInfoRequest {
        format: Some(format),
    })
    .map_err(TransportError::Js)?;
    let data = transport
        .transceive(false, GROUP_OS, CMD_APPLICATION_INFO, &payload)
        .await?;
    check_smp_error(&data).map_err(TransportError::Js)?;
    let resp: ApplicationInfoResponse = cbor_decode(&data).map_err(TransportError::Js)?;
    Ok(resp.output)
}

pub async fn application_info(
    transport: &mut WebSerialTransport,
) -> Result<ApplicationInfo, TransportError> {
    let mut info = ApplicationInfo::default();

    for (format, field) in FORMAT_FIELDS {
        let value = match fetch_app_info_field(transport, format).await {
            Ok(v) => Some(v),
            // Build time is allowed to fail; everything else must succeed
            Err(_) if *format == "b" => None,
            Err(e) => return Err(e),
        };
        match *field {
            "kernel_name" => info.kernel_name = value,
            "node_name" => info.node_name = value,
            "kernel_release" => info.kernel_release = value,
            "kernel_version" => info.kernel_version = value,
            "build_time" => info.build_time = value,
            "machine" => info.machine = value,
            "processor" => info.processor = value,
            "hardware_platform" => info.hardware_platform = value,
            "operating_system" => info.operating_system = value,
            _ => unreachable!(),
        }
    }

    Ok(info)
}

#[derive(Serialize)]
struct BootloaderInfoRequest {}

#[derive(Deserialize)]
struct BootloaderInfoResponse {
    bootloader: String,
}

#[derive(Serialize)]
#[serde(tag = "query", rename = "mode")]
struct BootloaderInfoMcubootModeRequest {}

#[derive(Deserialize)]
struct BootloaderInfoMcubootModeResponse {
    mode: i32,
    #[serde(default, rename = "no-downgrade")]
    no_downgrade: bool,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum BootloaderInfo {
    #[serde(rename = "mcuboot")]
    MCUboot {
        name: &'static str,
        mode: i32,
        mode_name: Option<&'static str>,
        no_downgrade: bool,
    },
    #[serde(rename = "unknown")]
    Unknown { name: String },
}

fn mcuboot_mode_name(mode: i32) -> Option<&'static str> {
    match mode {
        0 => Some("MCUBOOT_MODE_SINGLE_SLOT"),
        1 => Some("MCUBOOT_MODE_SWAP_USING_SCRATCH"),
        2 => Some("MCUBOOT_MODE_UPGRADE_ONLY"),
        3 => Some("MCUBOOT_MODE_SWAP_USING_MOVE"),
        4 => Some("MCUBOOT_MODE_DIRECT_XIP"),
        5 => Some("MCUBOOT_MODE_DIRECT_XIP_WITH_REVERT"),
        6 => Some("MCUBOOT_MODE_RAM_LOAD"),
        7 => Some("MCUBOOT_MODE_FIRMWARE_LOADER"),
        8 => Some("MCUBOOT_MODE_SINGLE_SLOT_RAM_LOAD"),
        9 => Some("MCUBOOT_MODE_SWAP_USING_OFFSET"),
        _ => None,
    }
}

pub async fn bootloader_info(
    transport: &mut WebSerialTransport,
) -> Result<BootloaderInfo, TransportError> {
    let payload = cbor_encode(&BootloaderInfoRequest {}).map_err(TransportError::Js)?;
    let data = transport
        .transceive(false, GROUP_OS, CMD_BOOTLOADER_INFO, &payload)
        .await?;
    check_smp_error(&data).map_err(TransportError::Js)?;
    let resp: BootloaderInfoResponse = cbor_decode(&data).map_err(TransportError::Js)?;

    if resp.bootloader != "MCUboot" {
        return Ok(BootloaderInfo::Unknown {
            name: resp.bootloader,
        });
    }

    let payload =
        cbor_encode(&BootloaderInfoMcubootModeRequest {}).map_err(TransportError::Js)?;
    let data = transport
        .transceive(false, GROUP_OS, CMD_BOOTLOADER_INFO, &payload)
        .await?;
    check_smp_error(&data).map_err(TransportError::Js)?;
    let mode_resp: BootloaderInfoMcubootModeResponse =
        cbor_decode(&data).map_err(TransportError::Js)?;

    Ok(BootloaderInfo::MCUboot {
        name: "MCUboot",
        mode: mode_resp.mode,
        mode_name: mcuboot_mode_name(mode_resp.mode),
        no_downgrade: mode_resp.no_downgrade,
    })
}
