use actix_web::{get, post, put, error::PathError, web::{Path, Data, Json}, HttpResponse, http::{header::ContentType, StatusCode}};
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