use wasm_bindgen::prelude::*;

mod commands;
mod framing;
mod smp;
mod transport;

use transport::WebSerialTransport;

#[wasm_bindgen]
#[derive(Default)]
pub struct McuMgrWeb {
    transport: Option<WebSerialTransport>,
}

#[wasm_bindgen]
impl McuMgrWeb {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Connect to a serial device. Triggers the browser's port selection dialog.
    pub async fn connect(&mut self, baud_rate: u32) -> Result<(), JsValue> {
        let transport = WebSerialTransport::connect(baud_rate).await?;
        self.transport = Some(transport);
        Ok(())
    }

    /// Disconnect from the serial device.
    pub async fn disconnect(&mut self) -> Result<(), JsValue> {
        if let Some(transport) = self.transport.take() {
            transport.disconnect().await?;
        }
        Ok(())
    }

    /// Send an echo command, returns the echoed string.
    pub async fn echo(&mut self, msg: &str) -> Result<String, JsValue> {
        let transport = self.transport.as_mut().ok_or("Not connected")?;
        commands::echo::echo(transport, msg)
            .await
            .map_err(Into::into)
    }

    /// List images on the device, returns JSON string.
    pub async fn image_list(&mut self) -> Result<String, JsValue> {
        let transport = self.transport.as_mut().ok_or("Not connected")?;
        let images = commands::image::image_list(transport)
            .await
            .map_err(JsValue::from)?;
        serde_json::to_string_pretty(&images).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Upload a firmware image. `data` is a Uint8Array, `progress` is a callback(offset, total).
    pub async fn image_upload(
        &mut self,
        data: &[u8],
        image: Option<u32>,
        progress: &js_sys::Function,
    ) -> Result<(), JsValue> {
        let transport = self.transport.as_mut().ok_or("Not connected")?;
        commands::image::image_upload(transport, data, image, progress)
            .await
            .map_err(Into::into)
    }

    /// Reset the device. `boot_mode` is optional: 0 = normal boot, 1 = bootloader recovery.
    pub async fn reset(&mut self, boot_mode: Option<u8>) -> Result<(), JsValue> {
        let transport = self.transport.as_mut().ok_or("Not connected")?;
        commands::reset::reset(transport, boot_mode)
            .await
            .map_err(Into::into)
    }
}
