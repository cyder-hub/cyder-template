use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::{
    app::AppState,
    controller::api_id::ApiId,
    error::{AppError, AppResult},
    service::items::{self, CreateItemInput, Item},
};

#[derive(Debug, Serialize)]
pub struct ItemResponse {
    pub id: ApiId,
    pub title: String,
    pub description: String,
    pub completed: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Item> for ItemResponse {
    fn from(item: Item) -> Self {
        Self {
            id: item.id.into(),
            title: item.title,
            description: item.description,
            completed: item.completed,
            created_at: item.created_at,
            updated_at: item.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeleteItemResponse {
    pub deleted: bool,
}

pub async fn list_items(State(state): State<AppState>) -> AppResult<Json<Vec<ItemResponse>>> {
    items::list(state.database())
        .map(|items| items.into_iter().map(ItemResponse::from).collect())
        .map(Json)
}

pub async fn create_item(
    State(state): State<AppState>,
    Json(input): Json<CreateItemInput>,
) -> AppResult<Json<ItemResponse>> {
    items::create(state.database(), state.id_generator(), input)
        .map(ItemResponse::from)
        .map(Json)
}

pub async fn get_item(
    State(state): State<AppState>,
    Path(item_id): Path<ApiId>,
) -> AppResult<Json<ItemResponse>> {
    let item_id = item_id.into_i64();
    items::get(state.database(), item_id)?
        .map(ItemResponse::from)
        .map(Json)
        .ok_or(AppError::NotFound {
            resource: "item",
            id: item_id,
        })
}

pub async fn delete_item(
    State(state): State<AppState>,
    Path(item_id): Path<ApiId>,
) -> AppResult<Json<DeleteItemResponse>> {
    let item_id = item_id.into_i64();
    let deleted = items::delete(state.database(), item_id)?;
    if !deleted {
        return Err(AppError::NotFound {
            resource: "item",
            id: item_id,
        });
    }

    Ok(Json(DeleteItemResponse { deleted }))
}
