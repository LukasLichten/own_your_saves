use std::{path::PathBuf, collections::HashMap, sync::Mutex, str::FromStr};

use storage::StorageRepo;
use rusqlite::Connection;
use uuid::Uuid;

use crate::database;

pub mod io;
pub mod storage;
pub mod repository_file;

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

    // Pruning the temp folder, in case some got deleted and the like
    if !prune_temp_folders(db) {
        panic!("Something went wrong when cleaning out the temp folder");
    }

    con
}

impl RepoController {
    pub fn reload_folder(&mut self, db: &Connection) {
        let dir = io::get_folder_content(PathBuf::from(&self.root_path).as_path());

        self.repos.clear();
        let mut list = database::list_repos(&db, None);
        for folder in dir {
            let res = storage::read_storage_info(folder.as_path());
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

        let res = storage::new_repo(path.as_path(), name.clone());
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

pub fn list_temp_folder_content(db: &Connection, folder_token: Uuid) -> Option<Vec<String>> {
    fn recursive_folder(folder: PathBuf) -> Vec<String> {
        let mut content = Vec::<String>::new();

        let res = io::get_folder_content(folder.as_path());
        for item in res {
            let name = item.file_name().expect("Somehow, file path does not contain a filename").to_str().expect("And an OSstring somehow isn't a str").to_string();

            if item.is_file() {
                content.push(name);
            } else {
                // Recursively dealing with a folder
                content.push(name.clone()); // folder is added too

                for sub_item in recursive_folder(item){
                    content.push(name.clone() + "/" + sub_item.as_str());
                }
            }
        }

        content
    }
    
    let root = database::get_key_value(db, KEY_TEMP_FOLDER.to_string());
    if let Some(root) = root {
        let mut path = PathBuf::from(root);
        path.push(folder_token.to_string());

        let content = recursive_folder(path);

        return Some(content);
    }
    None
}

pub fn merge_temp_folder_into(db: &Connection, from_token: Uuid, target_token: Uuid, folder_name: String) -> bool {
    if let Some(from) = get_temp_folder_path(db, from_token) {
        if let Some(mut to) = get_temp_folder_path(db, target_token) {
            to.push(folder_name);

            if io::copy_folder(from.as_path(), to.as_path()).is_err() {
                return false;
            }

            return delete_temp_folder(db, from_token);
        }
    }

    false
}

pub fn prune_temp_folders(db: &Connection) -> bool {
    let root = database::get_key_value(db, KEY_TEMP_FOLDER.to_string());
    if let Some(root) = root {
        let path = PathBuf::from(root);

        // We first find all the folders
        let mut folders = Vec::<Uuid>::new();
        for item in io::get_folder_content(path.as_path()) {
            let name = item.file_name().expect("content in a folder does not have name, somehow").to_str().expect("An OSstring is somehow not a str");
            
            if let Ok(id) = Uuid::from_str(name) {
                folders.push(id);
            }
        }

        let to_be_deleted = database::prune_temp_folders(db, folders);

        for item in to_be_deleted {
            let mut target = path.clone();
            target.push(item.to_string());

            if let Err(_) = io::delete_folder(target.as_path()) {
                return false;
            }
        }


        return true;
    }

    false
}