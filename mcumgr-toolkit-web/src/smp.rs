pub const SMP_HEADER_SIZE: usize = 8;

pub mod op {
    pub const READ: u8 = 0;
    pub const READ_RSP: u8 = 1;
    pub const WRITE: u8 = 2;
    pub const WRITE_RSP: u8 = 3;
}

pub struct SmpHeader {
    pub ver: u8,
    pub op: u8,
    pub flags: u8,
    pub data_length: u16,
    pub group_id: u16,
    pub sequence_num: u8,
    pub command_id: u8,
}

impl SmpHeader {
    pub fn from_bytes(data: [u8; SMP_HEADER_SIZE]) -> Self {
        Self {
            ver: (data[0] >> 3) & 0b11,
            op: data[0] & 0b111,
            flags: data[1],
            data_length: u16::from_be_bytes([data[2], data[3]]),
            group_id: u16::from_be_bytes([data[4], data[5]]),
            sequence_num: data[6],
            command_id: data[7],
        }
    }

    pub fn to_bytes(&self) -> [u8; SMP_HEADER_SIZE] {
        let [length_0, length_1] = self.data_length.to_be_bytes();
        let [group_id_0, group_id_1] = self.group_id.to_be_bytes();
        [
            ((self.ver & 0b11) << 3) | (self.op & 0b111),
            self.flags,
            length_0,
            length_1,
            group_id_0,
            group_id_1,
            self.sequence_num,
            self.command_id,
        ]
    }
}
