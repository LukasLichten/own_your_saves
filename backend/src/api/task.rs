use actix_web::{get, post, web::{Data, Json, Payload, Path}, HttpResponse, HttpRequest};
use actix_web_lab::__reexports::{tokio::sync::RwLock, futures_util::StreamExt};
use common::data::{RequestUser, User, TokenCarrier, RequestDevice, Reply, Device, RequestRepository, Repository, AccessType, RepositoryAccess, Branch, Folder, RequestFolder, UploadFile};
use rusqlite::Connection;
use uuid::Uuid;
use crate::{database::{self, AuthHandle}, file_processing::{RepoController, self}};

#[get("/ping")]
pub async fn get_ping() -> Json<String> {
    Json("pong".to_string())
}

#[get("/user/all")]
pub async fn get_all_user(data: Data<Connection>) -> Json<Vec<RequestUser>> {
    let res = database::get_all_users(&data);


    Json(res)
}

#[post("/login")]
pub async fn login(data: Data<Connection>, user: Json<RequestUser>) -> Json<Reply<TokenCarrier>> {
    if let Some(name) = &user.user_name {
        if let Some(password) = user.password {
            let car = if let Some(device_id) = user.device_id {
                database::login(&data, name.clone(), password, device_id)
            } else {
                database::login(&data, name.clone(), password, 0_u8)
            };

            return if let Some(token) = car {
                Json(Reply::new(token))
            } else {
                Json(Reply::AuthFailed)
            };
        }
    }

    Json(Reply::Failed)
}

#[post("/auth")]
pub async fn auth(data: Data<Connection>, token: Json<TokenCarrier>) -> Json<Reply<TokenCarrier>> {
    let auth = database::authenticate(&data, &token);
    if let Some(new_token) = auth {
        if new_token.token != token.token {
            return Json(Reply::new(new_token));
        }

        return Json(Reply::new(new_token));
    }


    Json(Reply::AuthFailed)
}

fn handle_auth_request<T>(data: &Data<Connection>, token: Option<Uuid>) -> Result<AuthHandle, Json<Reply<T>>> {
    if let Some(token) = token {
        if let Some(res) = database::get_auth_handle_from_token(data, token) {
            return Ok(res);
        } else {
            return Err(Json(Reply::AuthFailed));
        }
    } else {
        // No Uuid token
        return Err(Json(Reply::MissingParameter{ token: None }));
    }


    //Err(Json(Reply::Failed))
}

#[post("/user/create")]
pub async fn create_new_user(data: Data<Connection>, user: Json<RequestUser>) -> Json<Reply<()>> {
    if let Some(name) = &user.user_name {
        if let Some(password) = user.password {

            if let Some(admin) = user.admin {
                if admin {
                    // This is a request for creating an admin, so we need to check if there is a logged in user, and if it is an admin
                    let res = handle_auth_request(&data, user.token);
                    if let Ok(handle) = res {
                        if handle.admin {
                            if database::create_user(&data, name.clone(), password, true) {
                                return Json(Reply::Ok { value: (), token: handle.token }); // Normal registration does not auth the current user, this one does, therefore token update
                            }
                        } else {
                            return Json(Reply::Denied { token: handle.token });
                        }

                    } else if let Err(e) = res {
                        return e;
                    }

                    return Json(Reply::Failed);
                }
            }

            if database::create_user(&data, name.clone(), password, false) {
                return Json(Reply::new(()));
            }
        }
    }    

    Json(Reply::Failed)
}

