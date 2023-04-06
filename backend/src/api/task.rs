use actix_web::{get, post, web::{Data, Json}, HttpResponse, HttpRequest};
use actix_web_lab::__reexports::{tokio::sync::RwLock};
use common::data::{RequestUser, Reply, RequestRepository};
use rusqlite::Connection;
use crate::{database, file_processing::RepoController, api::handle_auth_request};

#[get("/ping")]
pub async fn get_ping() -> Json<String> {
    Json("pong".to_string())
}

#[get("/user/all")]
pub async fn get_all_user(data: Data<Connection>) -> Json<Vec<RequestUser>> {
    let res = database::get_all_users(&data);


    Json(res)
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