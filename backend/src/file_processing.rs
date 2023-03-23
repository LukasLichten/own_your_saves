use std::{path::PathBuf, collections::HashMap, sync::Mutex};

use repository::StorageRepo;
use rusqlite::Connection;

use crate::database;

pub mod io;
pub mod repository;

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