use std::{path::PathBuf, collections::HashMap, sync::Mutex};

use repository::StorageRepo;

pub mod io;
pub mod repository;

pub struct RepoController {
    root_path: String,
    repos: HashMap<String,Mutex<StorageRepo>>
}

pub fn init() -> RepoController {
    let path = std::env::var("REPO_PATH").unwrap_or("./target/repo/".to_string()); // TODO handle release, although this technically works as a default there too
    
    let place = PathBuf::from(&path);
    if !place.is_dir() {
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

    con.reload_folder();

    con
}

impl RepoController {
    pub fn reload_folder(&mut self) {
        let dir = io::get_folder_content(PathBuf::from(&self.root_path).as_path());

        self.repos.clear();
        for folder in dir {
            let res = repository::read_storage_info(folder.as_path());
            if let Ok(rep) = res {
                let name = folder.file_name().unwrap().to_str().unwrap().to_string(); // TODO maybe do this better

                self.repos.insert(name, Mutex::new(rep));
            }
        }
    }

    pub fn get_repo(& self, name: &String) -> Option<&Mutex<StorageRepo>> {
        self.repos.get(name)
    }
}