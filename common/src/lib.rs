// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }

pub mod data;

use serde::{Deserialize, Serialize};
use sha3::Digest;



pub trait LargeU<T> {
    const NUM_OF_BYTES: usize;

    fn new() -> T;

    fn from_u8arr(bytes: &[u8]) -> T where T: LargeU<T> {
        let mut value = Self::new();
    
        let count = if bytes.len() > Self::NUM_OF_BYTES { Self::NUM_OF_BYTES } else { bytes.len() };
        let mut i = 0;
    
        while i < count {
            value.set_byte((Self::NUM_OF_BYTES - count) + i, bytes[i]);
            i = i + 1;
        }
    
        value
    }

    fn set_byte(&mut self, pos: usize, val: u8);

    fn to_be_bytes(& self) -> &[u8];
}

#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash, Serialize, Deserialize)]
pub struct U256 {
    bytes: [u8; 32]
}

impl LargeU<U256> for U256 {
    const NUM_OF_BYTES: usize = 32;

    fn new() -> U256{
        U256 {
            bytes: [0_u8; Self::NUM_OF_BYTES]
        }
    }

    fn to_be_bytes(& self) -> &[u8] {
        &self.bytes
    }

    fn set_byte(&mut self, pos: usize, val: u8) {
        self.bytes[pos] = val;
    }
}

impl std::fmt::Display for U256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", bytes_to_hex_string(&self.bytes))
    }
}

// Special type for handling a Sha3_224 hash, with an extra byte in the front to handle inequality
#[derive(Debug, PartialEq, Clone, Copy, Eq, Hash, Serialize, Deserialize)]
pub struct U232 {
    bytes: [u8; 29],
}

impl U232 {
    pub fn set_inequailty_byte(&mut self, byte:u8) {
        self.set_byte(0, byte);
    }

    pub fn equal_224(& self, other: &Self) -> bool {
        for i in 1..Self::NUM_OF_BYTES {
            if self.bytes[i] != other.bytes[i] {
                return false;
            }
        }

        true
    }
}

#[test]
fn test_equal_224() {
    let base = U232::new(); 
    assert!(base.equal_224(&base));
    assert!(base == base);

    let mut comp = base.clone();
    comp.set_inequailty_byte(0x05);
    assert!(base.equal_224(&comp));
    assert!(base != comp);
}

impl LargeU<U232> for U232 {
    const NUM_OF_BYTES: usize = 29;

    fn new() -> U232{
        U232 {
            bytes: [0_u8; Self::NUM_OF_BYTES]
        }
    }

    fn to_be_bytes(& self) -> &[u8] {
        &self.bytes
    }

    fn set_byte(&mut self, pos: usize, val: u8) {
        self.bytes[pos] = val;
    }
}

impl std::fmt::Display for U232 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", bytes_to_hex_string(&self.bytes))
    }
}

pub fn bytes_to_hex_string(data: &[u8]) -> String {
    fn convert(val: u8) -> char {
        match val {
            0x0 => '0',
            0x1 => '1',
            0x2 => '2',
            0x3 => '3',
            0x4 => '4',
            0x5 => '5',
            0x6 => '6',
            0x7 => '7',
            0x8 => '8',
            0x9 => '9',
            0xA => 'A',
            0xB => 'B',
            0xC => 'C',
            0xD => 'D',
            0xE => 'E',
            0xF => 'F',
            _ => ' '
        }
    }

    let mut out = String::new();

    let mut iter = data.iter();
    while let Some(byte) = iter.next() {
        let lower = byte.clone().wrapping_shl(4).wrapping_shr(4); //Shifting to truncate the lower data
        let upper = byte.clone().wrapping_shr(4); //Just needs to shift right

        out.push(convert(upper));
        out.push(convert(lower));
    }

    out
}

pub fn hex_string_to_bytes(text: &String) -> Vec<u8> {
    fn convert(val: char) -> u8 {
        match val {
            '0' => 0x0,
            '1' => 0x1,
            '2' => 0x2,
            '3' => 0x3,
            '4' => 0x4,
            '5' => 0x5,
            '6' => 0x6,
            '7' => 0x7,
            '8' => 0x8,
            '9' => 0x9,
            'A' => 0xA,
            'B' => 0xB,
            'C' => 0xC,
            'D' => 0xD,
            'E' => 0xE,
            'F' => 0xF,
            _ => 0
        }
    }

    let mut out = Vec::<u8>::new();

    let text = text.to_ascii_uppercase(); //deals with the potential of lower case characters

    let mut iter = text.chars().into_iter();
    let mut temp = Option::<u8>::None;
    while let Some(ch) = iter.next() {
        let val = convert(ch);

        if let Some(store) = temp {
            out.push(store + val);
            temp = Option::None;
        } else {
            temp = Some(val.wrapping_shl(4));
        }
    }


    out
}

pub fn hash_data(list_of_bytes: &[u8]) -> U232{
    let mut hasher = sha3::Sha3_224::new(); // Sha3_256::new();

    hasher.update(list_of_bytes);
    
    let res = hasher.finalize();
    
    U232::from_u8arr(res.as_slice())
}