use std::{path::Path};
use super::io;

pub fn read_storage_info(folder: &Path){

}

pub struct StorageRepo {
    folder: Path
}

pub struct RepoFile {
    version: u8,
    name: String,
    content: Vec<RepoFileType>,
    previous_commit: u32 //0 if not oplicable, or this is the first commit
}

pub enum RepoFileType {
    Head(Head),
    BranchHead,
    Edit(Vec<Instruction>),
    EditNotProcessed(Vec<u8>),
    NewFile,
    Resize(u64),
    Delete,
    Rename(String),
    NewFolder(String),
    Folder(Vec<u32>),
    CommitInfo(CommitInfo)
}

pub struct Head {
    name: String,
    branches: Vec<String>
}

pub struct CommitInfo {
    user_id: u32,
    text: String,
    timestamp: u64 //outside of the inital commit, this will be relative to the previous commit
}

pub struct Instruction {
    pointer: usize, // When compiled to 32bit we are restricted to 4gb files
    num_bytes: usize,
    operation: Operation
}

pub enum Operation {
    None,
    Replace(Vec<u8>),
    Blank,
    SetTo(u8),
    Copy(usize)
}

pub trait Writtable {
    fn to_bytes(& self, pointer_size: usize) -> Vec<u8>;
}

impl RepoFile {
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

            self.content[end] = RepoFileType::Edit(list);
        }
    }

    // Returns the pointer size, or the previous commit which may contain it
    pub fn get_pointer_size(& self) -> Result<usize,u32> {
        fn process_size(val: u64) -> usize {
            let bits = val.ilog2(); // technically this is one bit short
            let bytes = bits / 8 + 1; // but this means it handles the rounding
            // for example 16bits is log2=15 is 15/8 = 1 + 1 = 2
            // 17 bits is log2=16 is 16/8 = 2 + 1 = 3 

            bytes.try_into().unwrap_or_default()
        }
        
        if let RepoFileType::Resize(val) = self.content[0] {  
            return Ok(process_size(val));
        }
        if let RepoFileType::Resize(val) = self.content[0] {  
            return Ok(process_size(val));
        }

        Err(self.previous_commit)
    }
}

impl Writtable for RepoFile {
    fn to_bytes(& self, pointer_size: usize) -> Vec<u8> {
        let mut data = Vec::<u8>::new();
        data.push(self.version);
        data.push(0x00); // Type
        
        if let RepoFileType::Head(head) = &self.content[0] {
            // Head
            data[1] = 0x00;
            data.append(&mut head.to_bytes(pointer_size));
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
            data.append(&mut info.to_bytes(pointer_size));
            
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

        if let RepoFileType::Edit(instructions) = &self.content[offset + index] {
            // Resize
            if let RepoFileType::NewFile = self.content[offset - 1] { 
            } else {
                data[1] = data[1] + 0x02;
            }
            
            let mut iter = instructions.iter();
            while let Some(int) = iter.next() {
                data.append(&mut int.to_bytes(pointer_size));
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
    fn to_bytes(& self, _pointer_size: usize) -> Vec<u8> {
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
    fn to_bytes(& self, _pointer_size: usize) -> Vec<u8> {
        let mut data = Vec::<u8>::new();

        data.append(&mut self.user_id.to_be_bytes().to_vec());

        data.append(&mut self.text.as_bytes().to_vec());
        data.push(0x00);

        data.append(&mut io::value_to_utf8_bytes(self.timestamp).to_vec());
        data
    }
}

impl Writtable for Instruction {
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
}

// This reads the repo file and processes it
pub fn read_repo_file (file: &Path) -> std::io::Result<RepoFile> {

    if let Ok(data) = io::read_bytes(file) {
        let mut repo_file = RepoFile {
            version: data[0],
            name: file.file_name().unwrap().to_str().unwrap().to_string(),
            content: Vec::<RepoFileType>::new(),
            previous_commit: 0
        };

        let mut typ = data[1];

        let mut offset:usize = 2;

        
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
            return Ok(repo_file); // No further data
        }
        
        repo_file.previous_commit = io::get_u32(io::save_slice(&data, offset));
        offset = offset + 4;
        
        
        if typ == 0x01 {
            // Branch Head
            repo_file.content.push(RepoFileType::BranchHead);
            return Ok(repo_file); //No further data
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
            return Ok(repo_file); //No further data
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
            let mut files = Vec::<u32>::new();

            while offset < data.len() {
                files.push(io::get_u32(io::save_slice(&data, offset)));
                offset = offset + 4;
            }
            repo_file.content.push(RepoFileType::Folder(files));

            return Ok(repo_file); //Nothing more to add
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
            repo_file.content.push(RepoFileType::EditNotProcessed(io::save_slice(&data, offset).to_vec()));
        }
        //typ = typ % 0x02;

        return Ok(repo_file);
    }

    Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to read File"))
}