use std::{io, path::Path};
use std::io::prelude::*;
use std::fs::File;
use sha3::{Digest, Sha3_256};

// If this is compiled in 32bit then we would be restricted to 2gb files
pub fn read_bytes(file_name: &Path) -> io::Result<Vec<u8>> {
    let mut list = Vec::<u8>::new(); 

    let res = File::open(file_name.clone());
    if let Ok(mut f) = res {
        let res = f.read_to_end(&mut list);

        if let Ok(len) = res {
            if let Ok(meta) =  f.metadata() {
                let len:u64 = len.try_into().unwrap_or_default();

                if len == meta.len() { //Only if the full file got read we may let him exit
                    return io::Result::Ok(list);
                } else {
                    return io::Result::Err(io::Error::new(io::ErrorKind::InvalidData, format!("Lenth of Data loaded from file {} does not match file length", file_name.to_str().unwrap())));
                }
            }
        } else if let Err(e) = res {
            return io::Result::Err(e);
        }
    } else if let Err(e) = res {
        return io::Result::Err(e);
    }

    io::Result::Err(io::Error::new(io::ErrorKind::Other, "Failed to read File"))
}

pub fn write_bytes(file_name: &Path, list_of_bytes: Vec<u8>) -> io::Result<()> {
    let res = File::options().write(true).create(true).open(file_name);
    if let Ok(mut f) = res {
        let res = f.set_len(list_of_bytes.len().try_into().unwrap()); 
        
        if res.is_ok() {
            return f.write_all(list_of_bytes.as_slice());
        } else if let Err(e) = res {
            return io::Result::Err(e);
        }
    } else if let Err(e) = res {
        return io::Result::Err(e);
    }

    io::Result::Err(io::Error::new(io::ErrorKind::Other, "Failed to write File"))
    
}

pub fn hash_data(list_of_bytes: &[u8]) -> u32{
    let mut hasher = Sha3_256::new();

    hasher.update(list_of_bytes);
    
    let res = hasher.finalize();
    get_u32(res.as_slice())
}

pub fn hash_file(file_name: &Path) -> io::Result<u32> {
    let res = read_bytes(file_name);

    if let Ok(bytes) = res {
        return Ok(hash_data(&bytes[..]));
    }

    Err(res.unwrap_err())
}

// reverse is easily done with value.to_be_bytes();
// This deals with the issue that from_be_bytes requires a u8,4 array, and can not handle a simple &u8 pointer 
// This maybe moved out of this module
pub fn get_u32 (data: &[u8]) -> u32 {
    let c = if data.len() > 4 { 4 } else { data.len() };
    let mut i = 0;
    let mut bytes: [u8; 4] = [0,0,0,0];

    while i < c {
        bytes[(4-c) + i] = data[i];
        i = i + 1;
    }

    u32::from_be_bytes(bytes)
}

pub fn get_u64 (data: &[u8]) -> u64 {
    let c = if data.len() > 8 { 8 } else { data.len() };
    let mut i = 0;
    let mut bytes: [u8; 8] = [0,0,0,0,0,0,0,0];

    while i < c {
        bytes[(8-c) +i] = data[i];
        i = i + 1;
    }

    u64::from_be_bytes(bytes)
}

// We use utf8 format to store numbers in scalable but compact ways
pub fn get_utf8_value (data: &[u8]) -> (u64, usize) {
    let number_of_bytes:usize = data[0].leading_ones().try_into().unwrap_or_default();

    if number_of_bytes > data.len() {
        return (0,0)
    }

    //We output single bytes directly
    if number_of_bytes == 0 {
        return (data[0].try_into().unwrap_or_default(), 1);
    }

    let divider = match number_of_bytes {
        2 => 0b1100_0000u8,
        3 => 0b1110_0000u8,
        4 => 0b1111_0000u8,
        5 => 0b1111_1000u8,
        6 => 0b1111_1100u8,
        7 => 0b1111_1110u8,
        8 => 0b1111_1111u8,
        _ => 0u8
    };

    
    let mut value:u64 = (data[0] % divider).try_into().unwrap_or_default();
    
    let mut index = 1;
    while index < number_of_bytes {
        let b:u64 = (data[index] % 0b1000_0000).try_into().unwrap_or_default();
        value = value << 6;
        value = value + b;

        index = index + 1;
    }

    (value,number_of_bytes)
}

