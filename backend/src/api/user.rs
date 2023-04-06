use actix_web::{web::{Data, Json}, get};
use common::data::{RequestUser, Reply, TokenCarrier, User, RequestDevice, Device};
use rusqlite::Connection;
use uuid::Uuid;

use crate::{database, api::handle_auth_request};

#[get("/login")]
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

#[get("/auth")]
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



#[get("/user/create")]
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

#[get("/user/info")]
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

#[get("/user/delete")]
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

#[get("/device/info")]
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

#[get("/device/create")]
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

#[get("/device/delete")]
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