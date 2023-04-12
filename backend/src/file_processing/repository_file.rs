use std::path::{Path, PathBuf};

use common::{U232, LargeU};

use super::io;

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
    pub name: String,
    pub branches: Vec<String>
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
    pub fn get_name(& self) -> &String {
        &self.name
    }

    pub fn get_previous_commit(& self) -> U232 {
        self.previous_commit
    }

    pub fn clone_with_content(& self, content: Vec<RepoFileType>) -> RepoFile {
        RepoFile {
            version: self.version,
            name: self.name.clone(),
            content,
            previous_commit: self.previous_commit,
            repo_file_hash: self.repo_file_hash
        }
    }

    pub fn clone_with_prev_commit(& self, prev_commit: U232) -> RepoFile {
        RepoFile {
            version: self.version,
            name: self.name.clone(),
            content: self.content.clone(),
            previous_commit: prev_commit,
            repo_file_hash: self.repo_file_hash
        }
    }

    pub fn new(version: u8, name: String, content: Vec<RepoFileType>, previous_commit: U232, repo_file_hash: U232) -> RepoFile {
        RepoFile {
            version,
            name,
            content,
            previous_commit,
            repo_file_hash
        }
    }
    
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

    pub fn get_content<'a>(&'a self) -> &'a Vec<RepoFileType> {
        &self.content
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

impl CommitInfo {
    pub fn get_text(& self) -> String {
        self.text.clone()
    }

    pub fn get_user(& self) -> u32 {
        self.user_id
    }

    pub fn get_device(& self) -> u8 {
        self.device_id
    }

    pub fn get_timestamp(& self) -> u64 {
        self.timestamp
    }

    pub fn new(user_id: u32, device_id: u8, text: String, timestamp: u64) -> Self {
        CommitInfo { user_id, device_id, text, timestamp }
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
    pub fn new(pointer: usize, num_bytes: usize, operation: Operation) -> Instruction {
        Instruction {
            pointer,
            num_bytes,
            operation
        }
    }

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