pub mod file_processing;

fn main() {

    //Testing code
    let test_file_loc = "planning/sav/Y.sav".to_string();
    //let test_file_loc = "planning/sav/20221004-Beaten/main".to_string();
    let test_file_tar = "planning/sav/tar.sav".to_string();
    if let Ok(data)  = file_processing::io::read_bytes(test_file_loc) {
        let og_hash = file_processing::io::hash_data(&data);

        if file_processing::io::write_bytes(test_file_tar.clone(), data).is_ok() {
            if let Ok(data) = file_processing::io::read_bytes(test_file_tar) {
                let new_hash = file_processing::io::hash_data(&data);

                println!("{}\n{}", og_hash, new_hash);
            }
        }
        
    }
}
