use js_sys::{Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::framing::{FrameError, SerialFramer};
use crate::smp::{self, SMP_HEADER_SIZE, SmpHeader};

pub struct WebSerialTransport {
    port: JsValue,
    reader: JsValue,
    writer: JsValue,
    framer: SerialFramer,
    read_buf: Vec<u8>,
    next_seqnum: u8,
}

#[derive(Debug)]
pub enum TransportError {
    Js(String),
    Frame(FrameError),
    UnexpectedResponse,
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Js(msg) => write!(f, "JS error: {msg}"),
            Self::Frame(e) => write!(f, "Frame error: {e}"),
            Self::UnexpectedResponse => write!(f, "Unexpected response"),
        }
    }
}

impl From<JsValue> for TransportError {
    fn from(val: JsValue) -> Self {
        Self::Js(format!("{val:?}"))
    }
}

impl From<FrameError> for TransportError {
    fn from(val: FrameError) -> Self {
        Self::Frame(val)
    }
}

impl From<TransportError> for JsValue {
    fn from(val: TransportError) -> Self {
        JsValue::from_str(&val.to_string())
    }
}

impl WebSerialTransport {
    /// Prompt user to select a serial port and open it.
    pub async fn connect(baud_rate: u32) -> Result<Self, TransportError> {
        let global = js_sys::global();
        let navigator = Reflect::get(&global, &"navigator".into())?;
        let serial = Reflect::get(&navigator, &"serial".into())?;

        // request_port() returns a Promise<SerialPort>
        let request_port = Reflect::get(&serial, &"requestPort".into())?;
        let request_port_fn: js_sys::Function = request_port.unchecked_into();
        let port = JsFuture::from(js_sys::Promise::from(request_port_fn.call0(&serial)?)).await?;

        // port.open({ baudRate })
        let options = Object::new();
        Reflect::set(&options, &"baudRate".into(), &baud_rate.into())?;
        let open_fn: js_sys::Function = Reflect::get(&port, &"open".into())?.unchecked_into();
        JsFuture::from(js_sys::Promise::from(open_fn.call1(&port, &options)?)).await?;

        // Get reader from port.readable.getReader()
        let readable = Reflect::get(&port, &"readable".into())?;
        let get_reader_fn: js_sys::Function =
            Reflect::get(&readable, &"getReader".into())?.unchecked_into();
        let reader = get_reader_fn.call0(&readable)?;

        // Get writer from port.writable.getWriter()
        let writable = Reflect::get(&port, &"writable".into())?;
        let get_writer_fn: js_sys::Function =
            Reflect::get(&writable, &"getWriter".into())?.unchecked_into();
        let writer = get_writer_fn.call0(&writable)?;

        Ok(Self {
            port,
            reader,
            writer,
            framer: SerialFramer::new(),
            read_buf: Vec::with_capacity(4096),
            next_seqnum: 0,
        })
    }

    /// Close the serial port.
    pub async fn disconnect(self) -> Result<(), TransportError> {
        // reader.releaseLock()
        let release_lock: js_sys::Function =
            Reflect::get(&self.reader, &"releaseLock".into())?.unchecked_into();
        let _ = release_lock.call0(&self.reader);

        // writer.releaseLock()
        let release_lock: js_sys::Function =
            Reflect::get(&self.writer, &"releaseLock".into())?.unchecked_into();
        let _ = release_lock.call0(&self.writer);

        // port.close()
        let close_fn: js_sys::Function =
            Reflect::get(&self.port, &"close".into())?.unchecked_into();
        JsFuture::from(js_sys::Promise::from(close_fn.call0(&self.port)?)).await?;

        Ok(())
    }

    /// Write bytes to the serial port.
    async fn write_bytes(&self, data: &[u8]) -> Result<(), TransportError> {
        let arr = Uint8Array::from(data);
        let write_fn: js_sys::Function =
            Reflect::get(&self.writer, &"write".into())?.unchecked_into();
        JsFuture::from(js_sys::Promise::from(write_fn.call1(&self.writer, &arr)?)).await?;
        Ok(())
    }

    /// Read a chunk of bytes from the serial port.
    async fn read_chunk(&mut self) -> Result<(), TransportError> {
        let read_fn: js_sys::Function =
            Reflect::get(&self.reader, &"read".into())?.unchecked_into();
        let result = JsFuture::from(js_sys::Promise::from(read_fn.call0(&self.reader)?)).await?;

        let done = Reflect::get(&result, &"done".into())?
            .as_bool()
            .unwrap_or(true);
        if done {
            return Err(TransportError::Js("Stream closed".into()));
        }

        let value = Reflect::get(&result, &"value".into())?;
        let arr: Uint8Array = value.unchecked_into();
        let mut chunk = vec![0u8; arr.length() as usize];
        arr.copy_to(&mut chunk);
        self.read_buf.extend_from_slice(&chunk);
        Ok(())
    }

    /// Send data and receive response for an SMP command.
    pub async fn transceive(
        &mut self,
        write_operation: bool,
        group_id: u16,
        command_id: u8,
        cbor_payload: &[u8],
    ) -> Result<Vec<u8>, TransportError> {
        let sequence_num = self.next_seqnum;
        self.next_seqnum = self.next_seqnum.wrapping_add(1);

        let header = SmpHeader {
            ver: 0b01,
            op: if write_operation {
                smp::op::WRITE
            } else {
                smp::op::READ
            },
            flags: 0,
            data_length: cbor_payload.len() as u16,
            group_id,
            sequence_num,
            command_id,
        };

        // Encode and send
        let lines = self.framer.encode_frame(header.to_bytes(), cbor_payload);
        for line in &lines {
            self.write_bytes(line).await?;
        }

        // Read until we have a complete frame
        loop {
            match self.framer.decode_frame(&self.read_buf)? {
                Some((data, consumed)) => {
                    self.read_buf.drain(..consumed);

                    if data.len() < SMP_HEADER_SIZE {
                        return Err(TransportError::UnexpectedResponse);
                    }

                    let resp_header =
                        SmpHeader::from_bytes(data[..SMP_HEADER_SIZE].try_into().unwrap());

                    let expected_op = if write_operation {
                        smp::op::WRITE_RSP
                    } else {
                        smp::op::READ_RSP
                    };

                    if resp_header.sequence_num != sequence_num {
                        continue;
                    }

                    if resp_header.op != expected_op
                        || resp_header.group_id != group_id
                        || resp_header.command_id != command_id
                    {
                        return Err(TransportError::UnexpectedResponse);
                    }

                    return Ok(data[SMP_HEADER_SIZE..].to_vec());
                }
                None => {
                    self.read_chunk().await?;
                }
            }
        }
    }
}