pub fn value_to_utf8_bytes(number: u64) -> Vec<u8> {
    fn generate_head(bytes:&[u8;8], number_of_bytes: usize) -> u8 {
        let mask = match number_of_bytes {
            2 => 0b1100_0000u8,
            3 => 0b1110_0000u8,
            4 => 0b1111_0000u8,
            5 => 0b1111_1000u8,
            6 => 0b1111_1100u8,
            7 => 0b1111_1110u8,
            8 => 0b1111_1111u8,
            _ => 0u8
        };

        let offset:usize = (number_of_bytes - 1) * 6;
        let offset_byte:usize = offset / 8;

        let lower_byte:u8 = bytes[7 - offset_byte].clone() >> (offset % 8);
        let upper_byte:u8 = bytes[7 - offset_byte - 1].clone().wrapping_shl((8 - (offset % 8) - 1).try_into().unwrap_or_default()).wrapping_shl(1);

        //clamping the value
        let shift:u32 = (number_of_bytes + 1).try_into().unwrap_or_default();
        let value:u8 = (lower_byte + upper_byte).wrapping_shl(shift); 
        let value:u8 = value.wrapping_shr(shift);

        value + mask
    }

    fn generate_append_byte(bytes:&[u8;8], pos: usize) -> u8 {
        let offset:usize = pos * 6;
        let offset_byte:usize = offset / 8;

        let lower_byte:u8 = bytes[7 - offset_byte].clone() >> (offset % 8);
        let upper_byte = bytes[7 - offset_byte - 1].clone().wrapping_shl((8 - (offset % 8) - 1).try_into().unwrap_or_default()).wrapping_shl(1);

        ((upper_byte + lower_byte) % 0b0100_0000) + 0b1000_0000
    }

    let mut data = Vec::<u8>::new();

    //Simple numbers are processed like this
    if number < 128 {
        data.push(number.to_be_bytes()[7]);
        return data;
    }

    let number_of_bits:usize = number.ilog2().try_into().unwrap_or_default(); // requires rust v1.67.1

    //Threashold for 3 bytes is 11 bits, every 5 bits a new byte
    let mut bytes_in_tar:usize = (number_of_bits.saturating_sub(6) / 5) + 2;
    if bytes_in_tar > 8 { // We have to clamp at 8 bytes
        bytes_in_tar = 8;
    }

    let num_as_bytes = number.to_be_bytes();

    data.push(generate_head(&num_as_bytes, bytes_in_tar));
    
    //Adding the main data bytes
    bytes_in_tar = bytes_in_tar - 1;
    while bytes_in_tar > 0 {
        bytes_in_tar = bytes_in_tar - 1;
        data.push(generate_append_byte(&num_as_bytes, bytes_in_tar));
    }

    data
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

pub fn read_string_sequence(data: &[u8]) -> (String,usize) {
    let mut index = 0;
    while index < data.len() && data[index] != 0_u8 {
        index = index + 1;
    }

    
    if index != 0 {
        if let Ok(val) = String::from_utf8(data[..index].to_vec()) {
            return (val, index + 1);
        }
    }
    
    (String::new(),index + 1)
}

// used to prevent panics from overflows
pub fn save_slice(data: &[u8], offset: usize) -> &[u8] {
    if offset < data.len() {
        return &data[offset..];
    } else {
        return &[0_u8];
    }
}

pub fn save_cut(data: &[u8], size: usize) -> &[u8] {
    let mut size = size;
    if size < data.len() {
        size = data.len();
    }

    return &data[..size];
}

pub fn u64_to_usize(val: u64) -> usize {
    val.try_into().unwrap_or_default() //this might cause problems for 32bit, but f them
}

