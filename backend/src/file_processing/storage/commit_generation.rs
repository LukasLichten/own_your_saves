use std::path::{PathBuf, Path};

use common::{U232, LargeU};

use crate::file_processing::{storage::StorageRepo, io, repository_file::{RepoFileType, Instruction, Operation}};

const MAX_DIFFERENCE_PERCENT:u64 = 25;

pub struct OldSub {
    pub id: U232,
    pub name: String,
    pub is_folder: bool
}

pub fn process_leftover_file (store: &mut StorageRepo, left_over_commits: &Vec<OldSub>, item: &PathBuf, location: &Path) -> Result<usize, bool> {
    if let Ok(file_content) = io::read_bytes(item.as_path()) {
        let new_hash = common::hash_data(file_content.as_slice());
        
        let mut index = 0;
        // Lets see if an identical file exists
        for sub in left_over_commits.iter() {
            if !sub.is_folder && sub.id.equal_224(&new_hash) {
                // Match
                return Ok(index);
            }

            index += 1;
        }

        // we compare them, seeing if we get a close enough match
        // maybe we should iterate over all content to see if we get precise matches, but oh well, we might do this too
        let mut error_rates = Vec::<(usize, u64)>::new();
        let max_error_rate:u64 = file_content.len().try_into().unwrap();
        let max_error_rate = max_error_rate * MAX_DIFFERENCE_PERCENT;

        let mut index = 0;
        for sub in left_over_commits.iter() {
            if !sub.is_folder {
                let (_, mut sub_file) = store.build_file(sub.id, location);
                let sub_file_size = sub_file.len();

                // Resizing old_data so we can compare
                if sub_file.len() > file_content.len() {
                    sub_file = sub_file[..file_content.len()].to_vec();
                } else {
                    sub_file.append(&mut vec![0_u8; file_content.len() - sub_file.len()])
                }

                let diff = io::generate_vec_diff(&sub_file, &file_content).expect("Vec size should match, right?");
                let error_count = diff.len() + sub_file_size.abs_diff(file_content.len());
                
                
                if let Ok(error_count) = error_count.try_into() { // We do this to avoid overflows on 32bit systems
                    let error_count:u64 = error_count;
                    let error_count = error_count * 100;

                    if error_count < max_error_rate {
                        error_rates.push((index, error_count));
                    }
                }
            }

            index += 1;
        }

        // Now we need to find the smallest
        let mut lowest = None;
        for (index, error_rate) in error_rates {
            if let Some((_, prev_errorate)) = lowest {
                if prev_errorate > error_rate {
                    lowest = Some((index, error_rate));
                }
            } else {
                lowest = Some((index, error_rate));
            }
        }

        if let Some((index,_)) = lowest {
            return Ok(index);
        }
        
        // So we couldn't find a match, we add a new file
        return Err(true);
    } else {
        return Err(false); // Something went wrong when reading a file that should exist
    }
}


pub fn process_leftover_folder (store: &mut StorageRepo, left_over_commits: &Vec<OldSub>, item: &PathBuf) -> Result<usize, bool> {
    // Find out what this folder contains
    let content = io::get_folder_content(item.as_path());
    let mut content_detail = Vec::<OldSub>::new();
    for element in content {
        let name = element.file_name().expect("Path does not contain file/folder name, somehow").to_str().expect("a os string is not a str, somehow").to_string();
        
        content_detail.push(
            if element.is_file() {
                if let Ok(hash) = io::hash_file(element.as_path()) {
                    OldSub { id: hash, name, is_folder: false }
                } else {
                    return Err(false);
                }
            } else {
                OldSub { id: U232::new(), name, is_folder: true }
            }
        );
    }

    // Find out what the left_over_commits would contain
    let mut lefty_content_detail = Vec::<(usize, Vec<OldSub>)>::new();
    let mut index:usize = 0;
    for sub in left_over_commits.iter() {
        if sub.is_folder {
            let res = get_old_sub_info(store, sub.id);
            if let Some (new_entry) = res {
                lefty_content_detail.push((index, new_entry));
            } else {
                return Err(false);
            }
        }

        index += 1;
    }

    // If there are no old folders to match with, we will create a new one
    if lefty_content_detail.is_empty() {
        return Err(true);
    }

    // We compare the different old folders to find the best match
    let mut best_match = None;
    for (index, mut sub_content) in lefty_content_detail {
        let mut rate = 0;

        // Definitly not great algorythm, but should do the job
        for item in content_detail.iter() {
            let mut index = 0;
            let mut found = None;

            for sub_item in sub_content.iter() {
                if sub_item.name == item.name {
                    if sub_item.is_folder && item.is_folder {
                        rate += 2;
                        found = Some(index);
                    } else if !sub_item.is_folder && !item.is_folder {
                        if found.is_some() {
                            rate -= 1;
                        }

                        found = Some(index);
                        rate += 2;
                        if sub_item.id == item.id {
                            rate += 1;
                        }
                    }
                    break;
                } else if !sub_item.is_folder && !item.is_folder && sub_item.id == item.id {
                    rate += 1;
                    found = Some(index);
                }

                index += 1;
            }

            if let Some(index) = found {
                sub_content.remove(index);
            }
        }

        if let Some((_, old_rate)) = best_match {
            if old_rate < rate {
                best_match = Some((index, rate));
            }
        } else if let None = best_match {
            best_match = Some((index, rate));
        }
        
    }

    if let Some((index, _)) = best_match {
        return Ok(index);
    } else {
        // strange, whatever, lets just create a new one
        return Err(true);
    }
}

