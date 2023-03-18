use std::{path::{Path, PathBuf}, usize};

use common::{U256, LargeU, data::{RequestUser, Device, TokenCarrier, User}};
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::file_processing;

const SCHEMA_VERSION:usize = 0;

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
    
    // Generating the Schema
    fn error_handle<T>(res: Result<T,rusqlite::Error>) {
        if let Err(e) = res {
            panic!("Unable to set up database: {}", e.to_string());
        }
    }

    let res = connection.execute(
                            "CREATE TABLE IF NOT EXISTS keyvalues(
                                key TEXT NOT NULL UNIQUE PRIMARY KEY,
                                value TEXT
                            );",
    params![]);
    
    error_handle(res);
    
    let res = get_key_value(&connection, "version".to_string());
    if let Some(val) = res {
        if let Ok(version) = val.parse() {
            if version != SCHEMA_VERSION {
                migrate_db(&connection, version);
            }
            // else everything fine
        } else {
            // should never happen, would mean we accidentally wrote a text into the version value
            panic!("Database could not be loaded, version number is corrupted and reads: {}", val);
        }
    } else {
        set_key_value(&connection, "version".to_string(), SCHEMA_VERSION.to_string());

        // Database is new, we generate the whole schema
        // INSERT INTO keyvalues (key, value) VALUES ('version', '{}');
        let res = connection.execute_batch(format!(
            "CREATE TABLE users(
                user_id INTEGER PRIMARY KEY,
                user_name TINYTEXT NOT NULL UNIQUE,
                password BINARY(32) NOT NULL
            );
            CREATE TABLE devices(
                user_id INTEGER,
                device_id UNSIGNED TINYINT,
                device_name TEXT,
                PRIMARY KEY (user_id, device_id),
                FOREIGN KEY (user_id) REFERENCES users(user_id)
            );
            CREATE TABLE tokens(
                token BLOB PRIMARY KEY,
                user_id INTEGER,
                device_id UNSIGNED TINYINT,
                FOREIGN KEY (user_id) REFERENCES users(user_id),
                FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, device_id)
            );
            
                ").as_str()
        );

        error_handle(res);
    }
    

    

    connection
}

// This isn't great, but better then nothing
pub fn sanetize_string(input: &String) -> String {
    input.replace(|c: char| c == '\'' || c == '\"', "")
}

