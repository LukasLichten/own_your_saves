

// Special type for handling a Sha3_224 hash, with an extra byte in the front to handle inequality
pub const NUM_BYTES:usize = 29;

#[derive(PartialEq, Clone, Copy, Eq, Hash)]
pub struct U232 {
    bytes: [u8; 29],
}

pub fn new() -> U232{
    U232 {
        bytes: [0_u8; 29]
    }
}

pub fn from_u8arr(bytes: &[u8]) -> U232 {
    let mut value = new();

    let count = if bytes.len() > NUM_BYTES { NUM_BYTES } else { bytes.len() };
    let mut i = 0;

    while i < count {
        value.bytes[(NUM_BYTES - count) + i] = bytes[i];
        i = i + 1;
    }

    value
}

impl U232 {
    pub fn to_be_bytes(& self) -> &[u8] {
        &self.bytes
    }

    pub fn set_inequailty_byte(&mut self, byte:u8) {
        self.bytes[0] = byte;
    }
}

impl std::fmt::Display for U232 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", super::io::bytes_to_hex_string(&self.bytes))
    }
}