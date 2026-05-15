use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    service::users::{self, CreateUserInput, User},
};

#[derive(Debug, Serialize)]
pub struct DeleteUserResponse {
    pub deleted: bool,
}

pub async fn list_users(State(state): State<AppState>) -> AppResult<Json<Vec<User>>> {
    users::list(state.database()).map(Json)
}

pub async fn create_user(
    State(state): State<AppState>,
    Json(input): Json<CreateUserInput>,
) -> AppResult<Json<User>> {
    users::create(state.database(), state.id_generator(), input).map(Json)
}

pub async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
) -> AppResult<Json<User>> {
    users::get(state.database(), user_id)?
        .map(Json)
        .ok_or(AppError::NotFound {
            resource: "user",
            id: user_id,
        })
}

pub async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
) -> AppResult<Json<DeleteUserResponse>> {
    let deleted = users::delete(state.database(), user_id)?;
    if !deleted {
        return Err(AppError::NotFound {
            resource: "user",
            id: user_id,
        });
    }

    Ok(Json(DeleteUserResponse { deleted }))
}
