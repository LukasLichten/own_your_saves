use std::{path::{Path, PathBuf}, collections::HashMap, sync::{Mutex, MutexGuard}};
use super::io;
use common::{U232,LargeU};

pub fn read_storage_info(folder: &Path) -> std::io::Result<StorageRepo>{
    let mut file = PathBuf::from(folder);
    file.push("HEADER");
    
    if let Ok(head_file) = read_repo_file(file.as_path()) {
        if let RepoFileType::Head(head_info) = &head_file.content[0] {
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

    let mut header_repo_file = RepoFile {
        version: 0,
        name: "HEADER".to_string(),
        content: vec![RepoFileType::Head(head); 1],
        repo_file_hash: U232::new(),
        previous_commit: U232::new()
    };

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

        let res = read_repo_file(file.as_path());
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

    fn insert_commit(&mut self, commit: Mutex<RepoFile>) -> U232 {
        let folder = PathBuf::from(&self.folder);
        let hash = {
            let mut commit = commit.lock().unwrap();
            commit.write_file_back(folder.as_path());
            U232::from_u8arr(common::hex_string_to_bytes(&commit.name).as_slice())
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

            if let Ok(branch) = read_repo_file(file.as_path()) {
                if let RepoFileType::BranchHead = branch.content[0] { // Not really necessary, we just want to insure everything is in order
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
            if branch.name == name {
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
            self.header.content[0] = RepoFileType::Head(head);
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
            if branch.name == branch_name {
                break;
            }
            index = index + 1;
        }

        if branch.name != branch_name {
            // No branch with this name, creating one
            if let RepoFileType::Head(header) = self.header.get_type(0x00) {
                let mut header = header.clone();
                header.branches.push(branch_name.clone());
                
                // Create branch file
                let mut branch = RepoFile {
                    version: 0,
                    name: branch_name.clone(),
                    content: vec![RepoFileType::BranchHead;1],
                    previous_commit: U232::from_u8arr(common::hex_string_to_bytes(&repo_file.name).as_slice()),
                    repo_file_hash: U232::new()
                };
                branch.write_file_back(folder.as_path());

                // Updating Header file
                self.header.content[0] = RepoFileType::Head(header);
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
        if repo_file.previous_commit != branch.previous_commit {
            // There is a conflict
            return false;
        }

        // Updating the branch
        let mut branch = branch.clone();
        branch.previous_commit = U232::from_u8arr(common::hex_string_to_bytes(&repo_file.name).as_slice());
        branch.write_file_back(folder.as_path());

        // Update the information again
        self.update_header_and_branches();

        true
    }

    pub fn create_commit(&mut self, prev_commit_id: U232, location: &Path) -> Option<U232> {
        if !location.exists() {
            if prev_commit_id == U232::new() {
                // Nothing to commit, exiting
                return None;
            } else {
                // Deleting what existed
                let repo = RepoFile {
                    version: 0,
                    content: vec![RepoFileType::Delete; 1],
                    name: common::bytes_to_hex_string(self.get_free_commit_id_for_delete(&prev_commit_id).to_be_bytes()),
                    previous_commit: prev_commit_id,
                    repo_file_hash: U232::new()
                };

                return Some(self.insert_commit(Mutex::new(repo)));
            }
        }

        if let Ok(repo_file) = self.get_commit(prev_commit_id){

            let res  = { 
                let repo_file = repo_file.lock().unwrap();
                repo_file.get_type(0x0F).clone()
            };

            if let RepoFileType::Folder(_files) = res {
                if location.is_dir() {
                    return self.create_folder_commit(Some(prev_commit_id), location);
                } else {
                    // TODO
                }
            } else {
                return self.create_file_commit(Some(prev_commit_id), location);
            }
        } else {
            // No previous commit
            if location.is_dir() {
                return self.create_folder_commit(None, location);
            } else if location.is_file() {
                return self.create_file_commit(None, location);
            }
        }

        None
    }

    fn create_folder_commit(&mut self, prev_commit: Option<U232>, location: &Path) -> Option<U232> {
        None
    }

    fn create_file_commit(&mut self, prev_commit: Option<U232>, location: &Path) -> Option<U232> {
        let (new_data,
            mut old_data,
            new_hash,
            rename,
            prev_com_id) = if let Some(old_id) = prev_commit {
            
            if let Ok(new_data) = io::read_bytes(location) {
                let new_hash = common::hash_data(new_data.as_slice());

                if new_hash == old_id {
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
                //TODO handling this case properly
                return None;
            }
        } else {
            // First commit
            if let Ok(new_data) = io::read_bytes(location) {
                let new_hash = common::hash_data(new_data.as_slice());

                (new_data, vec![0_u8;0], new_hash, Some(location.file_name()), U232::new())
            } else {
                //TODO handling this case properly
                return None;
            }
        };
        
        let mut repo_file = RepoFile {
            name: common::bytes_to_hex_string(self.get_free_commit_id(&new_hash).to_be_bytes()),
            version: 0, // Current version
            previous_commit: prev_com_id,
            content: Vec::<RepoFileType>::new(),
            repo_file_hash: U232::new(),
        };

        // New File
        if repo_file.previous_commit == U232::new() {
            repo_file.content.push(RepoFileType::NewFile);
        }

        // Resize
        if old_data.len() != new_data.len() {
            repo_file.content.push(RepoFileType::Resize(new_data.len().try_into().unwrap()));

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
            repo_file.content.push(RepoFileType::Rename(name));
        }

        // Edit
        let mut diff = Vec::<(usize, u8)>::new();
        let mut index = 0;
        while index < new_data.len() {
            if new_data[index] != old_data[index] {
                diff.push((index, new_data[index]));
            }
            index = index + 1;
        }

        let mut instructions = Vec::<Instruction>::new();

        
        let pointer_size: usize = ((new_data.len().ilog2()) / 8 + 1).try_into().unwrap();
        let ins_overhead = 1 + pointer_size + 1; // Type Byte + Pointer Bytes + Minimum Bytes to define Length

        // Generating instructions, improvements here can severely reduce file size and instruction count, without changing compatibility
        index = 0;
        while index < diff.len() {
            let mut add_index = 1;

            let mut block = vec![diff[index].1;1];

            let mut single_type: bool = true;

            // Building a sequence to process in the instruction
            while (index + add_index) < diff.len() {
                let (last_offset, _o) = diff[index + add_index - 1];
                let (offset, val) = diff[index + add_index];

                if offset > last_offset + 1 {
                    // Meaning there is at least one unchanged byte interrupting the sequence, it maybe worth just writing those again
                    if offset > last_offset + ins_overhead {
                        // The gap is so large, it is more efficient to just start a new instruction
                        break;
                    } else if single_type && block[0] != val && block.len() > ins_overhead {
                        // Maintain single type and exit
                        break;

                    } else {
                        // We need to add the inbetween bytes to the block
                        let mut add_offset = 1;
                        while last_offset + add_offset <= offset { // = as we just let it also add the new item
                            let val = new_data[last_offset + add_offset];
                            
                            // If this has been a single type sequence we have to check
                            if single_type && val != block[0] {
                                if (block.len() - add_offset + 1) > ins_overhead {
                                    // We remove the items that are not needed and exit
                                    block = block[..(block.len() - add_offset + 1)].to_vec();
                                    break;
                                } else {
                                    single_type = false;
                                    block.push(val);
                                }
                            } else {
                                block.push(val);
                            }

                            add_offset = add_offset + 1;
                        }
                    }
                    
                } else {
                    // This is a simple sequence

                    // However we still want to check if we are still a single_type sequence
                    if single_type && block[0] != val {
                        // single_type sequence would get interrupted when adding this, we need to compute if it is worth making a new instruction, or switchting type
                        if block.len() > ins_overhead {
                            // We end this sequence
                            break;
                        } else {
                            // Not worth it, continuing
                            single_type = false;
                            block.push(val);
                        }
                    } else {
                        // We just continue
                        block.push(val);
                    }
                }

                add_index = add_index + 1;
            }

            // Constructing the Instruction
            let op = if single_type {
                let val = block[0]; 
                if val == 0x00 {
                    Operation::Blank
                } else {
                    Operation::SetTo(val)
                }
            } else {
                // TODO check existing bytes for match to do a copy on
                Operation::Replace(block.clone())
            };

            let ins = Instruction {
                pointer: diff[index].0,
                num_bytes: block.len(),
                operation: op
            };

            // we test the instructions to see if we get the correct result in the end
            ins.run_instruction(&mut old_data);

            instructions.push(ins);

            index = index + add_index; //- 1;
        }

        // Check of the instructions:
        if new_hash != common::hash_data(old_data.as_slice()) {
            panic!("TODO write error handling for when instructions are incorrectly generating, producing a file that does not match\nTarget Hash:{}\nResulting Hash:{}",new_hash,common::hash_data(old_data.as_slice()));
        }
        
        repo_file.content.push(RepoFileType::Edit(instructions, pointer_size));
        
        Some(self.insert_commit(Mutex::new(repo_file)))
    }

    fn get_commit_chain<'a>(&'a mut self, commit: U232) -> Vec<&'a Mutex<RepoFile>> {
        let mut stack = Vec::<&Mutex<RepoFile>>::new();

        let mut ids = Vec::<U232>::new();
        let mut index = commit;
        while let Ok(res) = self.get_commit(index) {
            let prev_commit = {
                let file = res.lock().unwrap();
                file.previous_commit.clone()
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

        // Getting the new folder instruction to the get the 
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

#[derive(Clone)]
pub struct RepoFile {
    version: u8,
    name: String,
    content: Vec<RepoFileType>,
    previous_commit: U232, //0 if not oplicable, or this is the first commit
    repo_file_hash: U232,
}

#[derive(Clone)]
pub enum RepoFileType {
    Head(Head),
    BranchHead,
    Edit(Vec<Instruction>, usize), // usize is the pointer size
    EditNotProcessed(Vec<u8>),
    NewFile,
    Resize(u64),
    Delete,
    Rename(String),
    NewFolder(String),
    Folder(Vec<U232>),
    CommitInfo(CommitInfo),
    None
}

#[derive(Clone)]
pub struct Head {
    name: String,
    branches: Vec<String>
}

#[derive(Clone)]
pub struct CommitInfo {
    user_id: u32,
    device_id: u8,
    text: String,
    timestamp: u64 //inital commit is unix time in s, after that this will be relative to the previous commit
}

#[derive(Clone)]
pub struct Instruction {
    pointer: usize, // When compiled to 32bit we are restricted to 4gb files
    num_bytes: usize,
    operation: Operation
}

#[derive(Clone)]
pub enum Operation {
    None,
    Replace(Vec<u8>),
    Blank,
    SetTo(u8),
    Copy(usize)
}

pub trait Writtable {
    fn to_bytes(& self) -> Vec<u8>;
}

pub enum WritingStates {
    NotNecessary,
    Ok,
    Conflict(Vec<u8>),
    Err(std::io::Error)
}

impl RepoFile {
    pub fn reread_file(&mut self, folder: &Path) -> bool {
        let mut file = PathBuf::from(folder.as_os_str());
        file.push(&self.name);

        if let Ok(data) = io::read_bytes(file.as_path()) {
            let hash = common::hash_data(data.as_slice());

            if hash != self.repo_file_hash { // file has changed, lets update
                let mut other = decode_repo_file(data, file.file_name().unwrap().to_str().unwrap().to_string());

                //Processing Edit, if possible
                if let Ok(pointer_size) = other.get_pointer_size()  {
                    other.parse_edit_instructions(pointer_size);
                } else if let RepoFileType::Edit(_ins, pointer_size) = &self.get_type(0x02) {
                    other.parse_edit_instructions(pointer_size.clone());
                }

                self.version = other.version;
                self.name = other.name; // This shouldn't change, but whatever
                self.content = other.content;
                self.previous_commit = other.previous_commit;
                self.repo_file_hash = other.repo_file_hash;

                return true;
            }
        }

        return false;
    }

    pub fn get_name(& self) -> &String {
        &self.name
    }

    pub fn get_previous_commit(& self) -> U232 {
        self.previous_commit
    }

    pub fn write_file_back(&mut self, folder: &Path) -> WritingStates{
        let mut file = PathBuf::from(folder.as_os_str());
        file.push(&self.name);

        // We check if anything changed
        let data = self.to_bytes();
        let new_hash = common::hash_data(data.as_slice());
        if self.repo_file_hash == new_hash {
            return WritingStates::NotNecessary;
        }

        // We check if the file changed since last pull
        if file.exists() {
            let res = io::read_bytes(file.as_path());
            if let Ok(file_data) = res {
                let file_hash = common::hash_data(file_data.as_slice());
                
                if new_hash == file_hash {
                    // In case we have written the file already, but not updated since
                    self.repo_file_hash = new_hash;
                    return WritingStates::NotNecessary;
                }

                if file_hash != self.repo_file_hash { 
                    // File has been updated since last pull
                    return WritingStates::Conflict(data);
                }
            } else if let Err(e) = res {
                return WritingStates::Err(e);
            }
        }

        // Technically, we have an issue here if the name of this file changes
        // This would happen if a branch is renamed, or the instructions to create the file changed in a way to produce a different outcome, producing a different hash and so a different commit name
        // In both cases, self.name must be updated prior to calling this function
        // The result is that the old files will stay behind, which is also an advantage, as any unaccounted references can still referre and function with them
        // maybe periodical clean up is required

        // We write the data
        let res = io::write_bytes(file.as_path(), data);
        if let Err(e) = res {
            return WritingStates::Err(e);
        }

        self.repo_file_hash = new_hash;
        WritingStates::Ok
    }

    pub fn parse_edit_instructions(&mut self, pointer_size: usize) {
        let end = self.content.len() - 1;

        // Edit is the final content piece, so we do a if let on it to get the data
        if let RepoFileType::EditNotProcessed(data) = &self.content[end] {
            let data = data.as_slice(); // turning the vector reference into a array
            let mut offset = 0;

            let mut list = Vec::<Instruction>::new();

            // Parsing the individual instructions
            while offset < data.len() {
                let typ = data[offset];
                offset = offset + 1;

                let pointer = io::u64_to_usize(io::get_u64(io::save_cut(io::save_slice(data, offset), pointer_size)));
                offset = offset + pointer_size;

                let (area, num_bytes) = io::get_utf8_value(io::save_slice(data, offset));
                offset = offset + num_bytes;
                let area = io::u64_to_usize(area);

                let operation = match typ {
                    0x01 => { // Replace
                        let re = io::save_cut(io::save_slice(data, offset),area).to_vec();
                        offset = offset + area;
                        Operation::Replace(re)
                    },
                    0x02 => Operation::Blank, // Blank
                    0x03 => { // Set To
                        let mut val = 0x00_u8;
                        if offset < data.len() {
                            val = data[offset];
                            offset = offset + 1;
                        }

                        Operation::SetTo(val)
                    },
                    0x04 => { // Copy From
                        let mut pointer = 0;
                        if (offset + pointer_size) < data.len() {
                            pointer = io::u64_to_usize(io::get_u64(io::save_cut(io::save_slice(data, offset),pointer_size)));
                            offset = offset + pointer_size;
                        }

                        Operation::Copy(pointer)
                    },
                    _ => Operation::None
                };

                list.push(Instruction {
                    pointer,
                    num_bytes: area,
                    operation
                });
            }

            self.content[end] = RepoFileType::Edit(list, pointer_size);
        }
    }

    pub fn get_type(& self, typ: u8) -> &RepoFileType {
        let mut iter = self.content.iter();
        while let Some(element) = iter.next(){
            match element {
                RepoFileType::Head(_d) => if typ == 0x00 {
                    return element;
                },
                RepoFileType::BranchHead => if typ == 0x01 {
                    return element;
                },
                RepoFileType::Edit(_d,_s) =>  if typ == 0x02 {
                    return element;
                },
                RepoFileType::EditNotProcessed(_d) =>  if typ == 0x02 {
                    return element;
                },
                RepoFileType::NewFile => if typ == 0x03 {
                    return element;
                },
                RepoFileType::Rename(_d) => if typ == 0x04 {
                    return element;
                },
                RepoFileType::Delete => if typ == 0x05 {
                    return element;
                },
                RepoFileType::Resize(_d) => if typ == 0x08 {
                    return element;
                },
                RepoFileType::NewFolder(_d) => if typ == 0x0D {
                    return element;
                },
                RepoFileType::Folder(_d) => if typ == 0x0F {
                    return element;
                },
                RepoFileType::CommitInfo(_d) => if typ == 0x10 {
                    return element;
                },
                RepoFileType::None => ()
            }
        }



        &RepoFileType::None
    }

    // Returns the pointer size, or the previous commit which may contain it
    pub fn get_pointer_size(& self) -> Result<usize, U232> {
        if let RepoFileType::Resize(val) = self.get_type(0x08) {
            let bits = val.ilog2(); // technically this is one bit short
            let bytes = bits / 8 + 1; // but this means it handles the rounding
                                      // for example 16bits is log2=15 is 15/8 = 1 + 1 = 2
                                      // 17 bits is log2=16 is 16/8 = 2 + 1 = 3

            
            return Ok(bytes.try_into().unwrap_or_default());
        }

        // If this file has processed edit, we have a pointer size
        if let RepoFileType::Edit(_ins, pointer_size) = self.get_type(0x02) {
            return Ok(pointer_size.clone());
        }


        Err(self.previous_commit)
    }
}

impl Writtable for RepoFile {
    fn to_bytes(& self) -> Vec<u8> {
        let mut data = Vec::<u8>::new();
        data.push(self.version);
        data.push(0x00); // Type

        if let RepoFileType::Head(head) = self.get_type(0x00) {
            // Head
            data[1] = 0x00;
            data.append(&mut head.to_bytes());
            return data;
        }

        // Previous Commit
        data.append(&mut self.previous_commit.to_be_bytes().to_vec());

        if let RepoFileType::BranchHead = self.get_type(0x01) {
            // Branch Head
            data[1] = 0x01;
            return data;
        }

        if let RepoFileType::CommitInfo(info) = &self.get_type(0x10) {
            // Commit Info, mixes with all remaining types
            data[1] = 0x10;
            data.append(&mut info.to_bytes());
        }

        if let RepoFileType::Delete = self.get_type(0x05) {
            //Delete
            data[1] = data[1] + 0x05;
            return data;
        }

        // Handles the different new instructions
        if let RepoFileType::NewFile = self.get_type(0x03) {
            // Branch Head
            data[1] = data[1] + 0x03;
        } else if let RepoFileType::NewFolder(name) = &self.get_type(0x0D) {
            // New Folder
            data[1] = data[1] + 0x0D;
            data.append(&mut name.as_bytes().to_vec());
            data.push(0x00);
        }

        if let RepoFileType::Folder(files) = &self.get_type(0x0F) {
            // Folder
            if let RepoFileType::NewFolder(_name) = &self.get_type(0x0D) {
            } else {
                data[1] = data[1] + 0x0F;
            }

            let mut iter = files.iter();
            while let Some(f) = iter.next() {
                data.append(&mut f.to_be_bytes().to_vec());
            }
            return data;
        }

        // Mixable Instructions
        if let RepoFileType::Resize(size) = &self.get_type(0x08) {
            // Resize
            if let RepoFileType::None = self.get_type(0x03) {
                data[1] = data[1] + 0x08;
            }

            data.append(&mut io::value_to_utf8_bytes(size.clone()).to_vec());
        }

        if let RepoFileType::Rename(name) = &self.get_type(0x04) {
            // Rename
            if let RepoFileType::None = self.get_type(0x03) {
                data[1] = data[1] + 0x04;
            }

            data.append(&mut name.as_bytes().to_vec());
            data.push(0x00);
        }

        let edit = self.get_type(0x02);
        if let RepoFileType::Edit(instructions, pointer_size) = edit {
            // Edit
            if let RepoFileType::None = self.get_type(0x03) {
                data[1] = data[1] + 0x02;
            }

            let mut iter = instructions.iter();
            while let Some(int) = iter.next() {
                let mut store = int.to_bytes(pointer_size.clone());
                data.append(&mut store);
            }
        } else if let RepoFileType::EditNotProcessed(dat) = edit {
            // Edit, but the instructions never got parsed
            if let RepoFileType::None = self.get_type(0x03) {
                data[1] = data[1] + 0x02;
            }

            data.append(&mut dat.clone());
        }

        data
    }
}

impl Writtable for Head {
    fn to_bytes(& self) -> Vec<u8> {
        let mut data = Vec::<u8>::new();

        data.append(&mut self.name.as_bytes().to_vec());
        data.push(0x00_u8);

        data.append(&mut io::value_to_utf8_bytes(self.branches.len().try_into().unwrap()));

        let mut iter = self.branches.iter();
        while let Some(val) = iter.next() {
            data.append(&mut val.as_bytes().to_vec());
            data.push(0x00_u8);
        }

        data
    }
}

impl Writtable for CommitInfo {
    fn to_bytes(& self) -> Vec<u8> {
        let mut data = Vec::<u8>::new();

        data.append(&mut self.user_id.to_be_bytes().to_vec());
        data.push(self.device_id);

        data.append(&mut self.text.as_bytes().to_vec());
        data.push(0x00);

        data.append(&mut io::value_to_utf8_bytes(self.timestamp).to_vec());
        data
    }
}

impl Instruction {
    fn to_bytes(& self, pointer_size: usize) -> Vec<u8> {
        fn resize_output(num: usize, pointer_size: usize) -> Vec<u8> {
            let bytes = num.to_be_bytes();
            bytes[bytes.len() - pointer_size..].to_vec()
        }

        let mut data = Vec::<u8>::new();

        data.push(0x00); //Type
        data.append(&mut resize_output(self.pointer, pointer_size));

        data.append(&mut io::value_to_utf8_bytes(self.num_bytes.try_into().unwrap_or_default()).to_vec());

        if let Operation::Replace(bytes) = &self.operation {
            // Replace
            data.append(&mut bytes.clone());
            data[0] = 0x01;
        } else if let Operation::Blank = &self.operation {
            //Blank
            data[0] = 0x02;
        } else if let Operation::SetTo(byte) = &self.operation {
            //Set To
            data.push(byte.clone());
            data[0] = 0x03;
        } else if let Operation::Copy(other_pointer) = &self.operation {
            //Copy
            data.append(&mut resize_output(other_pointer.clone(), pointer_size));
            data[0] = 0x04;
        } else {
            return Vec::<u8>::new(); // we delete this instruction
        }

        data
    }

    pub fn run_instruction(& self, data: &mut Vec<u8>) {
        if self.pointer >= data.len() {
            return; //pointer is out of bounds, we exit
        }
        
        // In case the edit area is out of bounds
        let num_bytes = if self.pointer + self.num_bytes > data.len() {
            data.len() - self.pointer
        } else {
            self.num_bytes
        };

        if let Operation::Replace(bytes) = &self.operation {
            let mut index = 0;
            while index < num_bytes {
                data[self.pointer + index] = bytes[index].clone();
                index = index + 1;
            }
        } else if let Operation::Blank = &self.operation {
            let mut index = 0;
            while index < num_bytes {
                data[self.pointer + index] = 0x00;
                index = index + 1;
            }
        } else if let Operation::SetTo(byte) = &self.operation {
            let mut index = 0;
            let byte = byte.clone();
            while index < num_bytes {
                data[self.pointer + index] = byte;
                index = index + 1;
            }
        } else if let Operation::Copy(other_pointer) = &self.operation {
            let other_pointer = other_pointer.clone();
            if other_pointer >= data.len() {
                return; // We can't copy something that is out of bounds
            }
            // In case the area from which we copy overflows, then we clamp it
            let num_bytes = if other_pointer + num_bytes > data.len() {
                data.len() - other_pointer
            } else {
                num_bytes
            };

            let mut index = 0;

            while index < num_bytes {
                data[self.pointer + index] = data[other_pointer + index].clone();
                index = index + 1;
            }
        }


    }
}

pub fn read_repo_file(file: &Path) -> std::io::Result<RepoFile> {
    let res = io::read_bytes(file);
    if let Ok(data) = res {
        return Ok(decode_repo_file(data, file.file_name().unwrap().to_str().unwrap().to_string()));
    } else if let Err(e) = res {
        return Err(e)
    }



    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Failed to read File",
    ))
}

// This reads the repo file and processes it
pub fn decode_repo_file(data: Vec<u8>, file_name: String) -> RepoFile {
    let mut repo_file = RepoFile {
        version: data[0],
        name: file_name,
        content: Vec::<RepoFileType>::new(),
        previous_commit: U232::new(),
        repo_file_hash: common::hash_data(data.as_slice()),
    };

    let mut typ = data[1];

    let mut offset: usize = 2;

    if typ == 0x00 {
        // Head
        let (name, num_bytes) = io::read_string_sequence(io::save_slice(&data, offset));
        offset = offset + num_bytes;
        let (num_branches, num_bytes) = io::get_utf8_value(io::save_slice(&data, offset));
        offset = offset + num_bytes;

        let mut head = Head {
            name,
            branches: Vec::<String>::new()
        };

        let mut index = 0;
        while index < num_branches && offset < data.len() {
            let (bra_name, num_bytes) = io::read_string_sequence(io::save_slice(&data, offset));
            offset = offset + num_bytes;

            if !bra_name.is_empty() {
                head.branches.push(bra_name);
            }

            index = index + 1;
        }

        repo_file.content.push(RepoFileType::Head(head));
        return repo_file; // No further data
    }

    repo_file.previous_commit = U232::from_u8arr(io::save_slice(&data, offset));
    offset = offset + U232::NUM_OF_BYTES;

    if typ == 0x01 {
        // Branch Head
        repo_file.content.push(RepoFileType::BranchHead);
        return repo_file; //No further data
    }

    if (typ % 0x20) / 0x10 == 1 {
        //Commit Info
        let user_id = io::get_u32(io::save_slice(&data, offset));
        offset = offset + 4;
        let device_id = if offset < data.len() { data[offset] } else {0_u8};
        offset = offset + 1;
        let (text, num_bytes) = io::read_string_sequence(io::save_slice(&data, offset));
        offset = offset + num_bytes;
        let (timestamp, num_bytes) = io::get_utf8_value(io::save_slice(&data, offset));
        offset = offset + num_bytes;

        repo_file.content.push(RepoFileType::CommitInfo(CommitInfo {
            user_id,
            device_id,
            text,
            timestamp
        }));
    }
    typ = typ % 0x10;

    if typ == 0x03 {
        // New File, we add a content node, then change typ to be a Edit, Resize, Rename
        repo_file.content.push(RepoFileType::NewFile);
        typ = 0x02 + 0x04 + 0x08;
    }

    if typ == 0x05 {
        // Delete
        repo_file.content.push(RepoFileType::Delete);
        return repo_file; //No further data
    }

    if typ == 0x0D {
        // New Folder
        let (folder_name, num_bytes) = io::read_string_sequence(io::save_slice(&data, offset));
        offset = offset + num_bytes;
        repo_file.content.push(RepoFileType::NewFolder(folder_name));

        typ = 0x0F;
        //We don't return as we add a folder typ to this to account for the files
    }
    if typ == 0x0F {
        // Folder
        let mut files = Vec::<U232>::new();

        while offset < data.len() {
            files.push(U232::from_u8arr(io::save_slice(&data, offset)));
            offset = offset + U232::NUM_OF_BYTES;
        }
        repo_file.content.push(RepoFileType::Folder(files));

        return repo_file; //Nothing more to add
    }

    if typ / 0x08 == 1 {
        // Resize
        let (size, num_bytes) = io::get_utf8_value(io::save_slice(&data, offset));
        offset = offset + num_bytes;

        repo_file.content.push(RepoFileType::Resize(size));
    }
    typ = typ % 0x08;

    if typ / 0x04 == 1 {
        // Rename
        let (text, num_bytes) = io::read_string_sequence(io::save_slice(&data, offset));
        offset = offset + num_bytes;

        repo_file.content.push(RepoFileType::Rename(text));
    }
    typ = typ % 0x04;

    if typ / 0x02 == 1 {
        // Edit
        // We can't process Edit instructions without knowing the pointer size, which we only find out when we know the file size
        // So lets just store the data containing all instructions
        repo_file.content.push(RepoFileType::EditNotProcessed(
            io::save_slice(&data, offset).to_vec(),
        ));
    }
    //typ = typ % 0x02;

    repo_file
}
