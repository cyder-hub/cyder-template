use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::{
    app::AppState,
    error::{AppError, AppResult},
    service::items::{self, CreateItemInput, Item},
};

#[derive(Debug, Serialize)]
pub struct DeleteItemResponse {
    pub deleted: bool,
}

pub async fn list_items(State(state): State<AppState>) -> AppResult<Json<Vec<Item>>> {
    items::list(state.database()).map(Json)
}

pub async fn create_item(
    State(state): State<AppState>,
    Json(input): Json<CreateItemInput>,
) -> AppResult<Json<Item>> {
    items::create(state.database(), state.id_generator(), input).map(Json)
}

pub async fn get_item(
    State(state): State<AppState>,
    Path(item_id): Path<i64>,
) -> AppResult<Json<Item>> {
    items::get(state.database(), item_id)?
        .map(Json)
        .ok_or(AppError::NotFound {
            resource: "item",
            id: item_id,
        })
}

pub async fn delete_item(
    State(state): State<AppState>,
    Path(item_id): Path<i64>,
) -> AppResult<Json<DeleteItemResponse>> {
    let deleted = items::delete(state.database(), item_id)?;
    if !deleted {
        return Err(AppError::NotFound {
            resource: "item",
            id: item_id,
        });
    }

    Ok(Json(DeleteItemResponse { deleted }))
}
