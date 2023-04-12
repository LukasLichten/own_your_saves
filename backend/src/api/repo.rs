use actix_web::{web::{Data, Json}, get};
use actix_web_lab::__reexports::tokio::sync::RwLock;
use common::{data::{Reply, RequestRepository, Repository, AccessType, RepositoryAccess, Branch, CreateCommit}, U232, LargeU};
use rusqlite::Connection;

use crate::{database, api::handle_auth_request, file_processing::{RepoController, self, repository_file::CommitInfo}};

#[get("/repo/info")]
pub async fn get_repo(data: Data<Connection>, request: Json<RequestRepository>) -> Json<Reply<Repository>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        if let Some(name) = &request.repo_name {
            // Getting the repo
            let res = database::get_repo(&data, name.clone());
            if let Some(mut rep) = res {
                
                // Checking and setting the availability
                let res = database::get_user_repo_permission(&data, handle.user_id, rep.repo_name.clone());
                if handle.admin {
                    rep.permission = Some(AccessType::All);
                    return Json(Reply::Ok { value: rep, token: handle.token });
                } else if let Some(perm) = res {
                    if perm.is_read_allowed() {
                        rep.permission = Some(perm);

                        return Json(Reply::Ok { value: rep, token: handle.token });
                    } else {
                        return Json(Reply::Denied { token: handle.token });
                    }
                } else {
                    return Json(Reply::Denied { token: handle.token });
                }
            } else {
                return Json(Reply::NotFound { token: handle.token });
            }
        } else {
            return Json(Reply::MissingParameter { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}

#[get("/repo/list")]
pub async fn list_repo(data: Data<Connection>, request: Json<RequestRepository>) -> Json<Reply<Vec<Repository>>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        let data = if handle.admin {
            database::list_repos(&data, None)
        } else {
            database::list_repos(&data, Some(handle.user_id))
        };

        return Json(Reply::Ok { value: data, token: handle.token });
    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}

#[get("/repo/create")]
pub async fn create_repo(repocontroller: Data<RwLock<RepoController>>, data: Data<Connection>, request: Json<RequestRepository>) -> Json<Reply<Repository>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        
        // Adding it to the Database
        let res = database::create_repo(&data, request.clone());
        if let Some(mut rep) = res {

            // 
            let mut repocontroller = repocontroller.write().await;
            if repocontroller.create_repo(rep.repo_name.clone()) {
                drop(repocontroller); // releasing the lock
                database::set_user_repo_permission(&data, handle.user_id, rep.repo_name.clone(), AccessType::Owner);
                rep.permission = Some(AccessType::Owner);

                return Json(Reply::Ok { value: rep, token: handle.token });
            } else {
                // We have to undo the insertion into the DB
                database::delete_repo(&data, rep.repo_name);
                return Json(Reply::Error { token: handle.token })
            }
        } else {
            return Json(Reply::Error { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }


    Json(Reply::Failed)
}

#[get("/repo/delete")]
pub async fn delete_repo(controller: Data<RwLock<RepoController>>, data: Data<Connection>, request: Json<RequestRepository>) -> Json<Reply<()>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        if let Some(repo_name) = &request.repo_name {
            let res = database::get_user_repo_permission(&data, handle.user_id, repo_name.clone());
            if handle.admin {
                // Will have access, irrelevant of what
            } else if let Some(acc) = res {
                if let AccessType::Owner = acc {
                    // Only owner can delete, maybe add All in the future?
                } else {
                    return Json(Reply::Denied { token: handle.token });
                }
            } else if let None = res {
                // Check if exists to send correct responds
                let res = database::get_repo(&data, repo_name.clone());
                if let Some(_) = res {
                    return Json(Reply::Denied { token: handle.token });
                } else {
                    return Json(Reply::NotFound { token: handle.token });
                }
            }

            if database::delete_repo(&data, repo_name.clone()) {
                let mut controller = controller.write().await;
                if controller.delete_repo(repo_name) {
                    return Json(Reply::Ok { value: (), token: handle.token });
                } else {
                    // Undo deletion out of DB
                    controller.reload_folder(&data);
                    return Json(Reply::Error { token: handle.token });
                }
            } else {
                return Json(Reply::Error { token: handle.token });
            }
        } else {
            return Json(Reply::MissingParameter { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}

#[get("/repo/permission/set")]
pub async fn set_repo_access(data: Data<Connection>, request: Json<RepositoryAccess>) -> Json<Reply<()>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        // Check if user exists
        let res = database::get_user(&data, request.user_id);
        if let Some(_other_user) = res {

            //Check if we are allowed to update permission
            let token_res = database::get_user_repo_permission(&data, handle.user_id, request.repo_name.clone());
            let request_res = database::get_user_repo_permission(&data, request.user_id, request.repo_name.clone());

            let allowed =
            if request.user_id == handle.user_id {
                if let Some(this_user) = token_res {
                    if this_user == request.permission {
                        true // No change is permitted
                    } else if let AccessType::Owner = this_user {
                        false // Demoting Owner not permitted
                    } else if let AccessType::All = this_user {
                        if let AccessType::Owner = request.permission {
                            handle.admin // Can't promote to owner, except admin
                        } else {
                            true // Self demotion allowed
                        }
                    } else if let AccessType::No = request.permission { // Careful, this checks what is requested
                        true // Allow self demotion to No access
                    } else {
                       handle.admin //admin may still change their perms
                    }
                } else {
                    false // This user has no rights here
                }
            } else if handle.admin {
                if let Some(other) = request_res {
                    if let AccessType::Owner = other {
                        other == request.permission // You can still not demote owners, but no change is permitted
                    } else {
                        true
                    }
                } else {
                    true
                }
            } else if let Some(this_user) = token_res {
                if let AccessType::Owner = this_user {
                    true
                } else if let AccessType::All = this_user {
                    if let AccessType::Owner = request.permission {
                        false // Can't promote past the current rank
                    } else {
                        true
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if allowed {
                if database::set_user_repo_permission(&data, request.user_id, request.repo_name.clone(), request.permission.clone()) {
                    return Json(Reply::Ok { value: (), token: handle.token })
                }
            } else {
                return Json(Reply::Denied { token: handle.token })
            }
        } else {
            return Json(Reply::NotFound { token: handle.token })
        }
    } else if let Err(e) = res {
        return e;
    }


    Json(Reply::Failed)
}

#[get("/repo/branch/list")]
pub async fn list_branches(controller: Data<RwLock<RepoController>>, data: Data<Connection>, request: Json<RequestRepository>) -> Json<Reply<Vec<Branch>>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        if let Some(repo_name) = &request.repo_name {
            //Checking for access
            let res = database::get_user_repo_permission(&data, handle.user_id, repo_name.clone());
            if handle.admin {
                // Will have access, irrelevant of what
            } else if let Some(acc) = res {
                if let AccessType::No = acc {
                    return Json(Reply::Denied { token: handle.token });
                }
            } else if let None = res {
                // Check if exists to send correct responds
                let res = database::get_repo(&data, repo_name.clone());
                if let Some(_) = res {
                    return Json(Reply::Denied { token: handle.token });
                } else {
                    return Json(Reply::NotFound { token: handle.token });
                }
            }

            // Getting the repo
            let controller = controller.read().await;
            let res = controller.get_repo(repo_name);
            if let Some(repo) = res {
                let repo = repo.lock().unwrap();
                let list = repo.get_branches();

                let mut output = Vec::<Branch>::new();

                for item in list {
                    output.push(
                        Branch { name: item.get_name().clone(), last_commit: item.get_previous_commit() }
                    );
                }
                
                return Json(Reply::Ok { value: output, token: handle.token });
            } else {
                return Json(Reply::NotFound { token: handle.token });
            }
        } else {
            return Json(Reply::MissingParameter { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}

#[get("/repo/commit/create")]
pub async fn create_commit(controller: Data<RwLock<RepoController>>, data: Data<Connection>, request: Json<CreateCommit>) -> Json<Reply<U232>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        if let Some(repo_db) = database::get_repo(&data, request.repo_name.clone()) {
            // Checking if the user is allowed to push
            let access = if handle.admin {
                true
            } else if let Some(perm) = database::get_user_repo_permission(&data, handle.user_id, repo_db.repo_name.clone()) {
                perm.is_write_allowed()
            } else {
                false
            };

            if !access {
                return Json(Reply::Denied { token: handle.token });
            }

            // Checking for the temp folder
            if let (Some(folder),Some(path)) = (database::get_temp_folder(&data, request.folder_token), file_processing::get_temp_folder_path(&data, request.folder_token)) {
                if !database::get_sub_folders(&data, folder.folder_token).is_empty() {
                    // Folder has not been merged completly, aborting
                    return Json(Reply::Error { token: handle.token });
                }

                // Applying the folder_name
                let build_path = if let Some(name) = folder.folder_name {
                    let mut target = path.clone();
                    target.push(name);
                    if file_processing::io::move_folder(path.as_path(), target.as_path()).is_err() {
                        return Json(Reply::Error { token: handle.token });
                    }
                    target
                } else {
                    let content = file_processing::io::get_folder_content(path.as_path());
                    if content.len() == 1 {
                        // Single file commit
                        content[0].clone()
                    } else {
                        path.clone()
                    }
                };

                let conn = controller.read().await;
                if let Some(repo) = conn.get_repo(&repo_db.repo_name) {
                    let mut repo = repo.lock().unwrap();
                    
                    // Checking for the previous commit
                    let previous_commit = if let Some(prev) = request.previous_commit {
                        if prev == U232::new() {
                            None
                        } else if let Err(_) = repo.get_commit(prev) {
                            // Previous commit could not be found
                            drop(repo);
                            drop(conn);
                            return Json(Reply::NotFound { token: handle.token });
                        } else {
                            Some(prev)
                        }
                    } else {
                        None
                    };

                    
                    // Creating the commit
                    if let Some(commit) = repo.create_commit(previous_commit, build_path.as_path(), build_path.eq(&path)) {
                        if previous_commit != Some(commit.clone()) {
                            let time =if let Ok(t) = chrono::Utc::now().timestamp().try_into() {
                                t
                            } else {
                                0
                            };
                            let text = if let Some(text) = &request.commit_message {
                                text.clone()
                            } else {
                                "".to_string()
                            };

                            repo.set_commit_info(commit, CommitInfo::new(handle.user_id, handle.device_id, text, time));
                        }

                        drop(repo);
                        drop(conn);

                        // Cleaning up the temp folder
                        database::delete_temp_folder(&data, folder.folder_token);
                        file_processing::delete_temp_folder(&data, folder.folder_token);
                        // No need to worry about sub folders, as we inforce that there should not be any
                        // although during execution of this command some might have been created
                        // we assume proper usage of the API (high expectations, I know, but this can only be done by the client also running this request, no one else has the folder token)

                        return Json(Reply::Ok { value: commit, token: handle.token });
                    } else {
                        // Something went wrong
                        drop(repo);
                        drop(conn);
                        return Json(Reply::Error { token: handle.token });
                    }
                } else {
                    // Somehow the repo was not found
                    return Json(Reply::NotFound { token: handle.token });
                }

                
            } else {
                return Json(Reply::NotFound { token: handle.token });
            }
        } else {
            return Json(Reply::NotFound { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}