use actix_web::{web::{Data, Json, Payload, Path}, get, post, HttpResponse};
use actix_web_lab::__reexports::futures_util::StreamExt;
use common::data::{Reply, RequestFolder, Folder, UploadFile};
use rusqlite::Connection;
use uuid::Uuid;

use crate::{database, api::handle_auth_request, file_processing};

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