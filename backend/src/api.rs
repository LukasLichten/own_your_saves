use actix_web::web::{Data, Json};
use common::data::Reply;
use rusqlite::Connection;
use uuid::Uuid;

use crate::database::{self, AuthHandle};

pub mod task;
pub mod user;
pub mod repo;
pub mod transfer;

pub fn handle_auth_request<T>(data: &Data<Connection>, token: Option<Uuid>) -> Result<AuthHandle, Json<Reply<T>>> {
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