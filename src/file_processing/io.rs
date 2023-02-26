use std::io;
use std::io::prelude::*;
use std::fs::File;
use sha3::{Digest, Sha3_256};

// If this is compiled in 32bit then we would be restricted to 2gb files
pub fn read_bytes(file_name: String) -> io::Result<Vec<u8>> {
    let mut list = Vec::<u8>::new(); 

    let res = File::open(file_name);
    if let Ok(mut f) = res {
        let res = f.read_to_end(&mut list);

        if let Ok(len) = res {
            if let Ok(meta) =  f.metadata() {
                let len:u64 = len.try_into().unwrap_or_default();

                if len == meta.len() { //Only if the full file got read we may let him exit
                    return io::Result::Ok(list);
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

pub fn write_bytes(file_name: String, list_of_bytes: Vec<u8>) -> io::Result<()> {
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

pub fn hash_data(list_of_bytes: &Vec<u8>) -> u32{
    let mut hasher = Sha3_256::new();

    hasher.update(list_of_bytes.as_slice());
    
    let res = hasher.finalize();
    get_u32(res.as_slice())
}

//reverse is easily done with value.to_be_bytes();
//Should maybe be optimized
//Also maybe moved out of this module
pub fn get_u32 (data: &[u8]) -> u32 {
    let mut value = 0;

    let mut counter:u32 = if data.len() > 4 { 4 } else { data.len() }.try_into().unwrap_or_default();

    let mut iter = data.iter();

    while let Some(sample) = iter.next() {
        counter = counter - 1;
        let multi:u32 = 2_u32.pow(8 * counter + 0);
        let v:u32 = sample.clone().try_into().unwrap_or_default();
        value = value + multi * v;

        if counter == 0 {
            // more then 4 bytes given, we clamp
            return value;
        }
    }

    value
}