use std::{path::Path, collections::HashMap};

pub mod file_processing;

fn main() {
    let num:u64 = 40052457;
    let res = file_processing::io::value_to_utf8_bytes(num);
    let (num2, b) = file_processing::io::get_utf8_value(res.as_slice());
    println!("{}\n{}\t{}", num, num2, b);

    let mut map = HashMap::<file_processing::u232::U232, String>::new();

    //Testing code
    let test_file_loc = Path::new("planning/sav/Y.sav");
    //let test_file_loc = "planning/sav/20221004-Beaten/main".to_string();
    let test_file_tar = Path::new("planning/sav/tar.sav");
    if let Ok(data)  = file_processing::io::read_bytes(test_file_loc) {
        let og_hash = file_processing::io::hash_data(&data);

        map.insert(og_hash.clone(), "Text".to_string());
        //let mut data = data;
        //data[4378] = data[4378].wrapping_sub(1); //Bit flip simulation

        if file_processing::io::write_bytes(test_file_tar.clone(), data).is_ok() {
            if let Ok(data) = file_processing::io::read_bytes(test_file_tar) {
                let new_hash = file_processing::io::hash_data(&data);

                println!("{}", og_hash);
                let h = file_processing::io::bytes_to_hex_string(&new_hash.to_be_bytes()).to_lowercase();
                let hh = file_processing::u232::from_u8arr(file_processing::io::hex_string_to_bytes(&h).as_slice());
                println!("{}\n{}\n{}", h, hh, og_hash == hh);

                println!("{}",map[&hh]);
            }
        }
        
    }
}
