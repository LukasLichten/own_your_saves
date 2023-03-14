use actix_web::{get, post, web::{Path, Data, Json}, HttpResponse};
use common::{RequestUser, LargeU, U256};
use rusqlite::Connection;
use crate::database;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct TaskIdentifier {
    task_global_id: String
}

#[get("/task/{task_global_id}")]
pub async fn get_task(task_id: Path<TaskIdentifier>) -> Json<String> {
    Json(task_id.into_inner().task_global_id)
}

#[get("/ping")]
pub async fn get_ping() -> Json<String> {
    Json("pong".to_string())
}

#[post("/user/create")]
pub async fn post_new_user(data: Data<Connection>, user: Json<RequestUser>) -> HttpResponse {
    if let Some(name) = &user.user_name {
        if let Some(password) = &user.password {
            let password = U256::from_u8arr(common::hex_string_to_bytes(&password).as_slice());

            if database::create_user(&data, name.clone(), password) {
                return HttpResponse::Accepted().finish();
            }
        } else {
            return HttpResponse::BadRequest().body("Deserialization Error: password field is required");
        }
    } else {
        return HttpResponse::BadRequest().body("Deserialization Error: user_name field is required");
    }

    

    HttpResponse::BadRequest().finish()
}

#[get("/user/all")]
pub async fn get_all_user(data: Data<Connection>) -> Json<Vec<RequestUser>> {
    let res = database::get_all_users(&data);


    Json(res)
}