#[post("/user/info")]
pub async fn get_user(data: Data<Connection>, user: Json<RequestUser>) -> Json<Reply<User>> {
    let res = handle_auth_request(&data, user.token);
    if let Ok(handle) = res {
        let target_user_id = if let Some(requested) = user.user_id {
            if handle.admin {
                requested
            } else {
                return Json(Reply::Denied { token: handle.token });
            }
        } else {
            handle.user_id
        };

            
            
        let res = database::get_user(&data, target_user_id);
        if let Some(user) = res {
            return Json(Reply::Ok { value: user, token: handle.token });
        } else {
            return Json(Reply::NotFound { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }
    
    Json(Reply::Failed)
}

#[post("/user/delete")]
pub async fn delete_user(data: Data<Connection>, user: Json<RequestUser>) -> Json<Reply<()>> {
    let res = handle_auth_request(&data, user.token);
    if let Ok(handle) = res {
        let target_user_id = if let Some(requested) = user.user_id {
            if handle.admin {
                requested
            } else {
                return Json(Reply::Denied { token: handle.token });
            }
        } else {
            handle.user_id
        };

        // Checking if the requested user exists
        let res = database::get_user(&data, target_user_id);
        if let None = res {
            return Json(Reply::NotFound { token: handle.token });
        }

        // Actually deleting the user
        if database::delete_user(&data, target_user_id) {
            return if target_user_id != handle.user_id {
                Json(Reply::Ok { value: (), token: handle.token })
            } else {
                Json(Reply::Ok { value: (), token: None })
            };
        } else {
            return Json(Reply::Error { token: handle.token });
        }

    } else if let Err(e) = res {
        return e;
    }
    
    Json(Reply::Failed)
}

#[post("/device/info")]
pub async fn get_device(data: Data<Connection>, device: Json<RequestDevice>) -> Json<Reply<Device>> {
    let res = handle_auth_request(&data, device.token);
    if let Ok(handle) = res {
        let target_user_id = if let Some(requested) = device.user_id {
            if handle.admin {
                requested
            } else {
                return Json(Reply::Denied { token: handle.token });
            }
        } else {
            handle.user_id
        };

        let target_device_id = if let Some(requested) = device.device_id {
            if handle.admin {
                requested
            } else {
                return Json(Reply::Denied { token: handle.token });
            }
        } else if handle.user_id != target_user_id {
            // Different user, but no device ID given, setting to default
            0
        } else {
            handle.device_id
        };
            
        let res = database::get_device(&data, target_user_id, target_device_id);
        if let Some(device) = res {
            return Json(Reply::Ok { value: device, token: handle.token });
        } else {
            return Json(Reply::NotFound { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }


    Json(Reply::Failed)
}

#[post("/device/create")]
pub async fn create_device(data: Data<Connection>, device: Json<RequestDevice>) -> Json<Reply<Device>> {
    let res = handle_auth_request(&data, device.token);
    if let Ok(handle) = res {
        if let Some(device_name) = &device.device_name {
            let target_user_id = if let Some(requested) = device.user_id {
                if handle.admin {
                    requested
                } else {
                    return Json(Reply::Denied { token: handle.token });
                }
            } else {
                handle.user_id
            };
    
            let res = database::create_device(&data, target_user_id, device_name.clone());
            if let Some(device) = res {
                return Json(Reply::Ok { value: device, token: handle.token });
            } else {
                return Json(Reply::NotFound { token: handle.token });
            }
        } else {
            return Json(Reply::MissingParameter{token: handle.token});
        }
    } else if let Err(e) = res {
        return e;
    }


    Json(Reply::Failed)
}

#[post("/device/delete")]
pub async fn delete_device(data: Data<Connection>, device: Json<RequestDevice>) -> Json<Reply<()>> {
    let res = handle_auth_request(&data, device.token);
    if let Ok(mut handle) = res {
        let target_user_id = if let Some(requested) = device.user_id {
            if handle.admin {
                requested
            } else {
                return Json(Reply::Denied { token: handle.token });
            }
        } else {
            handle.user_id
        };

        let target_device_id = if let Some(requested) = device.device_id {
            if handle.admin {
                requested
            } else {
                return Json(Reply::Denied { token: handle.token });
            }
        } else if handle.user_id != target_user_id {
            // Different user, but no device ID given, setting to default
            0
        } else {
            handle.device_id
        };

        if target_device_id == 0 {
            // Deleting 0 is not allowed
            return Json(Reply::Error { token: handle.token })
        }

        if target_user_id == handle.user_id && target_device_id == handle.device_id {
            // we have to log into default device first
            let token = if let Some(tok) = &handle.token {
                tok.token.clone()
            } else if let Some(tok) = device.token {
                tok
            } else {
                Uuid::new_v4() // This should never be called, just the rust compiler needs it
            };

            let res = database::authenticate(&data, &TokenCarrier { token, device_id: Some(0) });
            if let Some(car) = res {
                handle.token = Some(car);
            } else {
                return Json(Reply::Error { token: handle.token });
            }
        }
        
        // Making sure the device actually exists
        let res = database::get_device(&data, target_user_id, target_device_id);
        if let None = res {
            return Json(Reply::NotFound { token: handle.token })
        }

        // Delete
        if database::delete_device(&data, target_user_id, target_device_id) {
            return Json(Reply::Ok { value: (), token: handle.token });
        } else {
            return Json(Reply::Error { token: handle.token })
        }
    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}

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

#[get("/upload/folder")]
pub async fn upload_folder(data: Data<Connection>, request: Json<RequestFolder>) -> Json<Reply<Folder>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        let folder_name = if let Some(name) = &request.folder_name {
            let name = name.trim().to_string();
            if name.is_empty() && request.parent_folder.is_some() {
                return Json(Reply::MissingParameter { token: handle.token }); // Can't have empty folder names for subfolders
            }

            name
        } else if request.parent_folder.is_none() {
            "".to_string()
        } else {
            return Json(Reply::MissingParameter { token: handle.token });
        };

        // Making sure if it is a subfolder, that it does not have the same name as the others
        if let Some(parent_token) = request.parent_folder {
            // Testing against sub folders
            for item in database::get_sub_folders(&data, parent_token) {
                if item.folder_name == folder_name {
                    // We can't have two matching folder names
                    return Json(Reply::Denied { token: handle.token }); // Technically missusing denied, but this function works with any permissions, so...
                }
            }

            // Testing against local files
            if let Some(content) = file_processing::list_temp_folder_content(&data, parent_token) {
                for item in content {
                    if item == folder_name {
                        // Files can not have the same name as a folder
                        return Json(Reply::Denied { token: handle.token });
                    }
                }
            }
        }
        


        let folder = database::create_temp_folder(&data, folder_name);
        if file_processing::create_temp_folder(&data, folder.folder_token) {
            if let Some(parent) = request.parent_folder {
                if !database::link_temp_parent_folder(&data, parent, folder.folder_token) {
                    // Something went wrong in linking, undoing what we did
                    database::delete_temp_folder(&data, folder.folder_token);
                    file_processing::delete_temp_folder(&data, folder.folder_token);

                    return Json(Reply::NotFound { token: handle.token });
                }
            }

            return Json(Reply::Ok { value: folder, token: handle.token });
        } else {
            return Json(Reply::Error { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}

#[post("/upload/file/{folder_token}/{path}")]
pub async fn upload_file(data: Data<Connection>, mut body: Payload, target: Path<UploadFile>) -> HttpResponse {
    let res = file_processing::get_temp_folder_path(&data, target.folder_token);
    if let Some(mut path) = res {
        for item in database::get_sub_folders(&data, target.folder_token) {
            if item.folder_name == target.path {
                // Can't have a file with the same name as a folder
                return HttpResponse::Conflict().finish();
            }
        }

        let mut data = Vec::<u8>::new();

        while let Some(item) = body.next().await {
            if let Ok(item) = item {
                for by in item {
                    data.push(by);
                }
            }
        }

        path.push(target.path.clone());
        if let Ok(_) = file_processing::io::write_bytes(path.as_path(), data) {
            return HttpResponse::Ok().finish()
        } else {
            return HttpResponse::InternalServerError().finish();
        }
    }

    HttpResponse::Gone().finish()
}

#[get("/upload/merge")]
pub async fn merge_folders(data: Data<Connection>, request: Json<RequestFolder>) -> Json<Reply<Folder>> {
    fn recursive_folder_merger(data: &Connection, folder_token: Uuid) -> Result<(),()> {
        if let Some(_folder) = database::get_temp_folder(&data, folder_token) {
            let subs = database::get_sub_folders(&data, folder_token);
            for item in subs {
                // Processes all the subfolders of this one
                recursive_folder_merger(data, item.folder_token)?;

                // Now that it is complete, we will merge it into here
                if !file_processing::merge_temp_folder_into(data, item.folder_token, folder_token, item.folder_name) {
                    return Err(()); // something went wrong in merging
                }

                // Remove the reference from the DB
                database::delete_temp_folder(data, item.folder_token);
            }

            Ok(())
        } else {
            Err(())
        }
    }
    
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        if let Some(folder_token) = request.folder_token {
            if let Some(mut folder) = database::get_temp_folder(&data, folder_token) {
                if recursive_folder_merger(&data, folder_token).is_ok() {
                    folder.content = file_processing::list_temp_folder_content(&data, folder_token);

                    return Json(Reply::Ok { value: folder, token: handle.token });
                } else {
                    // Error in merging folder
                    return Json(Reply::Error { token: handle.token });
                }

            } else {
                // Folder not found
                return Json(Reply::NotFound { token: handle.token });
            }
        } else {
            // No Request token
            return Json(Reply::MissingParameter { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }


    Json(Reply::Failed)
}

#[get("/download/list")]
pub async fn get_download_folder(data: Data<Connection>, request: Json<RequestFolder>) -> Json<Reply<Folder>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        if let Some(folder_token) = request.folder_token {
            if let Some(mut folder) = database::get_temp_folder(&data, folder_token) {
                folder.content = file_processing::list_temp_folder_content(&data, folder_token);

                return Json(Reply::Ok { value: folder, token: handle.token });
            }
        } else {
            return Json(Reply::MissingParameter { token: handle.token });
        }
    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}

#[get("/download")]
pub async fn download(data: Data<Connection>, request: Json<UploadFile>) -> HttpResponse {
    // There is not authentication, maybe we should?
    if let Some(mut folder) = file_processing::get_temp_folder_path(&data, request.folder_token) {
        folder.push(request.path.clone());
        if folder.is_file() {
            if let Ok(data) = file_processing::io::read_bytes(folder.as_path()) {
                return HttpResponse::Ok().body(data);
            } else {
                return HttpResponse::InternalServerError().finish();
            }
        } else if folder.is_dir() {
            return HttpResponse::NoContent().finish(); // Could be a bit confusing to understand that it is a folder
        }
    }

    HttpResponse::Gone().finish()
}

#[get("/download/clear")]
pub async fn clear_temp_folder(data: Data<Connection>, request: Json<RequestFolder>) -> Json<Reply<()>> {
    fn recursive_delete(data: &Connection, folder: Uuid) {
        // Deleting the subs
        for item in database::get_sub_folders(data, folder) {
            recursive_delete(data, item.folder_token);
        }

        database::delete_temp_folder(data, folder);
        file_processing::delete_temp_folder(data, folder);
    }
    
    
    let res = handle_auth_request(&data, request.token);
    if let Ok(handle) = res {
        if let Some(folder) = request.folder_token {
            if let Some(_) = database::get_temp_folder(&data, folder) {
                recursive_delete(&data, folder);
                return Json(Reply::Ok { value: (), token: handle.token });
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

#[get("/placeholder")]
pub async fn placeholder(data: Data<Connection>, request: Json<RequestRepository>) -> Json<Reply<()>> {
    let res = handle_auth_request(&data, request.token);
    if let Ok(_handle) = res {

    } else if let Err(e) = res {
        return e;
    }

    Json(Reply::Failed)
}

#[post("/test")]
pub async fn get_test(dat: Data<RwLock<RepoController>>, req: HttpRequest) -> Json<String> {
    let res = req.cookie("foo");
    if let Some(cookie) = res {

        let _t = HttpResponse::Ok().cookie(cookie.clone()).finish();
        
            

        return Json(cookie.value().to_string());
    }
    
    // let res = req.cookies();
    // if let Ok(list) = res {
    //     for keks in list.iter() {
    //         text.push_str("Key: ");
    //         text.push_str(keks.name());
    //         text.push_str(" Value: ");
    //         text.push_str(keks.value());
    //         text.push_str("\n");
    //     }

    //     return Json(text);
    // }

    // if let Some(t) = dat {
    //     return t;
    // }
    
    

    let res = dat.read().await;
    let _val = res.get_repo(&"Name".to_string()).unwrap();

    Json("".to_string())
}