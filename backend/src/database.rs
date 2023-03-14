use std::path::{Path, PathBuf};

use common::{U256, LargeU, RequestUser};
use rusqlite::{Connection};

use crate::file_processing;

pub fn init_sql() -> Connection {
    let mut path = std::env::var("DB_PATH").unwrap_or("./target/db/dat.db".to_string()); // TODO handle release, although this technically works as a default there too
    
    // We need to insure the folder exists
    let place = PathBuf::from(&path);
    let folder = if place.is_dir() {
        // Path does not include a db file, we need to update it
        let mut p = place.clone();
        p.push("dat.db");
        path = p.to_str().unwrap_or("dat.db").to_string(); 
        
        place.as_path()
    } else {
        if let Some(p) = place.parent() {
            p
        } else {
            Path::new("./")
        }
    };
    if let Err(e) = file_processing::io::create_folder(folder) {
        // Error handle
        panic!("Unable to create database folder at {}\nError: {}",folder.to_str().unwrap_or("*this is very broken, send help*"), e.to_string());
    }

    // Opening the DB
    let connection = match Connection::open(&path) {
        Ok(conn) => conn,
        Err(e) => panic!("Unable to load DB at {}:\nError{}",path, e.to_string())
    };
    
    // ,
    let res = connection.execute_batch(
                                "CREATE TABLE IF NOT EXISTS users(
                                        user_id INTEGER PRIMARY KEY,
                                        user_name TINYTEXT NOT NULL UNIQUE,
                                        password BINARY(32) NOT NULL
                                    );
                                    "
    );

    if let Err(e) = res {
        panic!("Unable to set up database: {}", e.to_string());
    }

    connection
}


pub fn create_user(conn: &Connection, name: String, password: U256) -> bool {
    let res = conn.execute("INSERT INTO users (user_name, password) VALUES (?1, ?2)", (name, password.to_be_bytes()));

    if let Ok(_c) = res {
        return true;
    } else if let Err(_e) = res {
        return false;
    }

    false
}

pub fn get_all_users(conn: &Connection) -> Vec<RequestUser> {
        
        let mut stmt = conn.prepare("SELECT user_id, user_name, password FROM users").unwrap();
        
        let user_iter = stmt.query_map([], |row| {
            let byte:[u8;32] = row.get(2)?;
            Ok(RequestUser::new(row.get(0)?, row.get(1)?, common::bytes_to_hex_string(&byte)))
        }).unwrap();

        let mut data = Vec::<RequestUser>::new();

        
        
        for person in user_iter {
            if let Ok(user) = person {
                data.push(user);
            } else if let Err(e) = person {
                println!("{}", e.to_string());
            }
        }

        data
}




// conn.execute(
//     "INSERT INTO person (name, data) VALUES (?1, ?2)",
//     (&me.name, &me.data),
// )?;

// let mut stmt = conn.prepare("SELECT id, name, data FROM person")?;
// let person_iter = stmt.query_map([], |row| {
//     Ok(Person {
//         id: row.get(0)?,
//         name: row.get(1)?,
//         data: row.get(2)?,
//     })
// })?;

// for person in person_iter {
//     println!("Found person {:?}", person.unwrap());
// }