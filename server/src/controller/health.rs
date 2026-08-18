use axum::{Json, extract::State};
use serde::Serialize;

use crate::{app::AppState, database, error::AppResult};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
}

pub async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: crate::app::APP_NAME,
    })
}

#[derive(Debug, Serialize)]
pub struct ReadyResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub database: database::DatabaseHealth,
}

pub async fn readyz(State(state): State<AppState>) -> AppResult<Json<ReadyResponse>> {
    ensure_accepting_traffic(&state)?;
    let database = database::check_readiness(state.database()).await;
    ensure_accepting_traffic(&state)?;
    let database = database?;

    Ok(Json(ReadyResponse {
        status: "ready",
        service: crate::app::APP_NAME,
        database,
    }))
}

fn ensure_accepting_traffic(state: &AppState) -> AppResult<()> {
    if state.lifecycle().is_ready() {
        Ok(())
    } else {
        Err(crate::error::AppError::Readiness {
            message: "service is shutting down".to_string(),
        })
    }
}
