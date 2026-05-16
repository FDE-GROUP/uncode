use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::{Json, Router, extract::State, http::StatusCode, routing::get, routing::post};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;
use uncode_core::message::ContentBlock;
use uncode_core::session::SessionEntry;
use uncode_session::store::{SessionError, SessionStore};

#[derive(Deserialize)]
struct SessionQuery {
    #[serde(default)]
    search: Option<String>,
    #[serde(default = "default_sort")]
    sort: String,
    #[serde(default = "default_order")]
    order: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_sort() -> String {
    "updated_at".into()
}
fn default_order() -> String {
    "desc".into()
}
fn default_limit() -> usize {
    50
}

const EVENT_CHANNEL_CAPACITY: usize = 256;

#[derive(Clone)]
struct AppState {
    store: Arc<SessionStore>,
    event_tx: broadcast::Sender<serde_json::Value>,
}

// ── Response types ──────────────────────────────────────────────

#[derive(Serialize)]
struct SessionSummary {
    id: String,
    model: String,
    title: Option<String>,
    message_count: usize,
    working_dir: String,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
struct SessionListResponse {
    items: Vec<SessionSummary>,
    total: usize,
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

#[derive(Serialize)]
struct SessionMetricsResponse {
    total_messages: u64,
    total_tool_calls: u64,
    tool_errors: u64,
    input_tokens: u64,
    output_tokens: u64,
    duration_secs: f64,
    tools: Vec<ToolStat>,
    files_modified: Vec<String>,
}

// ── REST handlers ───────────────────────────────────────────────

async fn list_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<SessionQuery>,
) -> Result<Json<SessionListResponse>, StatusCode> {
    let sessions = state.store.list_sessions().map_err(|e| {
        tracing::error!("list sessions: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut filtered: Vec<_> = if let Some(ref search) = query.search {
        let keyword = search.to_lowercase();
        sessions
            .into_iter()
            .filter(|s| {
                s.model.to_lowercase().contains(&keyword)
                    || s.title
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&keyword)
                    || s.id.contains(&keyword)
            })
            .collect()
    } else {
        sessions
    };

    let total = filtered.len();

    match query.sort.as_str() {
        "created_at" => filtered.sort_by(|a, b| {
            let cmp = a.created_at.cmp(&b.created_at);
            if query.order == "asc" {
                cmp
            } else {
                cmp.reverse()
            }
        }),
        "message_count" => filtered.sort_by(|a, b| {
            let cmp = a.message_count.cmp(&b.message_count);
            if query.order == "asc" {
                cmp
            } else {
                cmp.reverse()
            }
        }),
        _ => filtered.sort_by(|a, b| {
            let cmp = a.updated_at.cmp(&b.updated_at);
            if query.order == "asc" {
                cmp
            } else {
                cmp.reverse()
            }
        }),
    }

    let items: Vec<SessionSummary> = filtered
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(|s| SessionSummary {
            id: s.id,
            model: s.model,
            title: s.title,
            message_count: s.message_count as usize,
            working_dir: s.working_dir,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(SessionListResponse { items, total }))
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

async fn get_session_metrics(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<Json<SessionMetricsResponse>, StatusCode> {
    let header = state
        .store
        .read_header(&session_id)
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let entries = state.store.load_entries(&session_id).map_err(|e| match e {
        SessionError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    })?;

    let mut total_messages: u64 = 0;
    let mut total_tool_calls: u64 = 0;
    let mut tool_errors: u64 = 0;
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut tool_counts: HashMap<String, (u64, u64)> = HashMap::with_capacity(8);
    let mut files_modified: HashSet<String> = HashSet::new();

    for entry in &entries {
        if let SessionEntry::Message(me) = entry {
            total_messages += 1;
            if let Some(ref usage) = me.usage {
                input_tokens += usage.input_tokens;
                output_tokens += usage.output_tokens;
            }

            let mut last_tool_name: Option<&str> = None;
            for block in &me.content {
                match block {
                    ContentBlock::ToolCall(tc) => {
                        total_tool_calls += 1;
                        tool_counts.entry(tc.name.clone()).or_default().0 += 1;
                        last_tool_name = Some(&tc.name);

                        if tc.name == "write" || tc.name == "edit" {
                            if let Some(path) = tc.arguments.get("path").and_then(|v| v.as_str()) {
                                files_modified.insert(path.to_string());
                            }
                        }
                    }
                    ContentBlock::ToolResult(tr) => {
                        if tr.is_error {
                            tool_errors += 1;
                            if let Some(name) = last_tool_name {
                                tool_counts.entry(name.to_string()).or_default().1 += 1;
                            }
                        }
                        last_tool_name = None;
                    }
                    _ => {}
                }
            }
        }
    }

    let duration_secs = (header.updated_at - header.created_at).num_seconds().max(0) as f64;

    let tools: Vec<ToolStat> = tool_counts
        .into_iter()
        .map(|(name, (calls, errors))| ToolStat {
            name,
            calls,
            errors,
        })
        .collect();

    Ok(Json(SessionMetricsResponse {
        total_messages,
        total_tool_calls,
        tool_errors,
        input_tokens,
        output_tokens,
        duration_secs,
        tools,
        files_modified: files_modified.into_iter().collect(),
    }))
}

async fn get_metrics(State(state): State<AppState>) -> Result<Json<MetricsResponse>, StatusCode> {
    let mut sessions = state.store.list_sessions().map_err(|e| {
        tracing::error!("list sessions for metrics: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let total_sessions = sessions.len() as u64;

    let mut total_messages: u64 = 0;
    let mut total_tool_calls: u64 = 0;
    let mut total_tool_errors: u64 = 0;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut model_counts: HashMap<String, u64> = HashMap::with_capacity(4);
    let mut tool_counts: HashMap<String, (u64, u64)> = HashMap::with_capacity(8);

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

                    let mut last_tool_name: Option<&str> = None;
                    for block in &me.content {
                        match block {
                            ContentBlock::ToolCall(tc) => {
                                total_tool_calls += 1;
                                tool_counts.entry(tc.name.clone()).or_default().0 += 1;
                                last_tool_name = Some(&tc.name);
                            }
                            ContentBlock::ToolResult(tr) if tr.is_error => {
                                total_tool_errors += 1;
                                if let Some(name) = last_tool_name {
                                    tool_counts.entry(name.to_string()).or_default().1 += 1;
                                }
                            }
                            ContentBlock::ToolResult(_) => {}
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
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        })
        .collect();

    let models: Vec<ModelStat> = model_counts
        .into_iter()
        .map(|(model, count)| ModelStat { model, count })
        .collect();

    let tool_usage: Vec<ToolStat> = tool_counts
        .into_iter()
        .map(|(name, (calls, errors))| ToolStat {
            name,
            calls,
            errors,
        })
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

// ── Event ingestion (POST /api/events) ──────────────────────────

#[derive(Deserialize)]
struct EventPayload {
    event: serde_json::Value,
}

async fn post_event(
    State(state): State<AppState>,
    Json(payload): Json<EventPayload>,
) -> StatusCode {
    let _ = state.event_tx.send(payload.event);
    StatusCode::OK
}

// ── Optimization suggestions ────────────────────────────────────

#[derive(Serialize)]
struct Suggestion {
    category: String,
    severity: String, // "high", "medium", "low"
    title: String,
    description: String,
    detail: String,
}

#[derive(Serialize)]
struct SuggestionsResponse {
    suggestions: Vec<Suggestion>,
}

async fn get_suggestions(
    State(state): State<AppState>,
) -> Result<Json<SuggestionsResponse>, StatusCode> {
    let sessions = state.store.list_sessions().map_err(|e| {
        tracing::error!("list sessions for suggestions: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut suggestions: Vec<Suggestion> = Vec::new();

    // Per-tool stats
    let mut tool_calls: HashMap<String, u64> = HashMap::with_capacity(8);
    let mut tool_errors: HashMap<String, u64> = HashMap::with_capacity(8);
    // Per-session read counts per file
    let mut file_reads: HashMap<String, u64> = HashMap::with_capacity(16);
    // Token totals
    let mut total_input: u64 = 0;
    let mut total_output: u64 = 0;
    let mut session_count_with_tokens: u64 = 0;
    // High-turn sessions
    let mut high_turn_sessions: Vec<String> = Vec::new();

    for s in &sessions {
        if let Ok(entries) = state.store.load_entries(&s.id) {
            let mut session_input: u64 = 0;
            let mut session_output: u64 = 0;
            let mut turn_count: u64 = 0;

            for entry in &entries {
                if let SessionEntry::Message(me) = entry {
                    turn_count += 1;
                    if let Some(ref usage) = me.usage {
                        session_input += usage.input_tokens;
                        session_output += usage.output_tokens;
                    }
                    for block in &me.content {
                        match block {
                            ContentBlock::ToolCall(tc) => {
                                *tool_calls.entry(tc.name.clone()).or_default() += 1;
                                if tc.name == "read" {
                                    if let Some(path) =
                                        tc.arguments.get("path").and_then(|v| v.as_str())
                                    {
                                        *file_reads.entry(path.to_string()).or_default() += 1;
                                    }
                                }
                            }
                            ContentBlock::ToolResult(tr) if tr.is_error => {
                                let _ = tr;
                            }
                            _ => {}
                        }
                    }

                    // Attribute errors to tools by order
                    let mut last_tool: Option<&str> = None;
                    for block in &me.content {
                        match block {
                            ContentBlock::ToolCall(tc) => {
                                last_tool = Some(&tc.name);
                            }
                            ContentBlock::ToolResult(tr) if tr.is_error => {
                                if let Some(name) = last_tool {
                                    *tool_errors.entry(name.to_string()).or_default() += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            total_input += session_input;
            total_output += session_output;
            if session_input > 0 || session_output > 0 {
                session_count_with_tokens += 1;
            }

            if turn_count > 20 {
                high_turn_sessions.push(
                    s.title
                        .clone()
                        .unwrap_or_else(|| s.id.chars().take(8).collect()),
                );
            }
        }
    }

    // ── Rule 1: Unstable tools (error rate > 15%) ──
    for (tool, calls) in &tool_calls {
        let errors = tool_errors.get(tool).copied().unwrap_or(0);
        if *calls >= 3 && errors > 0 {
            let rate = errors as f64 / *calls as f64;
            if rate > 0.15 {
                suggestions.push(Suggestion {
                    category: "tool_stability".into(),
                    severity: if rate > 0.4 { "high" } else { "medium" }.into(),
                    title: format!("工具 {tool} 错误率偏高 ({:.0}%)", rate * 100.0),
                    description: format!(
                        "{tool} 被调用 {calls} 次，其中 {errors} 次失败。检查工具实现或添加更好的错误处理。",
                    ),
                    detail: format!("error_rate={:.2},calls={calls},errors={errors}", rate),
                });
            }
        }
    }

    // ── Rule 2: Repeated file reads (>5 reads of same file across sessions) ──
    let mut repeated_files: Vec<_> = file_reads.iter().filter(|(_, count)| **count > 5).collect();
    repeated_files.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (path, count) in repeated_files.iter().take(3) {
        suggestions.push(Suggestion {
            category: "context_efficiency".into(),
            severity: "medium".into(),
            title: format!("文件 {path} 被反复读取 ({count} 次)"),
            description: "Agent 频繁读取同一文件，可能是上下文窗口不足导致信息丢失。建议增大上下文或添加关键文件到系统提示。".into(),
            detail: format!("reads={count}"),
        });
    }

    // ── Rule 3: High-turn sessions (>20 turns) ──
    if !high_turn_sessions.is_empty() {
        suggestions.push(Suggestion {
            category: "agent_efficiency".into(),
            severity: "low".into(),
            title: format!("{} 个会话超过 20 轮对话", high_turn_sessions.len()),
            description: format!(
                "长会话: {}。高轮次可能表示 Agent 需要更明确的指令或更好的上下文管理。",
                high_turn_sessions.join("、")
            ),
            detail: format!("count={}", high_turn_sessions.len()),
        });
    }

    // ── Rule 4: Token efficiency ──
    if session_count_with_tokens > 0 {
        let avg_tokens = (total_input + total_output) as f64 / session_count_with_tokens as f64;
        if avg_tokens > 50_000.0 {
            suggestions.push(Suggestion {
                category: "cost_control".into(),
                severity: "medium".into(),
                title: format!("平均 Token 消耗较高 ({:.0}/会话)", avg_tokens),
                description: "会话平均消耗超过 50K tokens。建议优化系统提示长度、启用上下文压缩、或减少工具返回数据量。".into(),
                detail: format!(
                    "avg_tokens={avg_tokens:.0},total_input={total_input},total_output={total_output}"
                ),
            });
        }
    }

    // ── Rule 5: No tool usage at all ──
    let total_calls: u64 = tool_calls.values().sum();
    if sessions.len() > 3 && total_calls == 0 {
        suggestions.push(Suggestion {
            category: "agent_capability".into(),
            severity: "low".into(),
            title: "Agent 未使用任何工具".into(),
            description:
                "检测到多个会话但没有工具调用。确认 Agent 配置了正确的工具权限和系统提示。".into(),
            detail: format!("sessions={},tool_calls=0", sessions.len()),
        });
    }

    // Sort by severity
    let severity_order = |s: &str| match s {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    };
    suggestions.sort_by_key(|s| severity_order(&s.severity));

    Ok(Json(SuggestionsResponse { suggestions }))
}

// ── Settings ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct SettingsResponse {
    data_dir: String,
    version: String,
    github_repo: String,
    has_github_token: bool,
}

async fn get_settings(State(_state): State<AppState>) -> Json<SettingsResponse> {
    let data_dir_str = format!(
        "{}",
        dirs::data_dir()
            .unwrap_or_default()
            .join("uncode")
            .join("sessions")
            .display()
    );

    Json(SettingsResponse {
        data_dir: data_dir_str,
        version: env!("CARGO_PKG_VERSION").to_string(),
        github_repo: std::env::var("UNCODE_GITHUB_REPO")
            .unwrap_or_else(|_| "FDE-GROUP/uncode".into()),
        has_github_token: std::env::var("UNCODE_GITHUB_TOKEN").is_ok(),
    })
}

// ── GitHub Issues proxy ──────────────────────────────────────────

#[derive(Deserialize)]
struct IssuesQuery {
    #[serde(default = "default_issues_state")]
    state: String,
    #[serde(default = "default_issues_per_page")]
    per_page: u32,
}

fn default_issues_state() -> String {
    "open".into()
}
fn default_issues_per_page() -> u32 {
    30
}

async fn list_issues(
    axum::extract::Query(query): axum::extract::Query<IssuesQuery>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let repo = std::env::var("UNCODE_GITHUB_REPO").unwrap_or_else(|_| "FDE-GROUP/uncode".into());

    let url = format!(
        "https://api.github.com/repos/{repo}/issues?state={}&per_page={}&sort=updated",
        query.state, query.per_page
    );

    let client = reqwest::Client::builder()
        .user_agent("uncode-platform")
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut request = client.get(&url);
    if let Ok(token) = std::env::var("UNCODE_GITHUB_TOKEN") {
        request = request.bearer_auth(&token);
    }

    match request.send().await {
        Ok(resp) => {
            let issues: Vec<serde_json::Value> = resp.json().await.unwrap_or_default();
            Ok(Json(issues))
        }
        Err(e) => {
            tracing::warn!("GitHub API error: {e}");
            Ok(Json(vec![]))
        }
    }
}

async fn get_issue(
    axum::extract::Path(number): axum::extract::Path<u32>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = std::env::var("UNCODE_GITHUB_REPO").unwrap_or_else(|_| "FDE-GROUP/uncode".into());

    let url = format!("https://api.github.com/repos/{repo}/issues/{number}");

    let client = reqwest::Client::builder()
        .user_agent("uncode-platform")
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut request = client.get(&url);
    if let Ok(token) = std::env::var("UNCODE_GITHUB_TOKEN") {
        request = request.bearer_auth(&token);
    }

    match request.send().await {
        Ok(resp) if resp.status().is_success() => {
            let issue: serde_json::Value = resp.json().await.unwrap_or_default();
            Ok(Json(issue))
        }
        Ok(resp) => {
            tracing::warn!("GitHub API returned {}", resp.status());
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            tracing::warn!("GitHub API error: {e}");
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

// ── Main ────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let mut host = "127.0.0.1".to_string();
    let mut port: u16 = 3000;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => {
                if let Some(h) = args.next() {
                    host = h;
                }
            }
            "--port" => {
                if let Some(p) = args.next() {
                    port = p.parse().unwrap_or(3000);
                }
            }
            _ => {}
        }
    }

    let dir = SessionStore::default_dir()?;
    let store = Arc::new(SessionStore::new(dir));

    let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

    let state = AppState { store, event_tx };

    let app = Router::new()
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/sessions/{id}/metrics", get(get_session_metrics))
        .route("/api/metrics", get(get_metrics))
        .route("/api/suggestions", get(get_suggestions))
        .route("/api/settings", get(get_settings))
        .route("/api/issues", get(list_issues))
        .route("/api/issues/{number}", get(get_issue))
        .route("/api/events", post(post_event))
        .route("/ws/events", get(ws_events_handler))
        .fallback_service(ServeDir::new(
            std::env::var("UNCODE_FRONTEND_DIR").unwrap_or_else(|_| "apps/platform/dist".into()),
        ))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Platform server listening on http://{addr}");
    info!("WebSocket endpoint: ws://{addr}/ws/events");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn ws_events_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state.event_tx.clone()))
}

async fn handle_ws_connection(socket: WebSocket, event_tx: broadcast::Sender<serde_json::Value>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let mut rx = event_tx.subscribe();

    // Forward broadcast events to WebSocket client
    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let json = match serde_json::to_string(&event) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if ws_sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Read incoming messages (keep-alive / close)
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(Message::Ping(data)) => {
                    // Pong is auto-sent by axum
                    let _ = data;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}