pub fn get_old_sub_info(store: &mut StorageRepo, folder_commit: U232) -> Option<Vec<OldSub>> {
    let commit = if let Ok(f) = store.get_commit(folder_commit) {
        f
    } else {
        return None;
    }.lock().unwrap();
    let old_sub_ids = if let RepoFileType::Folder(commits) = commit.get_type(0x0F) {
        commits.clone()
    } else {
        return None;
    };
    drop(commit);


    let mut old_sub_commits = Vec::<OldSub>::new();

    for item in old_sub_ids {
        let mut is_folder = false;
        let mut name = None;

        let history = store.get_commit_chain(item.clone());
        if history.is_empty() {
            return None; // Something went wrong
        }
        let last = history[0].lock().unwrap();
        if let RepoFileType::Delete = last.get_type(0x05) {
            // this item was deleted on the previous iteration, no need to keep around
        } else {
            for i in history {
                let r_file = i.lock().unwrap();
                if let RepoFileType::NewFolder(n) = r_file.get_type(0x0D) {
                    name = Some(n.clone());
                    is_folder = true;
                    drop(r_file);
                    break;
                }
                if let RepoFileType::Rename(n) = r_file.get_type(0x04) {
                    name = Some(n.clone());
                    is_folder = false;
                    drop(r_file);
                    break;
                }
            }

            if let Some(name) = name {
                old_sub_commits.push(OldSub { id: item, name, is_folder })
            } else {
                return None; // Something is wrong with the old commits
            }
        }
    }

    Some(old_sub_commits)
}

pub fn generate_file_instructions(mut old_data: Vec<u8>, new_data: Vec<u8>) -> RepoFileType {
    // Resizing old_data so we can compare
    if old_data.len() > new_data.len() {
        old_data = old_data[..new_data.len()].to_vec();
    } else if old_data.len() < new_data.len() {
        old_data.append(&mut vec![0_u8; new_data.len() - old_data.len()])
    }

    let diff = io::generate_vec_diff(&old_data, &new_data).expect("They must be the same size, we insured that, didn't we?");

    let mut instructions = Vec::<Instruction>::new();

        
    let pointer_size: usize = ((new_data.len().ilog2()) / 8 + 1).try_into().unwrap();
    let ins_overhead = 1 + pointer_size + 1;
    // Type Byte + Pointer Bytes + Minimum Bytes to define Length

    // Generating instructions, improvements here can severely reduce file size and instruction count, without changing compatibility
    let mut index = 0;
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

        let ins = Instruction::new (
            diff[index].0,
            block.len(),
            op
        );

        // we test the instructions to see if we get the correct result in the end
        ins.run_instruction(&mut old_data);

        instructions.push(ins);

        index = index + add_index; //- 1;
    }

    // Check of the instructions:
    if common::hash_data(new_data.as_slice()) != common::hash_data(old_data.as_slice()) {
        panic!("TODO write error handling for when instructions are incorrectly generating, producing a file that does not match\nTarget Hash:{}\nResulting Hash:{}",common::hash_data(new_data.as_slice()) ,common::hash_data(old_data.as_slice()));
    }

    RepoFileType::Edit(instructions, pointer_size)
}