use std::{path::{Path, PathBuf}, collections::HashMap, sync::{Mutex, MutexGuard}};
use super::{io, repository_file::{self, RepoFileType, RepoFile, Head, CommitInfo}};
use common::{U232,LargeU};

mod commit_generation;

pub fn read_storage_info(folder: &Path) -> std::io::Result<StorageRepo>{
    let mut file = PathBuf::from(folder);
    file.push("HEADER");
    
    if let Ok(head_file) = repository_file::read_repo_file(file.as_path()) {
        if let RepoFileType::Head(head_info) = head_file.get_type(0x00) {
            let head_info = head_info.clone(); // We have to gain ownership, else we can't create the repo, and then add the branches to it

            let mut repo = StorageRepo {
                folder: folder.as_os_str().to_str().unwrap().to_string(),
                header:head_file,
                branches: Vec::<RepoFile>::new(),
                commits: HashMap::<U232, Mutex<RepoFile>>::new()
            };

            repo.read_branches(&head_info);

            return Ok(repo);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Failed to read File",
    ))
}

pub fn new_repo(folder: &Path, name: String) -> std::io::Result<StorageRepo> {
    if folder.exists() {
        if !io::get_folder_content(folder).is_empty() {
            // Creating a repo in a folder that already exists is not intended
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Directory needs to be empty"
            ));
        }
    }
    
    if let Err(e) = io::create_folder(folder) {
        return Err(e);
    }

    let head = Head {
        name,
        branches: Vec::<String>::new()
    };

    let mut header_repo_file = RepoFile::new( 
        0,
        "HEADER".to_string(),
        vec![RepoFileType::Head(head); 1],
        U232::new(),
        U232::new()
    );

    header_repo_file.write_file_back(folder);

    Ok(StorageRepo {
        folder: folder.to_str().unwrap().to_string(),
        header: header_repo_file,
        branches: Vec::<RepoFile>::new(),
        commits: HashMap::<U232, Mutex<RepoFile>>::new()
    })

}

pub struct StorageRepo {
    folder: String,
    header: RepoFile,
    branches: Vec<RepoFile>,
    commits: HashMap<U232, Mutex<RepoFile>>
}

impl StorageRepo {
    pub fn update_header_and_branches(&mut self) {
        let folder = PathBuf::from(&self.folder);

        if self.header.reread_file(folder.as_path()) {
            // Header was changed, possibly a new branch, so remove all branches and re-add them
            if let RepoFileType::Head(head_info) = &self.header.get_type(0x00) {
                let head_info = head_info.clone();
                self.read_branches(&head_info);
            } else {
                // I don't like panics, but wtf am I supposed to do if this happens... It shouldn't, but what if it does?
                panic!("The header file of a repository at {} lost it's header information on a reload", self.folder);
            }
        } else {
            //We update the branches
            let mut iter = self.branches.iter_mut();
            while let Some(branch) = iter.next() {
                branch.reread_file(folder.as_path());
            }
        }
    }

