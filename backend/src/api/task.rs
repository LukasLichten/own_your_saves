use actix_web::{get, post, web::{Path, Data, Json}, HttpResponse, HttpRequest};
use common::{data::{RequestUser, User, TokenCarrier, RequestDevice, Reply, Device}, LargeU, U256};
use rusqlite::Connection;
use uuid::Uuid;
use crate::database::{self, AuthHandle};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct TaskIdentifier {
    task_global_id: String
}

#[get("/ping")]
pub async fn get_ping() -> Json<String> {
    Json("pong".to_string())
}

#[post("/user/create")]
pub async fn create_new_user(data: Data<Connection>, user: Json<RequestUser>) -> Json<Reply<()>> {
    if let Some(name) = &user.user_name {
        if let Some(password) = user.password {

            if database::create_user(&data, name.clone(), password) {
                return Json(Reply::new(()));
            }
        }
    }

    

    Json(Reply::Failed)
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

#[post("/user/info")]
pub async fn get_user(data: Data<Connection>, user: Json<RequestUser>) -> Json<Reply<User>> {
    let res = handle_auth_request(&data, user.token);
    if let Ok(handle) = res {
        let target_user_id = if let Some(requested) = user.user_id {
            requested // TODO check if requesting user is an admin
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

#[post("/device/info")]
pub async fn get_device(data: Data<Connection>, device: Json<RequestDevice>) -> Json<Reply<Device>> {
    let res = handle_auth_request(&data, device.token);
    if let Ok(handle) = res {
        let target_user_id = if let Some(requested) = device.user_id {
            requested // TODO check if requesting user is an admin
        } else {
            handle.user_id
        };

        let target_device_id = if let Some(request) = device.device_id {
            request
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
                requested // TODO check if requesting user is an admin
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
            requested // TODO check if requesting user is an admin
        } else {
            handle.user_id
        };

        let target_device_id = if let Some(request) = device.device_id {
            request
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

#[post("/test")]
pub async fn get_test(dat: Option<Json<String>>, req: HttpRequest) -> Json<String> {
    

    

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

    if let Some(t) = dat {
        return t;
    }
    Json("".to_string())
}