use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

use crate::{
    Service,
    contract::{
        API_VERSION, ApiError, Approval, Envelope, EventPage, Result, Submission, TaskView,
    },
};

#[derive(Clone)]
struct HttpState {
    service: Service,
    requests: Arc<Semaphore>,
    polls: Arc<Semaphore>,
}

/// Build the authenticated version 1 API. All routes use the same service owner.
pub fn router(service: Service) -> Router {
    let state = HttpState {
        service,
        requests: Arc::new(Semaphore::new(16)),
        polls: Arc::new(Semaphore::new(8)),
    };
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/capabilities", get(capabilities))
        .route("/v1/shutdown", post(shutdown))
        .route("/v1/tasks", get(list).post(submit))
        .route("/v1/tasks/{id}", get(snapshot))
        .route("/v1/tasks/{id}/events", get(events))
        .route("/v1/tasks/{id}/approval", post(approve))
        .route("/v1/tasks/{id}/cancel", post(cancel))
        .layer(DefaultBodyLimit::max(16_384))
        .layer(middleware::from_fn_with_state(state.clone(), authorize))
        .with_state(state)
}

async fn authorize(State(state): State<HttpState>, request: Request, next: Next) -> Response {
    if request.headers().contains_key("origin") {
        return ApiError::new("forbidden", "browser-origin requests are not enabled")
            .into_response();
    }
    let supplied = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !bool::from(supplied.as_bytes().ct_eq(state.service.token().as_bytes())) {
        return ApiError::new("unauthorized", "a valid service bearer token is required")
            .into_response();
    }
    let Ok(_permit) = state.requests.clone().try_acquire_owned() else {
        return ApiError::new("capacity", "too many concurrent service requests").into_response();
    };
    next.run(request).await
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.code.as_str() {
            "unauthorized" => StatusCode::UNAUTHORIZED,
            "forbidden" => StatusCode::FORBIDDEN,
            "not_found" => StatusCode::NOT_FOUND,
            "conflict" | "cursor_ahead" => StatusCode::CONFLICT,
            "capacity" => StatusCode::TOO_MANY_REQUESTS,
            "invalid_request" => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(Envelope::new(self))).into_response()
    }
}

fn version<T>(request: Envelope<T>) -> Result<T> {
    if request.schema_version != API_VERSION {
        return Err(ApiError::invalid("unsupported API version"));
    }
    Ok(request.data)
}
async fn capabilities() -> Json<Envelope<Vec<&'static str>>> {
    Json(Envelope::new(vec!["conversation-v1"]))
}
async fn health() -> Json<Envelope<&'static str>> {
    Json(Envelope::new("ready"))
}
async fn shutdown(
    State(state): State<HttpState>,
    Json(request): Json<Envelope<()>>,
) -> Result<Json<Envelope<()>>> {
    version(request)?;
    state.service.shutdown_signal().cancel();
    Ok(Json(Envelope::new(())))
}
async fn list(
    State(state): State<HttpState>,
) -> Result<Json<Envelope<Vec<crate::contract::TaskSummary>>>> {
    Ok(Json(Envelope::new(state.service.list().await?)))
}
async fn submit(
    State(state): State<HttpState>,
    Json(request): Json<Envelope<Submission>>,
) -> Result<Json<Envelope<TaskView>>> {
    Ok(Json(Envelope::new(
        state.service.submit(version(request)?).await?,
    )))
}
async fn snapshot(
    State(state): State<HttpState>,
    Path(id): Path<String>,
) -> Result<Json<Envelope<TaskView>>> {
    Ok(Json(Envelope::new(state.service.snapshot(&id).await?)))
}
async fn approve(
    State(state): State<HttpState>,
    Path(id): Path<String>,
    Json(request): Json<Envelope<Approval>>,
) -> Result<Json<Envelope<TaskView>>> {
    Ok(Json(Envelope::new(
        state.service.approve(&id, version(request)?).await?,
    )))
}
async fn cancel(
    State(state): State<HttpState>,
    Path(id): Path<String>,
    Json(request): Json<Envelope<()>>,
) -> Result<Json<Envelope<TaskView>>> {
    version(request)?;
    Ok(Json(Envelope::new(state.service.cancel(&id).await?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    #[serde(default)]
    after: u64,
    #[serde(default)]
    wait_ms: u64,
}
async fn events(
    State(state): State<HttpState>,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<Envelope<EventPage>>> {
    let _permit = state
        .polls
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::new("capacity", "too many event consumers"))?;
    Ok(Json(Envelope::new(
        state
            .service
            .events(&id, cursor.after, cursor.wait_ms)
            .await?,
    )))
}