    pub fn get_commit(&mut self, id: U232) -> std::io::Result<&Mutex<RepoFile>> {
        let mut file = PathBuf::from(&self.folder);

        if self.commits.contains_key(&id) {

            // This would update the object
            // let mut commit = self.commits[&id].clone();
            // if commit.reread_file(file.as_path()) {
            //     // There has been a change, updating cached
            //     self.commits.insert(id, commit);
            // }

            return Ok(&self.commits[&id]);
        }

        file.push(common::bytes_to_hex_string(id.to_be_bytes()));

        let res = repository_file::read_repo_file(file.as_path());
        if let Ok(commit) = res {
            self.commits.insert(id, Mutex::new(commit)); //adding it to the cache
            return Ok(&self.commits[&id]);

        } else if let Err(e) = res {
            return Err(e);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to read File",
        ))
    }

    pub fn get_commit_info(&mut self, commit_id: U232) -> Option<CommitInfo> {
        let history = self.get_commit_chain(commit_id);
        if history.is_empty() {
            return None;
        }

        let mut time = 0;
        for item in history.iter().rev() {
            let loc = item.lock().unwrap();
            if let RepoFileType::CommitInfo(info) = loc.get_type(0x10) {
                if let RepoFileType::NewFile = loc.get_type(0x03) {
                    time = 0;
                } else if let RepoFileType::NewFolder(_) = loc.get_type(0x0D) {
                    time = 0;
                }

                time += info.get_timestamp();
            }
            drop(loc);
        }

        let item = history[0].lock().unwrap();
        if let RepoFileType::CommitInfo(info) = item.get_type(0x10) {
            let i = CommitInfo::new(info.get_user(), info.get_device(), info.get_text(), time);
            drop(item);
            return Some(i);
        }


        None
    }

    pub fn set_commit_info(&mut self, commit_id: U232, info: CommitInfo) -> bool {
        let history = self.get_commit_chain(commit_id);
        if history.is_empty() {
            return false;
        }

        let item = history[0].clone(); // TODO make sure setting commit info actually works and doesn't update the first one

        let mut time = 0;
        for item in history.iter().rev() {
            let loc = item.lock().unwrap();
            if let RepoFileType::CommitInfo(info) = loc.get_type(0x10) {
                if let RepoFileType::NewFile = loc.get_type(0x03) {
                    time = 0;
                } else if let RepoFileType::NewFolder(_) = loc.get_type(0x0D) {
                    time = 0;
                }

                time += info.get_timestamp();
            }
            drop(loc);
        }

        time = info.get_timestamp().saturating_sub(time); // In case the time is lower we assume set it to be the same as the previous commit

        let info = CommitInfo::new(info.get_user(), info.get_device(), info.get_text(), time);

        let item = item.lock().unwrap();
        let mut content = item.get_content().clone();
        let mut index = 0;
        
        // Removing the old CommitInfo, if present
        for ele in content.iter() {
            if let RepoFileType::CommitInfo(_) = ele {
                break;
            }

            index += 1;
        }
        if index != content.len() {
            content.remove(index);
        }

        // Adding the new commitInfo and overwriting the old repofile
        content.push(RepoFileType::CommitInfo(info));
        let new_item = item.clone_with_content(content);
        drop(item);
        self.insert_commit(Mutex::new(new_item));


        true
    }

    fn insert_commit(&mut self, commit: Mutex<RepoFile>) -> U232 {
        let folder = PathBuf::from(&self.folder);
        let hash = {
            let mut commit = commit.lock().unwrap();
            commit.write_file_back(folder.as_path());
            U232::from_u8arr(common::hex_string_to_bytes(&commit.get_name()).as_slice())
        };
        
        self.commits.insert(hash.clone(), commit);
        hash
    }

    // TODO assess if this is okay, afterall it may stand in our way to figure out what was deleted in a commit
    // TODO Saw bug with incorrect ID being attached to branch, has to be tested
    fn get_free_commit_id_for_delete(&mut self, hash: &U232) -> U232 {
        let mut hash = hash.clone();
        let mut party_byte = 0;
        while let Ok(conflict) = self.get_commit(hash) {
            let conflict = conflict.lock().unwrap();
            if let RepoFileType::Delete = conflict.get_type(0x05) {
                break; // We can reuse existing deletes
            }

            party_byte = party_byte + 1; // Potential for panic through overflow
            hash.set_inequailty_byte(party_byte);
        }

        hash
    }

    fn get_free_commit_id(&mut self, hash: &U232) -> U232 {
        let mut hash = hash.clone();
        let mut party_byte = 0;
        while let Ok(_conflict) = self.get_commit(hash) {
            party_byte = party_byte + 1; // Potential for panic through overflow
            hash.set_inequailty_byte(party_byte);
        }

        hash
    }

    fn read_branches(&mut self, header_info: &Head) {
        self.branches = Vec::<RepoFile>::new(); // Emptying, before adding new ones

        let mut iter = header_info.branches.iter();
        while let Some(branch_name) = iter.next() {
            let mut file = PathBuf::from(&self.folder);
            file.push(branch_name);

            if let Ok(branch) = repository_file::read_repo_file(file.as_path()) {
                if let RepoFileType::BranchHead = branch.get_type(0x01) { // Not really necessary, we just want to insure everything is in order
                    self.branches.push(branch);
                }
            }
        }
    }

    pub fn get_branches(& self) -> &Vec<RepoFile> {
        &self.branches
    }

    pub fn get_branch(&self, name: String) -> Option<&RepoFile> {
        for branch in &self.branches {
            if branch.get_name() == &name {
                return Some(branch);
            }
        }

        None
    }

    pub fn get_folder(& self) -> &String {
        &self.folder
    }

    pub fn delete_branch(&mut self, name: String) -> bool {
        
        if let RepoFileType::Head(head) = self.header.get_type(0x00) {
            let mut head = head.clone();
            let mut index = 0;

            while index < head.branches.len() {
                if head.branches[index] == name {
                    break;
                }

                index = index + 1;
            }

            if index == head.branches.len() { // Not found
                return false;
            } else {
                head.branches.remove(index);
            }

            // Update head
            let new_header = self.header.clone_with_content(vec![RepoFileType::Head(head)]);
            self.header = new_header;
            self.header.write_file_back(PathBuf::from(&self.folder).as_path());

            // Refreshing
            self.update_header_and_branches();
            return true;
        }

        false
    }

    // pub fn get_folder(& self) -> &String {
    //     &self.folder
    // }

    pub fn push_commit_onto_branch(&mut self, repo_file: &RepoFile, branch_name: String) -> bool {
        // Updating the files
        self.update_header_and_branches();
        let folder = PathBuf::from(&self.folder);

        // Finding the branch
        let mut index = 0;
        let mut branch = &self.header; // Potential that there are no branches, so we load the header file to not get a out of bounds
        while index < self.branches.len() {
            branch = &self.branches[index];
            if branch.get_name() == &branch_name {
                break;
            }
            index = index + 1;
        }

        if branch.get_name() != &branch_name {
            // No branch with this name, creating one
            if let RepoFileType::Head(header) = self.header.get_type(0x00) {
                let mut header = header.clone();
                header.branches.push(branch_name.clone());
                
                // Create branch file
                let mut branch = RepoFile::new(
                    0,
                    branch_name.clone(),
                    vec![RepoFileType::BranchHead;1],
                    U232::from_u8arr(common::hex_string_to_bytes(repo_file.get_name()).as_slice()),
                    U232::new()
                );
                branch.write_file_back(folder.as_path());

                // Updating Header file
                let new_header = self.header.clone_with_content(vec![RepoFileType::Head(header)]);
                self.header = new_header;
                self.header.write_file_back(folder.as_path());

                // Updating cache
                self.update_header_and_branches();
                return true;
            } else {
                // The Header file does not have a header info? This should not happen
                panic!("Header file does not have header information");
            }
        }

        // Checking if the branch has been updated since
        if repo_file.get_previous_commit() != branch.get_previous_commit() {
            // There is a conflict
            return false;
        }

        // Updating the branch
        let mut branch = branch.clone_with_prev_commit(U232::from_u8arr(common::hex_string_to_bytes(repo_file.get_name()).as_slice()));
        branch.write_file_back(folder.as_path());

        // Update the information again
        self.update_header_and_branches();

        true
    }

    pub fn create_commit(&mut self, prev_commit_id: Option<U232>, location: &Path, is_root: bool) -> Option<U232> {
        let prev_commit_id = if let Some(prev) = prev_commit_id {
            if prev == U232::new() {
                None
            } else {
                if self.get_commit(prev).is_err() {
                    return None;
                }
                Some(prev)
            }
        } else {
            None
        };

        // TODO fix why it recreates the file from scratch

        if !location.exists() {
            if let Some(prev) = prev_commit_id {
                return Some(self.create_delete_commit(prev));
            } else {
                // Nothing to commit, exiting
                return None;
            }
        }

        if location.is_file() {
            return self.create_file_commit(prev_commit_id, location);
        }

        //TODO rewrite merger to move files into a folder if named
        //TODO check if build can build folders with empty names
        
        return self.create_folder_commit(prev_commit_id, location, is_root);
    }

    fn create_delete_commit(&mut self, prev_commit_id: U232) -> U232 {
        // Deleting what existed
        let repo = RepoFile::new(
            0,
            common::bytes_to_hex_string(self.get_free_commit_id_for_delete(&prev_commit_id).to_be_bytes()),
            vec![RepoFileType::Delete; 1],
            prev_commit_id,
            U232::new()
        );

        return self.insert_commit(Mutex::new(repo));
    }

    fn create_folder_commit(&mut self, prev_commit: Option<U232>, location: &Path, is_root: bool) -> Option<U232> {
        // TODO check if the folder exists
        

        let name = location.file_name().expect("Path does not contain folder name, somehow").to_str().expect("a os string is not a str, somehow").to_string();

        let mut commits = Vec::<U232>::new();
        let mut appended = Vec::<u8>::new();
        let mut repo_file_type = Vec::<RepoFileType>::new();
        let mut final_prev_commit = U232::new();



        if let Some(prev_commit) = prev_commit {
            final_prev_commit = prev_commit.clone();

            let history = self.get_commit_chain(prev_commit);
            if history.is_empty() {
                return None; // Previous commit does not exist
            }
            let p = history[0].lock().unwrap();
            if let RepoFileType::Delete = p.get_type(0x05) {
                // If the previous commit was a delete we start from scratch (which is easier done by calling the function again on the same folder)
                drop(p);
                return self.create_folder_commit(None, location, is_root);
            }
            drop(p);
            

            // Checking for a rename
            let mut old_name = None;
            for item in history.iter() {
                let temp = item.lock().unwrap();
                if let RepoFileType::NewFolder(new_folder) = temp.get_type(0x0D) {
                    old_name = Some(new_folder.clone());
                    drop(temp);
                    break;
                }
                drop(temp);
            }

            if let Some(old) = old_name {
                if old != name && !is_root {
                    repo_file_type.push(RepoFileType::NewFolder(name));
                }
            } else {
                return None; // couldn't find an original file name
            };

            // Getting the old file list
            let mut old_sub_commits = if let Some(data) = commit_generation::get_old_sub_info(self, prev_commit) {
                data
            } else {
                return None;
            };
            
            let mut content = io::get_folder_content(location);
            let mut left_over_commits = Vec::<commit_generation::OldSub>::new();

            let mut commits = Vec::<U232>::new();
            let mut appended = Vec::<u8>::new();

            let mut changed = false; // Just comparing the commit ids should be sufficient, but we are not taking the chances

            // Iterating over the old_subcommits and content, looking for clean matches, processing those, and missfits get listed
            while let Some(sub) = old_sub_commits.pop() { //We have to iterate over the old_commits first, so the folder hash remains the same if no files changed
                let mut sub_commit = None;
                let mut index = 0;
                for item in content.iter() {
                    let name = item.file_name().expect("Path does not contain file/folder name, somehow").to_str().expect("a os string is not a str, somehow").to_string();

                    if sub.name == name {
                        // We have got a match 
                        if item.is_dir() && sub.is_folder {
                            sub_commit = self.create_folder_commit(Some(sub.id), item.as_path(), false);
                        } else if item.is_file() && !sub.is_folder {
                            sub_commit = self.create_file_commit(Some(sub.id), item.as_path());
                        } else {
                            // Folders can't have the same name as files, one was renamed/removed, and another type was created
                            break;
                        }

                        if let None = sub_commit {
                            return None; // Something went wrong, abort
                        }
                        break;
                    }

                    index += 1;
                }

                if let Some(commit) = sub_commit {
                    if sub.id != commit {
                        changed = true;
                    }

                    appended.append(&mut commit.to_be_bytes().to_vec());
                    commits.push(commit);
                    content.remove(index);
                } else {
                    // No clean match was found for this one, we add it to the left overs
                    left_over_commits.push(sub);
                }
            }

            // if there has been only perfect matches, we may exit
            if left_over_commits.is_empty() && content.is_empty() {
                // in case nothing changed, no renaming, we may just return the previous commit
                if !changed && repo_file_type.is_empty() {
                    return Some(prev_commit);
                }
            } else {
                // We process the remaining files/folders, trying to match them to the remaining old sub commits, if not possible creating new entries
                for item in content {
                    let commit_to_add = if item.is_file() {
                        
                        let res = commit_generation::process_leftover_file(self, &mut left_over_commits, &item, location);

                        match res {
                            Ok(index) => {
                                let commit = left_over_commits[index].id;
                                left_over_commits.remove(index);

                                self.create_file_commit(Some(commit), item.as_path())
                            },
                            Err(fine) => {
                                if fine {
                                    self.create_file_commit(None, item.as_path())
                                } else {
                                    return None;
                                }
                            }
                        }
                    } else {
                        let res = commit_generation::process_leftover_folder(self, &left_over_commits, &item);
                        
                        match res {
                            Ok(index) => {
                                let commit = left_over_commits[index].id;
                                left_over_commits.remove(index);

                                self.create_folder_commit(Some(commit), item.as_path(), false)
                            },
                            Err(fine) => {
                                if fine {
                                    self.create_folder_commit(None, item.as_path(), false)
                                } else {
                                    return None;
                                }
                            }
                        }
                    };

                    if let Some(commit) = commit_to_add {
                        appended.append(&mut commit.to_be_bytes().to_vec());
                        commits.push(commit);
                    }
                }


                // Remaing old commits need to be marked as to be delete
                for old in left_over_commits {
                    let repo = RepoFile::new(
                        0,
                        common::bytes_to_hex_string(self.get_free_commit_id_for_delete(&old.id).to_be_bytes()),
                        vec![RepoFileType::Delete; 1],
                        old.id,
                        U232::new()
                    );
    
                    let commit = self.insert_commit(Mutex::new(repo));

                    appended.append(&mut commit.to_be_bytes().to_vec());
                    commits.push(commit);
                }
            }
        } else {
            // new commit
            let content = io::get_folder_content(location);

            // Generating all sub commits
            for item in content {
                let res =if item.is_file() {
                    self.create_file_commit(None, item.as_path())
                } else {
                    // it is a directory
                    self.create_folder_commit(None, item.as_path(), false)
                };

                if let Some(com) = res {
                    appended.append(&mut com.to_be_bytes().to_vec());
                    commits.push(com);
                } else {
                    // As the item excists, it must be able to generate a commit
                    return None; // Something went wrong, aborting
                }
            }
            let name = if is_root {
                "".to_string()
            } else {
                name
            };
            repo_file_type.push(RepoFileType::NewFolder(name));
        }

        // Generating folder hash
        let folder_hash = self.get_free_commit_id(&common::hash_data(appended.as_slice()));
        
        repo_file_type.push(RepoFileType::Folder(commits));

        let id = self.insert_commit(
            Mutex::new(
            RepoFile::new(
                0,
                common::bytes_to_hex_string(folder_hash.to_be_bytes()),
                repo_file_type,
                final_prev_commit,
                U232::new()
        )));
        return Some(id);
    }

    fn create_file_commit(&mut self, prev_commit: Option<U232>, location: &Path) -> Option<U232> {
        let (new_data,
            mut old_data,
            new_hash,
            rename,
            prev_com_id) = if let Some(old_id) = prev_commit {

            let p = if let Ok(commit) = self.get_commit(old_id) {
                commit
            } else {
                return None;
            }.lock().unwrap();
            if let RepoFileType::Delete = p.get_type(0x05) {
                // If the previous commit was a delete we start from scratch (which is easier done by calling the function again on the same folder)
                drop(p);
                return self.create_file_commit(None, location);
            }
            drop(p);
            
            if let Ok(new_data) = io::read_bytes(location) {
                let new_hash = common::hash_data(new_data.as_slice());

                if new_hash.equal_224(&old_id) {
                    // no changes in the file, return the Prev commit
                    return Some(old_id);
                }

                let (loc, old_data) = self.build_file(old_id, location);

                let rename = if loc.file_name() != location.file_name() {
                    Some(location.file_name())
                } else {
                    None
                };

                (new_data, old_data, new_hash, rename, old_id)
             } else {
                return None;
            }
        } else {
            // First commit
            if let Ok(new_data) = io::read_bytes(location) {
                let new_hash = common::hash_data(new_data.as_slice());

                (new_data, vec![0_u8;0], new_hash, Some(location.file_name()), U232::new())
            } else {
                return None;
            }
        };
        
        let mut content = Vec::<RepoFileType>::new();

        // New File
        if prev_com_id == U232::new() {
            content.push(RepoFileType::NewFile);
        }

        // Resize
        if old_data.len() != new_data.len() {
            content.push(RepoFileType::Resize(new_data.len().try_into().unwrap()));

            // Resizing old_data so we can compare
            if old_data.len() > new_data.len() {
                old_data = old_data[..new_data.len()].to_vec();
            } else {
                old_data.append(&mut vec![0_u8; new_data.len() - old_data.len()])
            }
        }

        // Rename
        if let Some(name) = rename {
            let name = name.unwrap().to_str().unwrap().to_string();
            content.push(RepoFileType::Rename(name));
        }

        // Edit
        content.push(commit_generation::generate_file_instructions(old_data, new_data));

        let repo_file = RepoFile::new(
            0, // Current Version
            common::bytes_to_hex_string(self.get_free_commit_id(&new_hash).to_be_bytes()),
            content, 
            prev_com_id,
            U232::new()
        );
        
        Some(self.insert_commit(Mutex::new(repo_file)))
    }

    fn get_commit_chain<'a>(&'a mut self, commit: U232) -> Vec<&'a Mutex<RepoFile>> {
        let mut stack = Vec::<&Mutex<RepoFile>>::new();

        let mut ids = Vec::<U232>::new();
        let mut index = commit;
        while let Ok(res) = self.get_commit(index) {
            let prev_commit = {
                let file = res.lock().unwrap();
                file.get_previous_commit()
                //TODO potentially cut down calls, as build folder and file do not need the full history
            };

            ids.push(prev_commit.clone());
            index = prev_commit;
        }

        for i in ids {
            if self.commits.contains_key(&i) { // just avoid the zero pointer from the initial commit
                stack.push(&self.commits[&i]);
            }
        }

        stack
    }

    pub fn build_commit(&mut self, commit_id: U232, target_folder: &Path) {
        if let Ok(repo_file) = self.get_commit(commit_id){
            

            let res = { 
                let repo_file = repo_file.lock().unwrap();
                repo_file.get_type(0x0F).clone()
            };

            if let RepoFileType::Folder(_d) = res {
                self.build_folder(commit_id, target_folder);
            } else {
                let (file, data) = self.build_file(commit_id, target_folder);
                let _res = io::write_bytes(file.as_path(), data); //TODO prober handling
            }
        }
    }

    fn build_file(&mut self, commit: U232, target_folder: &Path) -> (PathBuf, Vec<u8>) {
        let mut stack = Vec::<MutexGuard<RepoFile>>::new();

        let mut max_file_size:usize = 0;
        let mut cur_file_size:usize = 0;
        let mut file_name = String::new();

        let full_history = self.get_commit_chain(commit);
        for temp in full_history {
            let temp = temp.lock().unwrap();

            // Let us also check for the largest file size, needed for defining the size of our build file
            if let RepoFileType::Resize(size) = temp.get_type(0x08) {
                let size = size.clone().try_into().unwrap();
                if size > max_file_size {
                    max_file_size = size;
                }

                // Only update cur_file_size on the first resize
                if cur_file_size == 0 {
                    cur_file_size = size;
                }
            }
            // As we are iterating into the past we take the first occurence of a new name and save it. We do not need older names
            if file_name.is_empty() {
                if let RepoFileType::Rename(name) = temp.get_type(0x04) {
                    file_name = name.clone();
                }
            }

            // Writing onto the stack
            if let RepoFileType::NewFile = temp.get_type(0x03) { // We exit once we read in all instruction up to a New File
                stack.push(temp);
                break;
            } else {
                stack.push(temp);
            }
        }

        let mut data: Vec<u8> = vec![0_u8; max_file_size];
        let mut pointer_size: usize = 0;

        // Executing the code
        while let Some(mut item) = stack.pop() {
            let res = item.get_type(0x02);
            
            if let RepoFileType::Edit(ins, p_size) = res {
                pointer_size = p_size.clone(); // We update the pointer size for future repo files

                let mut iter = ins.iter();
                while let Some(instruction) = iter.next() {
                    instruction.run_instruction(&mut data);
                }
            } else if let RepoFileType::EditNotProcessed(_bytes) = res {
                if let Ok(p_size) = item.get_pointer_size() { // In case there was a resize on this commit
                    pointer_size = p_size;
                }

                // Processing Instructions
                //let mut item = item;
                item.parse_edit_instructions(pointer_size);

                // Running instructions
                if let RepoFileType::Edit(ins, _p) = item.get_type(0x02) {
                    let mut iter = ins.iter();
                    while let Some(instruction) = iter.next() {
                        instruction.run_instruction(&mut data);
                    }
                }

                // Updating cache
                //self.insert_commit(item);
            } else if let Ok(p_size) = item.get_pointer_size() {
                // This is for the special case that there was no edit instruction, but a resize instruction, so we update that for future commits
                pointer_size = p_size.clone(); 
            }
        }

        let mut file = PathBuf::from(target_folder);
        file.push(file_name);

        data = data[..cur_file_size].to_vec(); // setting the correct file size

        // TODO validate the file hash

        (file, data)

    }

    fn build_folder(&mut self, commit: U232, target_folder: &Path) {
        let mut folder_path = PathBuf::from(target_folder.as_os_str());

        let full_history = self.get_commit_chain(commit);
        if full_history.len() == 0 {
            return; // Commit does not exist
        }

        // Used later to build the folder
        let res = {
            let val = &full_history[0];
            let newest_commit = val.lock().unwrap();
            newest_commit.get_type(0x0F).clone()
        };

        // Getting the new folder instruction to the get the name
        for item in full_history {
            let temp = item.lock().unwrap();

            if let RepoFileType::NewFolder(name) = temp.get_type(0x0D) {
                folder_path.push(name);

                // Creating the folder
                if let Err(_e) = io::create_folder(folder_path.as_path()) {
                    // TODO
                }
                break;
            }
        }

        // Building the folder
        if let RepoFileType::Folder(items) = res {
            let mut iter = items.iter();
            while let Some(commit) = iter.next() {
                self.build_commit(commit.clone(), folder_path.as_path());
            }
        }


    }
}
