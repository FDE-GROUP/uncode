use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Serialize;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;
use uncode_session::store::{SessionError, SessionStore};

#[derive(Clone)]
struct AppState {
    store: Arc<SessionStore>,
}

#[derive(Serialize)]
struct SessionSummary {
    id: String,
    model: String,
    title: Option<String>,
    message_count: usize,
    working_dir: String,
}

#[derive(Serialize)]
struct SessionDetail {
    id: String,
    model: String,
    title: Option<String>,
    working_dir: String,
    entries: Vec<serde_json::Value>,
}

async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionSummary>>, StatusCode> {
    let sessions = state.store.list_sessions().map_err(|e| {
        tracing::error!("list sessions: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let summaries: Vec<SessionSummary> = sessions
        .into_iter()
        .map(|s| SessionSummary {
            id: s.id,
            model: s.model,
            title: s.title,
            message_count: s.message_count as usize,
            working_dir: s.working_dir,
        })
        .collect();

    Ok(Json(summaries))
}

async fn get_session(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<Json<SessionDetail>, StatusCode> {
    let entries = state.store.load_entries(&session_id).map_err(|e| match e {
        SessionError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    let header = state
        .store
        .read_header(&session_id)
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let json_entries: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or_default())
        .collect();

    Ok(Json(SessionDetail {
        id: header.id,
        model: header.model,
        title: header.title,
        working_dir: header.working_dir,
        entries: json_entries,
    }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let dir = SessionStore::default_dir()?;
    let store = Arc::new(SessionStore::new(dir));
    let state = AppState { store };

    let app = Router::new()
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .fallback_service(ServeDir::new(
            std::env::var("UNCODE_FRONTEND_DIR").unwrap_or_else(|_| "apps/platform/dist".into()),
        ))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    info!("Platform server listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
