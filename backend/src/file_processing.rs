use std::{path::PathBuf, collections::HashMap, sync::Mutex};

use repository::StorageRepo;
use rusqlite::Connection;
use uuid::Uuid;

use crate::database;

pub mod io;
pub mod repository;

const KEY_TEMP_FOLDER:&str = "temp_folder";

pub struct RepoController {
    root_path: String,
    repos: HashMap<String,Mutex<StorageRepo>>
}

pub fn init(db: &Connection) -> RepoController {
    let path = std::env::var("REPO_PATH").unwrap_or("./target/repo/".to_string()); // TODO handle release, although this technically works as a default there too
    
    let place = PathBuf::from(&path);
    if place.exists() && !place.is_dir() {
        panic!("REPO_PATH has to be a folder. REPO_PATH value was: {}", path);
    }

    if let Err(e) = io::create_folder(place.as_path()) {
        // Error handle
        panic!("Unable to create folder for repositories at {}\nError: {}",path, e.to_string());
    }

    // Building the storage controller
    let mut con = RepoController {
        root_path: path,
        repos: HashMap::<String,Mutex<StorageRepo>>::new()
    };

    con.reload_folder(db);

    // Temp folder handling
    let temp_folder = PathBuf::from(if let Ok(temp_folder) = std::env::var("TEMP_PATH") {
        database::set_key_value(&db, KEY_TEMP_FOLDER.to_string(), temp_folder.clone());
        temp_folder
    } else if let Some(temp_folder) = database::get_key_value(&db, KEY_TEMP_FOLDER.to_string()) {
        temp_folder
    } else {
        // Setting default value
        let val = "./target/temp/".to_string();
        database::set_key_value(&db, KEY_TEMP_FOLDER.to_string(), val.clone());
        val
    });
    
    if temp_folder.is_file() {
        panic!("Temp Folder path points to a file: {}",temp_folder.to_str().unwrap());        
    } else if !temp_folder.exists() {
        if let Err(e) = io::create_folder(temp_folder.as_path()) {
            panic!("Unable to create temp folder at:{} \n Error code: {}", temp_folder.to_str().unwrap(), e.to_string())
        }

        let res = db.execute_batch("DELETE FROM temp_folder_reference; DELETE FROM temp_folder;");
        if let Err(e) = res {
            panic!("Unable to clear out temp data from the database: {}", e.to_string());
        }
    }

    con
}

impl RepoController {
    pub fn reload_folder(&mut self, db: &Connection) {
        let dir = io::get_folder_content(PathBuf::from(&self.root_path).as_path());

        self.repos.clear();
        let mut list = database::list_repos(&db, None);
        for folder in dir {
            let res = repository::read_storage_info(folder.as_path());
            if let Ok(rep) = res {
                let name = folder.file_name().unwrap().to_str().unwrap().to_string(); // TODO maybe do this better

                self.repos.insert(name.clone(), Mutex::new(rep));
                
                // Seeing if it already exists in the DB, if not add it
                let mut found = false;
                let mut index = 0;
                for item in list.iter() {
                    if item.repo_name == name {
                        found = true;
                        break;
                    }
                    index += 1;
                }

                if found {
                    list.remove(index);
                } else {
                    database::create_repo_fast(&db, name);
                }
            }
        }

        // Deleting repos that have not been found
        for item in list {
            database::delete_repo(&db, item.repo_name);
        }
    }

    pub fn create_repo(&mut self, name: String) -> bool {
        let mut path = PathBuf::from(&self.root_path);
        path.push(&name);

        let res = repository::new_repo(path.as_path(), name.clone());
        if let Ok(repo) = res {
            self.repos.insert(name, Mutex::new(repo));
            return true;
        }

        false
    }

    pub fn delete_repo(&mut self, name: &String) -> bool {
        if let Some(old_repo_mu) = self.repos.remove(name) {
            let old_repo = (&old_repo_mu).lock().unwrap();
            let path = PathBuf::from(old_repo.get_folder());

            if let Ok(_) = io::delete_folder(path.as_path()) {
                return true;
            } else {
                // Undo
                drop(old_repo);
                self.repos.insert(name.clone(), old_repo_mu);
                return false;
            }
        }
        false
    } 

    pub fn get_repo(& self, name: &String) -> Option<&Mutex<StorageRepo>> {
        self.repos.get(name)
    }
}

pub fn create_temp_folder(db: &Connection, folder_token: Uuid) -> bool {
    let root = database::get_key_value(db, KEY_TEMP_FOLDER.to_string());
    if let Some(root) = root {
        let mut path = PathBuf::from(root);
        path.push(folder_token.to_string());

        if path.exists() {
            // This should not happen, someone did not clean up, deleting folder
            if let Err(_e) = io::delete_folder(path.as_path()) {
                return false;
            }
        }

        if let Ok(_) = io::create_folder(path.as_path()) {
            return true;
        }
    }


    false
}

pub fn delete_temp_folder(db: &Connection, folder_token: Uuid) -> bool {
    let root = database::get_key_value(db, KEY_TEMP_FOLDER.to_string());
    if let Some(root) = root {
        let mut path = PathBuf::from(root);
        path.push(folder_token.to_string());

        if let Ok(_) = io::delete_folder(path.as_path()) {
            return true;
        }
    }
    false
}

pub fn get_temp_folder_path(db: &Connection, folder_token: Uuid) -> Option<PathBuf> {
    let root = database::get_key_value(db, KEY_TEMP_FOLDER.to_string());
    if let Some(root) = root {
        let mut path = PathBuf::from(root);
        path.push(folder_token.to_string());

        if path.exists() {
            return Some(path);
        }
    }

    None
}