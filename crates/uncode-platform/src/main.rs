use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;
use uncode_core::message::ContentBlock;
use uncode_core::session::SessionEntry;
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

#[derive(Serialize)]
struct MetricsResponse {
    total_sessions: u64,
    total_messages: u64,
    total_tool_calls: u64,
    tool_success_rate: f64,
    total_input_tokens: u64,
    total_output_tokens: u64,
    avg_messages_per_session: f64,
    models: Vec<ModelStat>,
    recent_sessions: Vec<SessionSummary>,
    tool_usage: Vec<ToolStat>,
}

#[derive(Serialize)]
struct ModelStat {
    model: String,
    count: u64,
}

#[derive(Serialize)]
struct ToolStat {
    name: String,
    calls: u64,
    errors: u64,
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

async fn get_metrics(
    State(state): State<AppState>,
) -> Result<Json<MetricsResponse>, StatusCode> {
    let mut sessions = state.store.list_sessions().map_err(|e| {
        tracing::error!("list sessions for metrics: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let total_sessions = sessions.len() as u64;

    // Aggregate across all sessions
    let mut total_messages: u64 = 0;
    let mut total_tool_calls: u64 = 0;
    let mut total_tool_errors: u64 = 0;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut model_counts: HashMap<String, u64> = HashMap::new();
    let mut tool_counts: HashMap<String, (u64, u64)> = HashMap::new(); // (calls, errors)

    for s in &sessions {
        *model_counts.entry(s.model.clone()).or_default() += 1;

        if let Ok(entries) = state.store.load_entries(&s.id) {
            for entry in &entries {
                if let SessionEntry::Message(me) = entry {
                    total_messages += 1;

                    if let Some(ref usage) = me.usage {
                        total_input_tokens += usage.input_tokens;
                        total_output_tokens += usage.output_tokens;
                    }

                    for block in &me.content {
                        match block {
                            ContentBlock::ToolCall(tc) => {
                                total_tool_calls += 1;
                                tool_counts
                                    .entry(tc.name.clone())
                                    .or_default()
                                    .0 += 1;
                            }
                            ContentBlock::ToolResult(tr) if tr.is_error => {
                                total_tool_errors += 1;
                            }
                            ContentBlock::ToolResult(_) => {}
                            _ => {}
                        }
                    }

                    // Second pass to attribute tool errors by order (match ToolResult to preceding ToolCall)
                    let mut last_tool_name: Option<String> = None;
                    for block in &me.content {
                        match block {
                            ContentBlock::ToolCall(tc) => {
                                last_tool_name = Some(tc.name.clone());
                            }
                            ContentBlock::ToolResult(tr) => {
                                if tr.is_error {
                                    if let Some(name) = &last_tool_name {
                                        tool_counts.entry(name.clone()).or_default().1 += 1;
                                    }
                                }
                                last_tool_name = None;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    let tool_success_rate = if total_tool_calls > 0 {
        (total_tool_calls - total_tool_errors) as f64 / total_tool_calls as f64
    } else {
        1.0
    };

    let avg_messages = if total_sessions > 0 {
        total_messages as f64 / total_sessions as f64
    } else {
        0.0
    };

    // Recent 10 sessions (sorted by updated_at desc)
    sessions.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
    let recent_sessions: Vec<SessionSummary> = sessions
        .iter()
        .take(10)
        .map(|s| SessionSummary {
            id: s.id.clone(),
            model: s.model.clone(),
            title: s.title.clone(),
            message_count: s.message_count as usize,
            working_dir: s.working_dir.clone(),
        })
        .collect();

    let models: Vec<ModelStat> = model_counts
        .into_iter()
        .map(|(model, count)| ModelStat { model, count })
        .collect();

    let tool_usage: Vec<ToolStat> = tool_counts
        .into_iter()
        .map(|(name, (calls, errors))| ToolStat { name, calls, errors })
        .collect();

    Ok(Json(MetricsResponse {
        total_sessions,
        total_messages,
        total_tool_calls,
        tool_success_rate,
        total_input_tokens,
        total_output_tokens,
        avg_messages_per_session: avg_messages,
        models,
        recent_sessions,
        tool_usage,
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
        .route("/api/metrics", get(get_metrics))
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