pub fn set_key_value(conn: &Connection, key: String, value: String) {
    //SET TRANSACTION ISOLATION LEVEL SERIALIZABLE
    let _res = conn.execute(format!("
            INSERT OR REPLACE INTO keyvalues (key, value) VALUES ('{0}','{1}');
            ", sanetize_string(&key), sanetize_string(&value)).as_str(), params![]);

    let _res = conn.cache_flush();
}

pub fn get_key_value(conn: &Connection, key: String) -> Option<String> {
    let res: Result<String, rusqlite::Error> = conn.query_row(format!("SELECT value FROM keyvalues WHERE key='{}'",sanetize_string(&key)).as_str(), params![], |row| row.get(0));
    if let Ok(val) = res {
        return Some(val)
    }
    None
}

pub fn authenticate(conn: &Connection, input_carrier:&TokenCarrier) -> Option<TokenCarrier> {
    let res:Result<(TokenCarrier,u32), rusqlite::Error> = conn.query_row(format!("SELECT token, device_id, user_id FROM tokens WHERE token=x'{}'", input_carrier.token_as_hex_string()).as_str(), params![],
             |row| Ok((TokenCarrier::new(row.get(0)?, row.get(1)?),row.get(2)?)));

    if let Ok((car, user_id)) = res {
        if let Some(input_device_id) = input_carrier.device_id {
            //If device id was omitted then we don't change device
            if car.get_device_id() != input_device_id {
                // We reauthenticate for the different device
                // Check if the device exists
                if let Some(device) = get_device(conn, user_id, input_device_id) {
                    // Delete the old token
                    let res = conn.execute(format!("DELETE FROM tokens WHERE token=x'{}'", car.token_as_hex_string()).as_str(), params![]);
                    if let Err(_e) = res {
                        return None;
                    }
    
                    return Some(TokenCarrier::new(create_token(conn, user_id, device.device_id),device.device_id));
                } else {
                    // We just return the one in the database with the old device id
                }
            }
        }

        // TODO replacing old tokens when it is time
        

        return Some(car);
    }

    None
}

pub fn login(conn: &Connection, name: String, password: U256, device_id: u8) -> Option<TokenCarrier> {
    let res:Result<(u32, [u8;32]), rusqlite::Error> = conn.query_row(format!("SELECT user_id, password FROM users WHERE user_name='{}'", sanetize_string(&name)).as_str(), params![], |row| Ok((row.get(0)?, row.get(1)?)));


    if let Ok((user_id, pw_bytes)) = res {
        let pw_hash = U256::from_u8arr(&pw_bytes);
        if password == pw_hash {
            // Authenticated
            if let None = get_device(conn, user_id, device_id) {
                // Falling back to default
                if let Some(_device) = get_device(conn, user_id, 0) {
                    return Some(TokenCarrier::new(create_token(conn, user_id, 0), 0));
                } else {
                    //Not even default exists, rejecting log in
                    return None;
                }
            }

            return Some(TokenCarrier::new(create_token(conn, user_id, device_id), device_id));
        }
    }

    None
}

// If the device does not exist we get a Stack overflow
fn create_token(conn: &Connection, user_id: u32, device_id: u8) -> Uuid {
    let token = Uuid::new_v4();

    let res = conn.execute("INSERT INTO tokens(token, user_id, device_id) VALUES (?1, ?2, ?3)", (token, user_id.clone(), device_id.clone()));

    if let Ok(rows) = res {
        if rows == 0 {
            return create_token(conn, user_id, device_id);
        }

        // There ought to be only one Token per user and device
        let _res = conn.execute(format!("DELETE FROM tokens WHERE user_id='{}' AND device_id='{}' AND NOT token=x'{}'", user_id, device_id, TokenCarrier::new_token(token).token_as_hex_string()).as_str(), params![]);

        return token;
    } else if let Err(e) = res {
        println!("{}",e.to_string());
    }

    create_token(conn, user_id, device_id)
}

pub fn get_auth_handle_from_token(conn: &Connection, token: Uuid) -> Option<AuthHandle> {
    let res: Result<(u32, u8), rusqlite::Error> = conn.query_row(format!("SELECT user_id, device_id FROM tokens WHERE token=x'{}'",
        TokenCarrier::new_token(token).token_as_hex_string()).as_str(), params![], |row| Ok((row.get(0)?, row.get(1)?)));

    if let Ok((user_id, device_id)) = res {
        //TODO handle token replacement
        return Some(AuthHandle{ user_id, device_id, token: None });
    }

    None
}

pub fn create_device(conn: &Connection, user_id: u32, device_name: String) -> Option<Device> {
    let res = get_user(conn, user_id);
    if let Some(user) = res {
        //Validate the user exists
        for i in 1..=255_u8 {
            if let None = get_device(conn, user.user_id, i) {
                // Finally a free ID
                let res = conn.execute("INSERT INTO devices(user_id, device_id, device_name) VALUES (?1, ?2, ?3)", 
                        (user_id, i, sanetize_string(&device_name)));

                if let Ok(_c) = res {
                    return get_device(conn, user_id, i);
                }
            }
        }

    }

    None
}

pub fn get_device(conn: &Connection, user_id: u32, device_id: u8) -> Option<Device> {
    let res:Result<Device, rusqlite::Error> = conn.query_row(format!("SELECT device_id, device_name FROM devices WHERE user_id='{}' AND device_id='{}'", user_id, device_id).as_str(), params![],
             |row| Ok(Device { device_id: row.get(0)?, device_name: row.get(1)? }));

    if let Ok(dev) = res {
        return Some(dev);
    }

    None
}

pub fn delete_device(conn: &Connection, user_id: u32, device_id: u8) -> bool{
    if device_id == 0 {
        return false; // Default device shall never be deleted
    }

    // Removing all tokens attached to the device
    let res = conn.execute(format!("DELETE FROM tokens WHERE user_id='{}' AND device_id='{}'", user_id, device_id).as_str(), params![]);
    if let Ok(_s) = res {
        // Deleting the device
        let res = conn.execute(format!("DELETE FROM devices WHERE user_id='{}' AND device_id='{}'", user_id, device_id).as_str(), params![]);
        if let Ok(_s) = res {
            return true;
        }
    }

    false
}

pub fn create_user(conn: &Connection, name: String, password: U256) -> bool {
    let res = conn.execute("INSERT INTO users (user_name, password) VALUES (?1, ?2)", (sanetize_string(&name), password.to_be_bytes()));

    if let Ok(_c) = res {
        let res:Result<u32, rusqlite::Error> = conn.query_row(format!("SELECT user_id FROM users WHERE user_name='{}'", sanetize_string(&name)).as_str(), params![],|row| row.get(0));
        if let Ok(user_id) = res {
            let res = conn.execute("INSERT INTO devices (user_id, device_id, device_name) VALUES (?1, ?2, ?3)", (user_id, 0, "DEFAULT"));
            return res.is_ok();
        }
        
    } else if let Err(_e) = res {
        return false;
    }

    false
}

pub fn get_user(conn: &Connection, user_id: u32) -> Option<User> {
    let res:Result<User, rusqlite::Error> = conn.query_row(format!("SELECT user_id, user_name, password FROM users WHERE user_id='{}'", user_id).as_str(), params![],|row| {
        Ok(User{user_id: row.get(0)?, user_name: row.get(1)?})
    });

    if let Ok(user) = res {
        return Some(user);
    }

    None
}

pub fn get_all_users(conn: &Connection) -> Vec<RequestUser> {
        
        let mut stmt = conn.prepare("SELECT user_id, user_name, password FROM users").unwrap();
        
        let user_iter = stmt.query_map([], |row| {
            let byte:[u8;32] = row.get(2)?;
            Ok(RequestUser::new(row.get(0)?, row.get(1)?, U256::from_u8arr(&byte)))
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


fn migrate_db(_conn: &Connection, curr_version:usize) {
    // fn error_handle<T>(res: Result<T,rusqlite::Error>) {
    //     if let Err(e) = res {
    //         panic!("Unable to set up database: {}", e.to_string());
    //     }
    // }

    if curr_version > SCHEMA_VERSION {
        panic!("Current Database version newer then SCHEMA, please update the software\nDB: {}; Schema: {}", curr_version, SCHEMA_VERSION);
    }

}

pub struct AuthHandle {
    pub user_id: u32,
    pub device_id: u8,
    pub token: Option<TokenCarrier>
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