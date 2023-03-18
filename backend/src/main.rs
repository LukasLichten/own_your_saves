pub mod file_processing;
pub mod api;
pub mod database;

use api::task;

use actix_web::{HttpServer, App, web::{Data, scope}, middleware::Logger};
use actix_web_lab::web::spa;

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    std::env::set_var("RUST_LOG", "debug");
    std::env::set_var("RUST_BACKTRACE", "1");
    
    env_logger::init();

    HttpServer::new(move || {
        let logger = Logger::default();
        let database = database::init_sql();
        let data = Data::new(database);

        App::new()
        .wrap(logger)
        .app_data(data)
        .service(
            scope("/api")
                .service(task::get_ping)
                .service(task::create_new_user)
                .service(task::login)
                .service(task::auth)
                .service(task::get_user)
                .service(task::get_device)
                .service(task::create_device)
                .service(task::delete_device)
                .service(task::get_test)

        )
        //Production
        .service(
            spa()
            .index_file("./dist/index.html")
            .static_resources_mount("/")
            .static_resources_location("./dist")
            .finish()
        )
        
    })
    .bind(("0.0.0.0", 80))?
    .run()
    .await
}




// let test_file_tar = Path::new("planning/sav/tar/Y.sav");
    // let test_folder = Path::new("planning/sav/tar/");

    // //let res = file_processing::reconstruction::new_repo(Path::new("planning/sav/repo/"), "Test".to_string());
    // let res = file_processing::reconstruction::read_storage_info(Path::new("planning/sav/repo/"));
    // if let Ok(mut repo) = res {
    //     let test_file_loc = Path::new("planning/sav/Y.sav");

    //     if let Some(bra) = repo.get_branch("master".to_string()) {
    //         let res = repo.create_commit(bra.get_previous_commit(), test_file_loc);
    //         if let Some(data) = res {
    //             //data.write_file_back(Path::new(&repo.folder));
    //             repo.push_commit_onto_branch(&data, "master".to_string());

    //             let list = repo.get_branches();
    //             println!("{}",list.len());

    //             for item in list {
    //                 println!("{}", item.get_name());
    //             }

    //             repo.build_commit(u232::from_u8arr(file_processing::io::hex_string_to_bytes(&data.get_name()).as_slice()), test_folder);

    //             //let og_hash = file_processing::io::hash_file(test_file_tar);
            
    //         } else  {
    //             panic!("Ahhhh");
    //         }


            
    //     }

        
    // } else if let Err(e) = res {
    //     panic!("{}", e.to_string());
    // }
    


    //Testing code
    
    //let test_file_loc = "planning/sav/20221004-Beaten/main".to_string();


// if let Ok(data)  = file_processing::io::read_bytes(test_file_loc) {
    //     let og_hash = file_processing::io::hash_data(&data);

    //     map.insert(og_hash.clone(), "Text".to_string());
    //     //let mut data = data;
    //     //data[4378] = data[4378].wrapping_sub(1); //Bit flip simulation

    //     if file_processing::io::write_bytes(test_file_tar.clone(), data).is_ok() {
    //         if let Ok(data) = file_processing::io::read_bytes(test_file_tar) {
    //             let new_hash = file_processing::io::hash_data(&data);

    //             println!("{}", og_hash);
    //             let h = file_processing::io::bytes_to_hex_string(&new_hash.to_be_bytes()).to_lowercase();
    //             let hh = file_processing::u232::from_u8arr(file_processing::io::hex_string_to_bytes(&h).as_slice());
    //             println!("{}\n{}\n{}", h, hh, og_hash == hh);

    //             println!("{}",map[&hh]);
    //         }
    //     }
        
    // }
