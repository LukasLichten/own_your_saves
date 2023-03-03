use std::{path::{Path, PathBuf}, collections::HashMap};
use super::{io,u232};

pub fn read_storage_info(folder: &Path) -> std::io::Result<StorageRepo>{
    let mut file = PathBuf::from(folder.as_os_str());
    file.push("HEADER");
    
    if let Ok(head_file) = read_repo_file(file.as_path()) {
        if let RepoFileType::Head(head_info) = &head_file.content[0] {
            let head_info = head_info.clone(); // We have to gain ownership, else we can't create the repo, and then add the branches to it

            let mut repo = StorageRepo {
                folder: folder.as_os_str().to_str().unwrap().to_string(),
                header:head_file,
                branches: Vec::<RepoFile>::new(),
                commits: HashMap::<u232::U232, RepoFile>::new()
            };

            repo.get_branches(&head_info);

            return Ok(repo);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Failed to read File",
    ))
}

pub struct StorageRepo {
    folder: String,
    header: RepoFile,
    branches: Vec<RepoFile>,
    commits: HashMap<u232::U232, RepoFile>
}

impl StorageRepo {
    pub fn update_header_and_branches(&mut self) {
        let folder = PathBuf::from(&self.folder);

        if self.header.reread_file(folder.as_path()) {
            // Header was changed, possibly a new branch, so remove all branches and re-add them
            if let RepoFileType::Head(head_info) = &self.header.get_type(0x00) {
                let head_info = head_info.clone();
                self.get_branches(&head_info);
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

    pub fn get_commit(&mut self, id: u232::U232) -> std::io::Result<&RepoFile> {
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

        file.push(io::bytes_to_hex_string(id.to_be_bytes()));

        let res = read_repo_file(file.as_path());
        if let Ok(commit) = res {
            self.commits.insert(id, commit); //adding it to the cache
            return Ok(&self.commits[&id]);

        } else if let Err(e) = res {
            return Err(e);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to read File",
        ))
    }

    pub fn insert_commit(&mut self, commit: RepoFile) {
        self.commits.insert(u232::from_u8arr(io::hex_string_to_bytes(&commit.name).as_slice()), commit);
    }

    fn get_branches(&mut self, header_info: &Head) {
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

    pub fn build_commit(&mut self, commit_id: u232::U232, target_folder: &Path) {
        if let Ok(repo_file) = self.get_commit(commit_id){
            let repo_file = repo_file.clone();

            if let RepoFileType::Folder(_d) = repo_file.get_type(0x0F) {
                self.build_folder(&repo_file, target_folder);
            } else {
                self.build_file(&repo_file, target_folder);
            }
        }
    }

    fn build_file(&mut self, repo_file: &RepoFile, target_folder: &Path) {
        let mut stack = Vec::<RepoFile>::new();

        let mut max_file_size:usize = 0;
        let mut cur_file_size:usize = 0;
        let mut file_name = String::new();

        let mut temp = repo_file.clone();
        while let RepoFileType::None = temp.get_type(0x03) { // New File, aka it returns None if it is not contained, therefore let us continue iterating
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

            // The iteration part
            if let Ok(t) = self.get_commit(temp.previous_commit) {
                stack.push(temp);
                temp = t.clone();
            } else {
                //idk, either we got an error on io, or reached the first commit without a new file
                break; // But we have to break, else risk a deadlock
            }
        }

        let mut data: Vec<u8> = vec![0_u8; max_file_size];
        let mut pointer_size: usize = 0;

        // Executing the code
        while let Some(item) = stack.pop() {
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
                let mut item = item;
                item.parse_edit_instructions(pointer_size);

                // Running instructions
                if let RepoFileType::Edit(ins, _p) = item.get_type(0x02) {
                    let mut iter = ins.iter();
                    while let Some(instruction) = iter.next() {
                        instruction.run_instruction(&mut data);
                    }
                }

                // Updating cache
                self.insert_commit(item);
            } else if let Ok(p_size) = item.get_pointer_size() {
                // This is for the special case that there was no edit instruction, but a resize instruction, so we update that for future commits
                pointer_size = p_size.clone(); 
            }
        }

        let mut file = PathBuf::from(target_folder);
        file.push(file_name);

        data = data[..cur_file_size].to_vec(); // setting the correct file size

        io::write_bytes(file.as_path(), data);

    }

    fn build_folder(&mut self, repo_file: &RepoFile, target_folder: &Path) {
        let mut folder_path = PathBuf::from(target_folder.as_os_str());

        let mut temp = repo_file.clone();
        while let RepoFileType::None = temp.get_type(0x0D) { // New Folder, aka it returns None if it is not contained, therefore let us continue iterating
            if let Ok(t) = self.get_commit(temp.previous_commit) {
                temp = t.clone();
            } else {
                //idk, either we got an error on io, or reached the first commit without a new folder
                break; // But we have to break, else risk a deadlock
            }
        }

        // Creating the folder
        if let RepoFileType::NewFolder(name) = temp.get_type(0x0D) {
            folder_path.push(name);
            if let Err(e) = io::create_folder(folder_path.as_path()) {
                // TODO
            }
        }

        // Building the folder
        if let RepoFileType::Folder(items) = repo_file.get_type(0x0F) {
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
    previous_commit: u232::U232, //0 if not oplicable, or this is the first commit
    repo_file_hash: u232::U232,
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
    Folder(Vec<u232::U232>),
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
            let hash = io::hash_data(data.as_slice());

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
            }
        }

        return false;
    }

    pub fn write_file_back(& self, folder: &Path) -> WritingStates{
        let mut file = PathBuf::from(folder.as_os_str());
        file.push(&self.name);

        // We check if anything changed
        let data = self.to_bytes();
        if self.repo_file_hash == io::hash_data(data.as_slice()) {
            return WritingStates::NotNecessary;
        }

        // We check if the file changed since last pull
        if file.exists() {
            let res = io::read_bytes(file.as_path());
            if let Ok(file_data) = res {
                let hash = io::hash_data(file_data.as_slice());
    
                if hash != self.repo_file_hash {
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
                let pointer = io::u64_to_usize(io::get_u64(io::save_cut(io::save_slice(data, offset), pointer_size)));
                offset = offset + pointer_size;

                let typ = data[offset];
                offset = offset + 1;

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
    pub fn get_pointer_size(& self) -> Result<usize,u232::U232> {
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

        if let RepoFileType::Head(head) = &self.content[0] {
            // Head
            data[1] = 0x00;
            data.append(&mut head.to_bytes());
            return data;
        }

        // Previous Commit
        data.append(&mut self.previous_commit.to_be_bytes().to_vec());

        if let RepoFileType::BranchHead = self.content[0] {
            // Branch Head
            data[1] = 0x01;
            return data;
        }

        let mut offset = 0;
        if let RepoFileType::CommitInfo(info) = &self.content[0] {
            // Commit Info, mixes with all remaining types
            data[1] = 0x10;
            data.append(&mut info.to_bytes());

            offset = 1;
        }

        if let RepoFileType::Delete = self.content[offset] {
            //Delete
            data[1] = data[1] + 0x05;
            return data;
        }

        // Handles the different new instructions
        if let RepoFileType::NewFile = self.content[offset] {
            // Branch Head
            data[1] = data[1] + 0x03;
            offset = offset + 1;
        } else if let RepoFileType::NewFolder(name) = &self.content[offset] {
            // New Folder
            data[1] = data[1] + 0x0D;
            data.append(&mut name.as_bytes().to_vec());
            data.push(0x00);
            offset = offset + 1;
        }

        if let RepoFileType::Folder(files) = &self.content[offset] {
            // Folder
            if let RepoFileType::NewFolder(_name) = &self.content[offset - 1] {
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
        let mut index = 0;
        if let RepoFileType::Resize(size) = &self.content[offset + index] {
            // Resize
            if let RepoFileType::NewFile = self.content[offset - 1] {
            } else {
                data[1] = data[1] + 0x08;
            }

            data.append(&mut io::value_to_utf8_bytes(size.clone()).to_vec());

            index = index + 1;
        }

        if let RepoFileType::Rename(name) = &self.content[offset + index] {
            // Resize
            if let RepoFileType::NewFile = self.content[offset - 1] {
            } else {
                data[1] = data[1] + 0x04;
            }

            data.append(&mut name.as_bytes().to_vec());
            data.push(0x00);

            index = index + 1;
        }

        if let RepoFileType::Edit(instructions, pointer_size) = &self.content[offset + index] {
            // Resize
            if let RepoFileType::NewFile = self.content[offset - 1] {
            } else {
                data[1] = data[1] + 0x02;
            }

            let mut iter = instructions.iter();
            while let Some(int) = iter.next() {
                data.append(&mut int.to_bytes(pointer_size.clone()));
            }
        } else if let RepoFileType::EditNotProcessed(dat) = &self.content[offset + index] {
            // Resize, but the instructions never got parsed
            if let RepoFileType::NewFile = self.content[offset - 1] {
            } else {
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
            self.pointer
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
        previous_commit: u232::new(),
        repo_file_hash: io::hash_data(data.as_slice()),
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
        while index < num_branches && offset >= data.len() {
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

    repo_file.previous_commit = u232::from_u8arr(io::save_slice(&data, offset));
    offset = offset + u232::NUM_BYTES;

    if typ == 0x01 {
        // Branch Head
        repo_file.content.push(RepoFileType::BranchHead);
        return repo_file; //No further data
    }

    if (typ % 0x20) / 0x10 == 1 {
        //Commit Info
        let user_id = io::get_u32(io::save_slice(&data, offset));
        offset = offset + 4;
        let (text, num_bytes) = io::read_string_sequence(io::save_slice(&data, offset));
        offset = offset + num_bytes;
        let (timestamp, num_bytes) = io::get_utf8_value(io::save_slice(&data, offset));
        offset = offset + num_bytes;

        repo_file.content.push(RepoFileType::CommitInfo(CommitInfo {
            user_id,
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
        let mut files = Vec::<u232::U232>::new();

        while offset < data.len() {
            files.push(u232::from_u8arr(io::save_slice(&data, offset)));
            offset = offset + u232::NUM_BYTES;
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
