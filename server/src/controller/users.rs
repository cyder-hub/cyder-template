use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::{
    app::AppState,
    controller::api_id::ApiId,
    error::{AppError, AppResult},
    service::users::{self, CreateUserInput, User},
};

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: ApiId,
    pub name: String,
    pub email: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.into(),
            name: user.name,
            email: user.email,
            active: user.active,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeleteUserResponse {
    pub deleted: bool,
}

pub async fn list_users(State(state): State<AppState>) -> AppResult<Json<Vec<UserResponse>>> {
    users::list(state.database())
        .map(|users| users.into_iter().map(UserResponse::from).collect())
        .map(Json)
}

pub async fn create_user(
    State(state): State<AppState>,
    Json(input): Json<CreateUserInput>,
) -> AppResult<Json<UserResponse>> {
    users::create(state.database(), state.id_generator(), input)
        .map(UserResponse::from)
        .map(Json)
}

pub async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<ApiId>,
) -> AppResult<Json<UserResponse>> {
    let user_id = user_id.into_i64();
    users::get(state.database(), user_id)?
        .map(UserResponse::from)
        .map(Json)
        .ok_or(AppError::NotFound {
            resource: "user",
            id: user_id,
        })
}

pub async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<ApiId>,
) -> AppResult<Json<DeleteUserResponse>> {
    let user_id = user_id.into_i64();
    let deleted = users::delete(state.database(), user_id)?;
    if !deleted {
        return Err(AppError::NotFound {
            resource: "user",
            id: user_id,
        });
    }

    Ok(Json(DeleteUserResponse { deleted }))
}
